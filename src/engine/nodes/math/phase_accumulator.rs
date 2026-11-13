//! Phase Accumulator — turns a wrapping 0..1 input (X1 encoders, LFOs in
//! phase mode, etc.) into a clamped non-wrapping integer-ish counter.
//!
//! On each tick it unwraps the delta across the 0↔1 boundary, scales it,
//! adds it to the running value, and clamps to `[min, max]`. A rising
//! edge on `reset` returns the value to 0.
//!
//! With the default settings (`scale = 24`, matching the X1's 24-clicks-
//! per-revolution), one encoder click bumps the output by exactly 1.

use crate::engine::param_spec::FloatParam;
use crate::engine::types::*;


const MIN: FloatParam = FloatParam::new("Min", -1000.0, 1000.0, 0.1, "");
const MAX: FloatParam = FloatParam::new("Max", -1000.0, 1000.0, 0.1, "");
const SCALE: FloatParam = FloatParam::new("Scale", 0.1, 256.0, 0.1, "").at_least(0.0);

pub struct PhaseAccumulatorProcessNode {
    id: NodeId,
    in_value: f32,
    prev_in: f32,
    reset_in: f32,
    prev_reset: f32,
    state: f32,
    min: f32,
    max: f32,
    scale: f32,
    /// Latched flag so the first tick after load doesn't treat the
    /// initial `in_value` as a delta from 0 (which would spike the
    /// state).
    initialized: bool,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports() -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![
            PortDef::new("in", PortType::Untyped),
            PortDef::new("reset", PortType::Logic),
        ],
        vec![PortDef::new("out", PortType::Untyped)],
    )
}

impl PhaseAccumulatorProcessNode {
    pub fn new(id: NodeId) -> Self {
        let (inputs, outputs) = ports();
        Self {
            id,
            in_value: 0.0,
            prev_in: 0.0,
            reset_in: 0.0,
            prev_reset: 0.0,
            state: 0.0,
            min: -5.0,
            max: 5.0,
            scale: 24.0,
            initialized: false,
            inputs,
            outputs,
        }
    }
}

impl ProcessNode for PhaseAccumulatorProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Phase Accumulator" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        match pi {
            0 => self.in_value = v,
            1 => self.reset_in = v,
            _ => {}
        }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi {
            0 => self.in_value,
            1 => self.reset_in,
            _ => 0.0,
        }
    }

    fn process(&mut self) {
        // Rising edge on reset: zero the accumulator.
        if self.reset_in >= 0.5 && self.prev_reset < 0.5 {
            self.state = 0.0;
        }
        self.prev_reset = self.reset_in;

        if !self.initialized {
            self.prev_in = self.in_value;
            self.initialized = true;
            return;
        }

        // Unwrap the per-tick delta across the 0↔1 boundary. For a wrapping
        // phase input, a jump from 0.98 → 0.02 is actually a small +0.04
        // forward step; a jump from 0.02 → 0.98 is a small -0.04 backward
        // step.
        let mut delta = self.in_value - self.prev_in;
        if delta > 0.5 { delta -= 1.0; }
        else if delta < -0.5 { delta += 1.0; }
        self.prev_in = self.in_value;

        self.state = (self.state + delta * self.scale).clamp(self.min, self.max);
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.state } else { 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![MIN.def(self.min), MAX.def(self.max), SCALE.def(self.scale)]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.min = MIN.apply(&value),
            1 => self.max = MAX.apply(&value),
            2 => self.scale = SCALE.apply(&value),
            _ => {}
        }
        // Re-clamp in case the user tightened the range past the current
        // value.
        self.state = self.state.clamp(self.min, self.max);
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "state": self.state }))
    }
    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(v) = data.get("state").and_then(|v| v.as_f64()) {
            self.state = (v as f32).clamp(self.min, self.max);
        }
    }
}
