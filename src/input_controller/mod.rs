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
    /// Behringer BCF2000 with factory preset 1. Fixed 44-input layout,
    /// bidirectional CC (motor faders, LED rings, button LEDs driven by the
    /// graph). `hw_output_port` is required for any of the feedback to work.
    Bcf2000 {
        hw_input_port: String,
        #[serde(default)]
        hw_output_port: String,
    },
}

impl InputControllerKind {
    pub fn label(&self) -> &'static str {
        match self {
            InputControllerKind::Midi { .. } => "MIDI",
            InputControllerKind::Bcf2000 { .. } => "BCF2000",
        }
    }

    /// MIDI input port name (empty when not configured).
    pub fn input_port(&self) -> &str {
        match self {
            InputControllerKind::Midi { hw_port_name } => hw_port_name,
            InputControllerKind::Bcf2000 { hw_input_port, .. } => hw_input_port,
        }
    }

    /// MIDI output port name, if the kind supports feedback (empty string if not set).
    pub fn output_port(&self) -> &str {
        match self {
            InputControllerKind::Midi { .. } => "",
            InputControllerKind::Bcf2000 { hw_output_port, .. } => hw_output_port,
        }
    }

    /// True if the kind supports graph → device feedback (echo CC out).
    pub fn has_feedback(&self) -> bool {
        matches!(self, InputControllerKind::Bcf2000 { .. })
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
    /// Graph → device feedback values, same length as `inputs`. Only used by
    /// kinds where `has_feedback()` is true. The session's feedback worker
    /// thread polls this and emits MIDI CC when values change.
    pub out_values: Vec<f32>,
    /// Connection status for UI badges.
    pub status: ConnectionStatus,
    /// When Some, incoming raw events are captured into `learn_buffer` for
    /// the UI to pick the next as a new learned input.
    pub learning: bool,
    pub learn_buffer: VecDeque<InputSource>,
}

impl ControllerRuntime {
    pub fn from_persistent(c: &InputController) -> Self {
        let inputs = match &c.kind {
            // BCF2000 is hardwired to preset 1 — saved `inputs` are ignored
            // so the layout stays in lock-step with the code.
            InputControllerKind::Bcf2000 { .. } => bcf2000_preset1_inputs(),
            _ => c.inputs.clone(),
        };
        let n = inputs.len();
        Self {
            id: c.id,
            name: c.name.clone(),
            kind: c.kind.clone(),
            inputs,
            values: vec![0.0; n],
            out_values: vec![0.0; n],
            status: ConnectionStatus::Disconnected,
            learning: false,
            learn_buffer: VecDeque::new(),
        }
    }

    pub fn to_persistent(&self) -> InputController {
        // Don't serialize the BCF2000 canonical input list — we regenerate it
        // on load so any mapping fix in code propagates to existing setups.
        let inputs = match &self.kind {
            InputControllerKind::Bcf2000 { .. } => Vec::new(),
            _ => self.inputs.clone(),
        };
        InputController {
            id: self.id,
            name: self.name.clone(),
            kind: self.kind.clone(),
            inputs,
        }
    }

    pub fn resize_values(&mut self) {
        self.values.resize(self.inputs.len(), 0.0);
        self.out_values.resize(self.inputs.len(), 0.0);
    }
}

// ---------------------------------------------------------------------------
// BCF2000 factory preset 1 layout
// ---------------------------------------------------------------------------

