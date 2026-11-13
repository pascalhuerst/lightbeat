use crate::engine::param_spec::{FloatParam};
use crate::engine::types::*;


const THRESHOLD: FloatParam = FloatParam::new("Threshold", -1.0, 1.0, 0.01, "");
const HYSTERESIS: FloatParam = FloatParam::new("Hysteresis", 0.0, 1.0, 0.01, "").at_least(0.0);

/// Schmitt trigger — a comparator with hysteresis. Output flips HIGH when
/// the input rises above `threshold + hysteresis` and flips LOW when it
/// falls below `threshold - hysteresis`. Between those bands the output
/// holds its previous state, which eliminates chattering on noisy signals
/// that cross a single-point threshold repeatedly.
pub struct SchmittTriggerProcessNode {
    id: NodeId,
    value_in: f32,
    state: bool,
    threshold: f32,
    hysteresis: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports() -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![PortDef::new("in", PortType::Untyped)],
        vec![PortDef::new("out", PortType::Logic)],
    )
}

impl SchmittTriggerProcessNode {
    pub fn new(id: NodeId) -> Self {
        let (inputs, outputs) = ports();
        Self {
            id,
            value_in: 0.0,
            state: false,
            threshold: 0.5,
            hysteresis: 0.1,
            inputs,
            outputs,
        }
    }
}

impl ProcessNode for SchmittTriggerProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Schmitt Trigger" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.value_in = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.value_in } else { 0.0 }
    }

    fn process(&mut self) {
        // Clamp hysteresis to non-negative; zero collapses to a plain
        // comparator around `threshold`.
        let h = self.hysteresis.max(0.0);
        let high = self.threshold + h;
        let low = self.threshold - h;
        if self.state {
            if self.value_in < low { self.state = false; }
        } else if self.value_in > high {
            self.state = true;
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { if self.state { 1.0 } else { 0.0 } } else { 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![THRESHOLD.def(self.threshold), HYSTERESIS.def(self.hysteresis)]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.threshold = THRESHOLD.apply(&value),
            1 => self.hysteresis = HYSTERESIS.apply(&value),
            _ => {}
        }
    }
}
