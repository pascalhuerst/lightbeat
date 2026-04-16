//! Input controllers: the user-configurable bridge between physical input
//! devices (MIDI, later: keyboard/HID/joystick) and the node graph.
//!
//! Design:
//! - An `InputController` is a persistent virtual slot with a kind-specific
//!   binding (e.g. a MIDI port name to match). Connection to the hardware is
//!   lazy/reconnectable — if the hardware is absent, the virtual controller
//!   still exists and emits 0s.
//! - Each controller owns a list of `LearnedInput`s that route specific
//!   incoming events (MIDI CC #7, MIDI Note 60, ...) to a named output.
//! - Per-input `InputBindingMode` (Value / TriggerOnPress / TriggerOnRelease)
//!   is applied by the engine node when reading, so the shared state can stay
//!   minimal (just the "raw current value").
//!
//! Threading:
//! - The midir backend callback runs on midir's internal thread. It writes
//!   into `ControllerRuntime::values` under a short Mutex lock.
//! - A reconnect worker polls port availability ~1Hz.
//! - The engine thread reads values; the UI thread reads values + consumes
//!   the learn buffer.

pub mod midi;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use self::midi::{MidiSession, MidiSource};

// ---------------------------------------------------------------------------
// Persistent types (stored in SetupFile)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputController {
    pub id: u32,
    pub name: String,
    pub kind: InputControllerKind,
    #[serde(default)]
    pub inputs: Vec<LearnedInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InputControllerKind {
    Midi {
        /// Hardware port display name (matched against system-enumerated
        /// ports at reconnect time). Empty = no mapping.
        hw_port_name: String,
    },
}

