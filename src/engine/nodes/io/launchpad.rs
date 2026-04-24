//! Dedicated Novation Launchpad S node.
//!
//! Binds to an `InputControllerKind::LaunchpadS` runtime and surfaces every
//! physical button as a named port — 64 grid pads (Untyped, 0..1 velocity),
//! 8 side scene-launch buttons (same shape), 8 top mode CC buttons (0..1
//! from CC value), plus an `any change` trigger. Every button has a
//! matching LED-feedback input; driving a 0..1 signal into it maps to red
//! brightness on the device (off → dim → mid → full).

use crate::engine::types::*;
use crate::input_controller::midi::MidiSource;
use crate::input_controller::{InputControllerKind, InputSource, SharedControllers};

pub struct LaunchpadDisplay {
    pub controller_id: u32,
    pub controller_name: String,
    pub connected: bool,
}

pub struct LaunchpadProcessNode {
    id: NodeId,
    controller_id: u32,
    controllers: SharedControllers,

    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,

    /// Per-output latched values, one slot per port. Index 0 is the
    /// `any change` trigger; 1..N mirror the physical controls in the
    /// order declared by `build_outputs()`.
    out_values: Vec<f32>,
    /// Previous tick's control values (skipping the trigger at index 0).
    prev_control_values: Vec<f32>,
    /// Per-input staged values (LED brightness before we push to
    /// `c.out_values` on the runtime).
    in_values: Vec<f32>,
    /// Cached mapping: output port index → runtime `values[]` index.
    /// Index 0 is always `usize::MAX` (the `any change` trigger is
    /// computed locally, not read from the runtime).
    out_to_runtime: Vec<usize>,
    /// Cached mapping: input port index → runtime `out_values[]` index.
    in_to_runtime: Vec<usize>,
    /// Controller id the maps were computed against. `0` = not yet built.
    cached_for: u32,

    connected: bool,
    controller_name: String,
}

impl LaunchpadProcessNode {
    pub fn new(id: NodeId, controllers: SharedControllers) -> Self {
        let outputs = build_outputs();
        let inputs = build_inputs();
        let n_out = outputs.len();
        let n_in = inputs.len();
        Self {
            id,
            controller_id: 0,
            controllers,
            inputs,
            outputs,
            out_values: vec![0.0; n_out],
            prev_control_values: Vec::new(),
            in_values: vec![0.0; n_in],
            out_to_runtime: Vec::new(),
            in_to_runtime: Vec::new(),
            cached_for: 0,
            connected: false,
            controller_name: String::new(),
        }
    }
}

impl ProcessNode for LaunchpadProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Launchpad S" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if let Some(slot) = self.in_values.get_mut(port_index) {
            *slot = value;
        }
    }

    fn process(&mut self) {
        if self.controller_id == 0 {
            for v in &mut self.out_values { *v = 0.0; }
            self.connected = false;
            return;
        }

        let mut state = self.controllers.lock().unwrap();
        let Some(c) = state.iter_mut().find(|c| c.id == self.controller_id) else {
            self.connected = false;
            for v in &mut self.out_values { *v = 0.0; }
            return;
        };
        if !matches!(c.kind, InputControllerKind::LaunchpadS { .. }) {
            self.connected = false;
            for v in &mut self.out_values { *v = 0.0; }
            return;
        }
        self.controller_name = c.name.clone();
        self.connected = true;

        if self.cached_for != self.controller_id
            || self.out_to_runtime.len() != out_count()
        {
            rebuild_maps(c, &mut self.out_to_runtime, &mut self.in_to_runtime);
            self.cached_for = self.controller_id;
        }

        // Read runtime values into output slots (skip slot 0 — the trigger).
        for (pi, &rt_idx) in self.out_to_runtime.iter().enumerate().skip(1) {
            self.out_values[pi] = if rt_idx == usize::MAX {
                0.0
            } else {
                c.values.get(rt_idx).copied().unwrap_or(0.0)
            };
        }

        // "any change" trigger — one-tick pulse whenever any control
        // differs from last tick's snapshot.
        let n_controls = self.out_values.len().saturating_sub(1);
        if self.prev_control_values.len() != n_controls {
            self.prev_control_values = vec![0.0; n_controls];
        }
        let changed = self.out_values[1..].iter()
            .zip(self.prev_control_values.iter())
            .any(|(cur, prev)| (cur - prev).abs() > f32::EPSILON);
        self.out_values[0] = if changed { 1.0 } else { 0.0 };
        self.prev_control_values.copy_from_slice(&self.out_values[1..]);

        // Push LED feedback. Writes go into `c.out_values` which the MIDI
        // feedback worker turns into LED messages.
        for (pi, &rt_idx) in self.in_to_runtime.iter().enumerate() {
            if rt_idx == usize::MAX { continue; }
            let v = self.in_values.get(pi).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            if let Some(slot) = c.out_values.get_mut(rt_idx) {
                *slot = v;
            }
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        self.out_values.get(pi).copied().unwrap_or(0.0)
    }
    fn read_input(&self, pi: usize) -> f32 {
        self.in_values.get(pi).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "controller_id": self.controller_id }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(id) = data.get("controller_id").and_then(|v| v.as_u64()) {
            let new_id = id as u32;
            if new_id != self.controller_id {
                self.controller_id = new_id;
                self.cached_for = 0;
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(LaunchpadDisplay {
            controller_id: self.controller_id,
            controller_name: self.controller_name.clone(),
            connected: self.connected,
        }));
    }
}