/// Canonical BCF2000 preset 1 control map. All channel 1. Widely documented as
/// the out-of-the-box factory default; the device manual doesn't spell it out
/// per-control but Behringer's community preset library and decades of
/// DAW-template work converge on these CC numbers.
pub fn bcf2000_preset1_inputs() -> Vec<LearnedInput> {
    use self::midi::MidiSource;
    let ch = 1u8;
    let mut inputs = Vec::with_capacity(44);
    let mut id: u32 = 1;

    let mut push_cc = |inputs: &mut Vec<LearnedInput>, id: &mut u32, name: &str, cc: u8, binary: bool| {
        inputs.push(LearnedInput {
            id: *id,
            name: name.to_string(),
            source: InputSource::Midi(MidiSource::Cc { channel: ch, controller: cc }),
            mode: if binary { InputBindingMode::Value } else { InputBindingMode::Value },
        });
        *id += 1;
        let _ = binary;
    };

    // 8 faders → CC 81..88 (continuous).
    for i in 0..8 { push_cc(&mut inputs, &mut id, &format!("Fader {}", i + 1), 81 + i as u8, false); }
    // 8 encoder rotations → CC 1..8 (continuous).
    for i in 0..8 { push_cc(&mut inputs, &mut id, &format!("Enc {}", i + 1), 1 + i as u8, false); }
    // 8 encoder pushes → CC 33..40 (binary).
    for i in 0..8 { push_cc(&mut inputs, &mut id, &format!("Enc {} Push", i + 1), 33 + i as u8, true); }
    // Top row buttons → CC 65..72 (binary).
    for i in 0..8 { push_cc(&mut inputs, &mut id, &format!("Btn Top {}", i + 1), 65 + i as u8, true); }
    // Bottom row buttons → CC 73..80 (binary).
    for i in 0..8 { push_cc(&mut inputs, &mut id, &format!("Btn Bot {}", i + 1), 73 + i as u8, true); }
    // 4 free buttons → CC 89..92 (binary).
    for i in 0..4 { push_cc(&mut inputs, &mut id, &format!("Btn Free {}", i + 1), 89 + i as u8, true); }

    inputs
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
            out_values: Vec::new(),
            status: ConnectionStatus::Disconnected,
            learning: false,
            learn_buffer: VecDeque::new(),
        });
        drop(state);
        self.reconcile_sessions();
        id
    }

    /// Add a BCF2000 controller — preset-1 input layout is populated
    /// automatically. `add_controller` is the generic path.
    pub fn add_bcf2000(&mut self, name: String) -> u32 {
        let mut state = self.shared.lock().unwrap();
        let id = state.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        let inputs = bcf2000_preset1_inputs();
        let n = inputs.len();
        // Bump next_input_id so any future learn/add doesn't collide.
        let max_input_id = inputs.iter().map(|i| i.id).max().unwrap_or(0);
        if max_input_id >= self.next_input_id {
            self.next_input_id = max_input_id + 1;
        }
        state.push(ControllerRuntime {
            id,
            name,
            kind: InputControllerKind::Bcf2000 {
                hw_input_port: String::new(),
                hw_output_port: String::new(),
            },
            inputs,
            values: vec![0.0; n],
            out_values: vec![0.0; n],
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

    /// Change the hardware input port mapping for a controller. Triggers
    /// reconnect. Preserves the kind's extra fields (e.g. BCF2000's output
    /// port mapping).
    pub fn set_hw_port(&mut self, id: u32, port: String) {
        {
            let mut state = self.shared.lock().unwrap();
            if let Some(c) = state.iter_mut().find(|c| c.id == id) {
                match &mut c.kind {
                    InputControllerKind::Midi { hw_port_name } => *hw_port_name = port,
                    InputControllerKind::Bcf2000 { hw_input_port, .. } => *hw_input_port = port,
                }
                c.status = ConnectionStatus::Disconnected;
            }
        }
        self.reconcile_sessions();
    }

    /// Change the hardware output port (used for motor fader / LED feedback
    /// on BCF2000 and other feedback-capable kinds). No-op for plain MIDI.
    pub fn set_hw_output_port(&mut self, id: u32, port: String) {
        {
            let mut state = self.shared.lock().unwrap();
            if let Some(c) = state.iter_mut().find(|c| c.id == id) {
                if let InputControllerKind::Bcf2000 { hw_output_port, .. } = &mut c.kind {
                    *hw_output_port = port;
                }
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

    /// List currently available MIDI output ports on the system.
    pub fn available_midi_output_ports() -> Vec<String> {
        midi::available_output_ports()
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

        // (controller_id, input_port, Option<output_port>)
        let controllers: Vec<(u32, String, Option<String>)> = {
            let state = self.shared.lock().unwrap();
            state.iter()
                .filter_map(|c| {
                    let ip = c.kind.input_port();
                    if ip.is_empty() { return None; }
                    let op = c.kind.output_port();
                    let op = if op.is_empty() { None } else { Some(op.to_string()) };
                    Some((c.id, ip.to_string(), op))
                })
                .collect()
        };

        // Drop sessions for controllers that no longer exist or whose port(s) changed.
        self.sessions.retain(|s| {
            let desc = controllers.iter().find(|(id, _, _)| *id == s.controller_id);
            let matched = match desc {
                Some((_, port, out_port)) => {
                    *port == s.port_name && *out_port == s.output_port_name
                }
                None => false,
            };
            matched && ports.contains(&s.port_name)
                && s.output_port_name.as_ref().map(|p| ports.contains(p)).unwrap_or(true)
        });

        // Open sessions for controllers that have a matching available port
        // but no active session yet.
        for (cid, port, out_port) in &controllers {
            let has_session = self.sessions.iter().any(|s| s.controller_id == *cid);
            if has_session { continue; }
            if !ports.contains(port) {
                let mut state = self.shared.lock().unwrap();
                if let Some(c) = state.iter_mut().find(|c| c.id == *cid) {
                    c.status = ConnectionStatus::Waiting;
                }
                continue;
            }
            match MidiSession::open(*cid, port.clone(), out_port.clone(), self.shared.clone()) {
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
                let ip = c.kind.input_port();
                c.status = if ip.is_empty() {
                    ConnectionStatus::Disconnected
                } else {
                    ConnectionStatus::Waiting
                };
            }
        }
    }
}