impl InputControllerKind {
    pub fn label(&self) -> &'static str {
        match self {
            InputControllerKind::Midi { .. } => "MIDI",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearnedInput {
    pub id: u32,
    pub name: String,
    pub source: InputSource,
    #[serde(default = "default_binding_mode")]
    pub mode: InputBindingMode,
}

fn default_binding_mode() -> InputBindingMode { InputBindingMode::Value }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InputSource {
    Midi(MidiSource),
}

impl InputSource {
    /// True if this source produces a discrete on/off state (vs. a continuous
    /// 0..1 value). Determines which modes are meaningful.
    pub fn is_binary(&self) -> bool {
        match self {
            InputSource::Midi(m) => m.is_binary(),
        }
    }

    pub fn label(&self) -> String {
        match self {
            InputSource::Midi(m) => m.label(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputBindingMode {
    /// Continuous value, or "1 while held" for binary sources.
    Value,
    /// 1.0 pulse for one engine tick on activation (binary only).
    TriggerOnPress,
    /// 1.0 pulse for one engine tick on deactivation (binary only).
    TriggerOnRelease,
}

impl InputBindingMode {
    pub fn label(&self) -> &'static str {
        match self {
            InputBindingMode::Value => "Value",
            InputBindingMode::TriggerOnPress => "Trigger on Press",
            InputBindingMode::TriggerOnRelease => "Trigger on Release",
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime shared state
// ---------------------------------------------------------------------------

/// Per-controller live state. Shared between midir callback, engine, and UI.
pub struct ControllerRuntime {
    pub id: u32,
    pub name: String,
    pub kind: InputControllerKind,
    pub inputs: Vec<LearnedInput>,
    /// One entry per input, same order as `inputs`. Raw current value:
    /// continuous 0..1, or 0.0/1.0 for binary (1 while held).
    pub values: Vec<f32>,
    /// Connection status for UI badges.
    pub status: ConnectionStatus,
    /// When Some, incoming raw events are captured into `learn_buffer` for
    /// the UI to pick the next as a new learned input.
    pub learning: bool,
    pub learn_buffer: VecDeque<InputSource>,
}

impl ControllerRuntime {
    pub fn from_persistent(c: &InputController) -> Self {
        Self {
            id: c.id,
            name: c.name.clone(),
            kind: c.kind.clone(),
            inputs: c.inputs.clone(),
            values: vec![0.0; c.inputs.len()],
            status: ConnectionStatus::Disconnected,
            learning: false,
            learn_buffer: VecDeque::new(),
        }
    }

    pub fn to_persistent(&self) -> InputController {
        InputController {
            id: self.id,
            name: self.name.clone(),
            kind: self.kind.clone(),
            inputs: self.inputs.clone(),
        }
    }

    pub fn resize_values(&mut self) {
        self.values.resize(self.inputs.len(), 0.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connected,
    /// Port is configured but not currently available on the system.
    Waiting,
}

pub type SharedControllers = Arc<Mutex<Vec<ControllerRuntime>>>;

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Owns active MIDI sessions and a reconnect worker. Controllers themselves
/// live inside `SharedControllers`.
pub struct InputControllerManager {
    pub shared: SharedControllers,
    /// Active per-controller midir session (dropped on disconnect / removal).
    sessions: Vec<MidiSession>,
    next_input_id: u32,
}

impl InputControllerManager {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(Vec::new())),
            sessions: Vec::new(),
            next_input_id: 1,
        }
    }

    /// Replace the entire controller set (called on setup load/undo/redo).
    /// Drops and rebuilds sessions as needed.
    pub fn set_controllers(&mut self, controllers: &[InputController]) {
        // Close all existing sessions; we'll rebuild below as needed.
        self.sessions.clear();

        let mut state = self.shared.lock().unwrap();
        *state = controllers.iter().map(ControllerRuntime::from_persistent).collect();
        drop(state);

        // Ensure next_input_id stays above any existing ids.
        self.next_input_id = controllers.iter()
            .flat_map(|c| c.inputs.iter().map(|i| i.id))
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(1);

        self.reconcile_sessions();
    }

    /// Export current controllers for setup save.
    pub fn export(&self) -> Vec<InputController> {
        let state = self.shared.lock().unwrap();
        state.iter().map(ControllerRuntime::to_persistent).collect()
    }

    pub fn add_controller(&mut self, name: String) -> u32 {
        let mut state = self.shared.lock().unwrap();
        let id = state.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        state.push(ControllerRuntime {
            id,
            name,
            kind: InputControllerKind::Midi { hw_port_name: String::new() },
            inputs: Vec::new(),
            values: Vec::new(),
            status: ConnectionStatus::Disconnected,
            learning: false,
            learn_buffer: VecDeque::new(),
        });
        drop(state);
        self.reconcile_sessions();
        id
    }

    pub fn remove_controller(&mut self, id: u32) {
        let mut state = self.shared.lock().unwrap();
        state.retain(|c| c.id != id);
        drop(state);
        self.reconcile_sessions();
    }

    /// Change the hardware port mapping for a controller. Triggers reconnect.
    pub fn set_hw_port(&mut self, id: u32, port: String) {
        {
            let mut state = self.shared.lock().unwrap();
            if let Some(c) = state.iter_mut().find(|c| c.id == id) {
                c.kind = InputControllerKind::Midi { hw_port_name: port };
                c.status = ConnectionStatus::Disconnected;
            }
        }
        self.reconcile_sessions();
    }

    pub fn rename(&mut self, id: u32, name: String) {
        let mut state = self.shared.lock().unwrap();
        if let Some(c) = state.iter_mut().find(|c| c.id == id) {
            c.name = name;
        }
    }

    pub fn set_learning(&mut self, id: u32, learning: bool) {
        let mut state = self.shared.lock().unwrap();
        if let Some(c) = state.iter_mut().find(|c| c.id == id) {
            c.learning = learning;
            c.learn_buffer.clear();
        }
    }

    /// Consume one learned event from the buffer and add it as an input.
    /// Returns the new input id if something was added.
    pub fn consume_learn(&mut self, id: u32) -> Option<u32> {
        let source = {
            let mut state = self.shared.lock().unwrap();
            let c = state.iter_mut().find(|c| c.id == id)?;
            c.learn_buffer.pop_front()?
        };
        // Check for a duplicate source — don't add twice.
        {
            let state = self.shared.lock().unwrap();
            let c = state.iter().find(|c| c.id == id)?;
            if c.inputs.iter().any(|i| i.source == source) {
                return None;
            }
        }

        let new_id = self.next_input_id;
        self.next_input_id += 1;
        let name = source.label();
        let mode = if source.is_binary() {
            InputBindingMode::Value
        } else {
            InputBindingMode::Value
        };
        let input = LearnedInput { id: new_id, name, source, mode };

        let mut state = self.shared.lock().unwrap();
        if let Some(c) = state.iter_mut().find(|c| c.id == id) {
            c.inputs.push(input);
            c.resize_values();
        }
        Some(new_id)
    }

    pub fn remove_input(&mut self, controller_id: u32, input_id: u32) {
        let mut state = self.shared.lock().unwrap();
        if let Some(c) = state.iter_mut().find(|c| c.id == controller_id) {
            c.inputs.retain(|i| i.id != input_id);
            c.resize_values();
        }
    }

    pub fn rename_input(&mut self, controller_id: u32, input_id: u32, name: String) {
        let mut state = self.shared.lock().unwrap();
        if let Some(c) = state.iter_mut().find(|c| c.id == controller_id) {
            if let Some(i) = c.inputs.iter_mut().find(|i| i.id == input_id) {
                i.name = name;
            }
        }
    }

    pub fn set_input_mode(&mut self, controller_id: u32, input_id: u32, mode: InputBindingMode) {
        let mut state = self.shared.lock().unwrap();
        if let Some(c) = state.iter_mut().find(|c| c.id == controller_id) {
            if let Some(i) = c.inputs.iter_mut().find(|i| i.id == input_id) {
                i.mode = mode;
            }
        }
    }

    /// List currently available MIDI input ports on the system.
    pub fn available_midi_ports() -> Vec<String> {
        midi::available_ports()
    }

    /// Called periodically (from UI update loop) to try reconnecting any
    /// controllers whose hw port just appeared, and drop sessions whose port
    /// disappeared. Cheap no-op if nothing changed.
    pub fn tick_reconnect(&mut self) {
        self.reconcile_sessions();
    }

    /// Rebuild sessions to match `shared` state + port availability.
    fn reconcile_sessions(&mut self) {
        let ports = midi::available_ports();

        let controllers: Vec<(u32, String)> = {
            let state = self.shared.lock().unwrap();
            state.iter()
                .filter_map(|c| match &c.kind {
                    InputControllerKind::Midi { hw_port_name } if !hw_port_name.is_empty() => {
                        Some((c.id, hw_port_name.clone()))
                    }
                    _ => None,
                })
                .collect()
        };

        // Drop sessions for controllers that no longer exist or whose port changed.
        self.sessions.retain(|s| {
            controllers.iter().any(|(id, port)| *id == s.controller_id && port == &s.port_name)
                && ports.contains(&s.port_name)
        });

        // Open sessions for controllers that have a matching available port
        // but no active session yet.
        for (cid, port) in &controllers {
            let has_session = self.sessions.iter().any(|s| s.controller_id == *cid);
            if has_session { continue; }
            if !ports.contains(port) {
                // Port not available — mark waiting.
                let mut state = self.shared.lock().unwrap();
                if let Some(c) = state.iter_mut().find(|c| c.id == *cid) {
                    c.status = ConnectionStatus::Waiting;
                }
                continue;
            }
            match MidiSession::open(*cid, port.clone(), self.shared.clone()) {
                Ok(session) => {
                    self.sessions.push(session);
                    let mut state = self.shared.lock().unwrap();
                    if let Some(c) = state.iter_mut().find(|c| c.id == *cid) {
                        c.status = ConnectionStatus::Connected;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to open MIDI port '{}': {}", port, e);
                }
            }
        }

        // Update status for controllers without an active session.
        let mut state = self.shared.lock().unwrap();
        for c in state.iter_mut() {
            let active = self.sessions.iter().any(|s| s.controller_id == c.id);
            if !active {
                c.status = match &c.kind {
                    InputControllerKind::Midi { hw_port_name } if hw_port_name.is_empty() => {
                        ConnectionStatus::Disconnected
                    }
                    InputControllerKind::Midi { hw_port_name } if ports.contains(hw_port_name) => {
                        // Port available but we failed to open — treat as waiting for retry.
                        ConnectionStatus::Waiting
                    }
                    _ => ConnectionStatus::Waiting,
                };
            }
        }
    }
}