// ---------- port layout -----------------------------------------------------

const N_PADS: usize = 64;
const N_SIDE: usize = 8;
const N_TOP: usize = 8;
const N_CONTROLS: usize = N_PADS + N_SIDE + N_TOP; // 80

fn out_count() -> usize { N_CONTROLS + 1 }  // +1 for "any change"

fn pad_name(row: u8, col: u8) -> String { format!("pad r{}c{}", row + 1, col + 1) }
fn side_name(row: u8) -> String { format!("side {}", row + 1) }

const TOP_LABELS: [&str; 8] = [
    "Up", "Down", "Left", "Right", "Session", "User 1", "User 2", "Mixer",
];

fn build_outputs() -> Vec<PortDef> {
    let mut v = Vec::with_capacity(out_count());
    v.push(PortDef::new("any change", PortType::Logic));
    for row in 0..8u8 {
        for col in 0..8u8 {
            v.push(PortDef::new(pad_name(row, col), PortType::Untyped));
        }
    }
    for row in 0..8u8 {
        v.push(PortDef::new(side_name(row), PortType::Untyped));
    }
    for label in TOP_LABELS {
        v.push(PortDef::new(format!("top {}", label), PortType::Untyped));
    }
    v
}

fn build_inputs() -> Vec<PortDef> {
    // One LED-feedback input per physical button. Same order as outputs
    // (minus the "any change" trigger).
    let mut v = Vec::with_capacity(N_CONTROLS);
    for row in 0..8u8 {
        for col in 0..8u8 {
            v.push(PortDef::new(format!("LED {}", pad_name(row, col)), PortType::Untyped));
        }
    }
    for row in 0..8u8 {
        v.push(PortDef::new(format!("LED {}", side_name(row)), PortType::Untyped));
    }
    for label in TOP_LABELS {
        v.push(PortDef::new(format!("LED top {}", label), PortType::Untyped));
    }
    v
}

/// Build the source for each control in the layout order used by
/// `build_outputs`. Used to resolve port index → runtime index.
fn layout_sources() -> Vec<MidiSource> {
    let mut v = Vec::with_capacity(N_CONTROLS);
    for row in 0..8u8 {
        for col in 0..8u8 {
            v.push(MidiSource::NoteVelocity { channel: 1, note: row * 16 + col });
        }
    }
    for row in 0..8u8 {
        v.push(MidiSource::NoteVelocity { channel: 1, note: row * 16 + 8 });
    }
    for i in 0..8u8 {
        v.push(MidiSource::Cc { channel: 1, controller: 104 + i });
    }
    v
}

fn rebuild_maps(
    runtime: &crate::input_controller::ControllerRuntime,
    out_map: &mut Vec<usize>,
    in_map: &mut Vec<usize>,
) {
    out_map.clear();
    out_map.resize(out_count(), usize::MAX);
    in_map.clear();
    in_map.resize(N_CONTROLS, usize::MAX);
    // Port 0 is the "any change" trigger — stays usize::MAX.

    let sources = layout_sources();
    for (ctrl_idx, src) in sources.iter().enumerate() {
        let rt_idx = runtime.inputs.iter().position(|i| matches!(
            &i.source,
            InputSource::Midi(ms) if ms == src
        ));
        if let Some(rt_idx) = rt_idx {
            out_map[ctrl_idx + 1] = rt_idx;
            in_map[ctrl_idx] = rt_idx;
        }
    }
}
