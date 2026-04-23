//! Dedicated Native Instruments Kontrol X1 (Mk1) node.
//!
//! Binds to an `InputControllerKind::X1` runtime and surfaces every physical
//! control as a named port — 34 button outputs (Logic, 1 while held),
//! 4 encoder outputs (Untyped, 0..1 wrapping), and 8 potentiometer outputs
//! (Untyped, 0..1 absolute). Per-button LED feedback is done with one
//! Untyped input per button, so you can drive any individual LED by wiring
//! a Button or Fader output into the matching input port.

use crate::engine::types::*;
use crate::input_controller::x1::{
    ALL_BUTTONS, ALL_ENCODERS, ALL_POTS, X1ButtonId, X1EncoderId, X1PotId, X1Source,
    button_led_index, button_name, encoder_name, pot_name,
};
use crate::input_controller::{InputControllerKind, InputSource, SharedControllers};

pub struct X1Display {
    pub controller_id: u32,
    pub controller_name: String,
    pub connected: bool,
}

pub struct X1ProcessNode {
    id: NodeId,
    controller_id: u32,
    controllers: SharedControllers,

    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,

    /// Per-output latched values (one slot per port, in port order). Index 0
    /// is the "any change" trigger; indices 1..N mirror the physical
    /// controls in the order declared by `build_outputs()`.
    out_values: Vec<f32>,
    /// Previous tick's control values (excluding the "any change" slot).
    /// Used to fire the change-trigger when something moved.
    prev_control_values: Vec<f32>,
    /// Per-input staged values (LED brightness before we push to out_values
    /// on the runtime).
    in_values: Vec<f32>,
    /// Cached mapping from output port index → runtime `values[]` index.
    /// Index 0 is always `usize::MAX` (the "any change" trigger is computed
    /// locally, not read from the runtime).
    out_to_runtime: Vec<usize>,
    /// Cached mapping from input port index (button feedback) → runtime
    /// `out_values[]` index.
    in_to_runtime: Vec<usize>,
    /// Controller id that `out_to_runtime` / `in_to_runtime` were computed
    /// against. `0` means "not yet built".
    cached_for: u32,

    connected: bool,
    controller_name: String,
}

impl X1ProcessNode {
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

impl ProcessNode for X1ProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "X1" }
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
        if !matches!(c.kind, InputControllerKind::X1) {
            self.connected = false;
            for v in &mut self.out_values { *v = 0.0; }
            return;
        }
        self.controller_name = c.name.clone();
        self.connected = true;

        // Rebuild port-index → runtime-index maps if this is the first tick
        // with this controller, or if the runtime's input set changed.
        if self.cached_for != self.controller_id
            || self.out_to_runtime.len() != out_count()
        {
            rebuild_maps(c, &mut self.out_to_runtime, &mut self.in_to_runtime);
            self.cached_for = self.controller_id;
        }

        // Read current values into our output slots. Skip slot 0 — that's
        // the "any change" trigger we compute after the loop.
        for (pi, &rt_idx) in self.out_to_runtime.iter().enumerate().skip(1) {
            if rt_idx == usize::MAX {
                self.out_values[pi] = 0.0;
            } else {
                self.out_values[pi] = c.values.get(rt_idx).copied().unwrap_or(0.0);
            }
        }

        // "any change" trigger — one-tick pulse whenever any control value
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

        // Push LED feedback — one input per button, matching the button-output
        // layout. Writes go into `out_values[]` which the X1 session turns
        // into LED bytes on its next poll tick.
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
        shared.display = Some(Box::new(X1Display {
            controller_id: self.controller_id,
            controller_name: self.controller_name.clone(),
            connected: self.connected,
        }));
    }
}

// ---------- port layout -----------------------------------------------------

fn control_count() -> usize {
    ALL_BUTTONS.len() + ALL_ENCODERS.len() + ALL_POTS.len()
}

fn out_count() -> usize {
    // +1 for the "any change" trigger slot at index 0.
    control_count() + 1
}

fn build_outputs() -> Vec<PortDef> {
    // All control outputs are Untyped — same convention as the generic
    // InputController / BCF2000 node. `Untyped` accepts/delivers any 0..1
    // signal with no port-type compatibility friction, which is what the
    // user almost always wants when wiring a MIDI-/HID-style controller
    // into the graph. The single `any change` trigger is Logic because
    // that's its nature (1-tick pulse).
    let mut v = Vec::with_capacity(out_count());
    v.push(PortDef::new("any change", PortType::Logic));
    for &b in ALL_BUTTONS {
        v.push(PortDef::new(button_name(b), PortType::Untyped));
    }
    for &e in ALL_ENCODERS {
        v.push(PortDef::new(encoder_name(e), PortType::Untyped));
    }
    for &p in ALL_POTS {
        v.push(PortDef::new(pot_name(p), PortType::Untyped));
    }
    v
}

/// Buttons that actually have a dedicated LED on the X1 Mk1, in the same
/// order as `ALL_BUTTONS`. Buttons without an LED are skipped so their
/// (useless) feedback input port doesn't clutter the node.
fn led_buttons() -> Vec<X1ButtonId> {
    ALL_BUTTONS
        .iter()
        .copied()
        .filter(|&b| button_led_index(b).is_some())
        .collect()
}

fn build_inputs() -> Vec<PortDef> {
    led_buttons()
        .into_iter()
        .map(|b| PortDef::new(format!("LED {}", button_name(b)), PortType::Untyped))
        .collect()
}

/// Build the `port_index → runtime_values_index` lookups. Usize::MAX signals
/// "no matching input on the runtime" (input set got trimmed, older saved
/// project, etc.).
fn rebuild_maps(
    runtime: &crate::input_controller::ControllerRuntime,
    out_map: &mut Vec<usize>,
    in_map: &mut Vec<usize>,
) {
    out_map.clear();
    out_map.resize(out_count(), usize::MAX);
    let led_btns = led_buttons();
    in_map.clear();
    in_map.resize(led_btns.len(), usize::MAX);
    // Port 0 is the "any change" trigger; leave as usize::MAX so the main
    // loop knows not to sample a runtime slot for it.

    // Build source → runtime-index once.
    let mut button_idx = std::collections::HashMap::<X1ButtonId, usize>::new();
    let mut encoder_idx = std::collections::HashMap::<X1EncoderId, usize>::new();
    let mut pot_idx = std::collections::HashMap::<X1PotId, usize>::new();
    for (i, input) in runtime.inputs.iter().enumerate() {
        if let InputSource::X1(s) = &input.source {
            match s {
                X1Source::Button(b) => { button_idx.insert(*b, i); }
                X1Source::Encoder(e) => { encoder_idx.insert(*e, i); }
                X1Source::Pot(p) => { pot_idx.insert(*p, i); }
            }
        }
    }

    // Outputs layout: "any change" at 0, then buttons, encoders, pots.
    let mut pi = 1;
    for &b in ALL_BUTTONS {
        if let Some(&idx) = button_idx.get(&b) {
            out_map[pi] = idx;
        }
        pi += 1;
    }
    for &e in ALL_ENCODERS {
        if let Some(&idx) = encoder_idx.get(&e) { out_map[pi] = idx; }
        pi += 1;
    }
    for &p in ALL_POTS {
        if let Some(&idx) = pot_idx.get(&p) { out_map[pi] = idx; }
        pi += 1;
    }

    // Inputs layout mirrors `led_buttons()` — one LED-feedback input per
    // button with an actual LED on the hardware.
    for (bi, b) in led_btns.iter().enumerate() {
        if let Some(&idx) = button_idx.get(b) {
            in_map[bi] = idx;
        }
    }
}
