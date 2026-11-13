//! Gain — multiplies the input by a 0..1 scalar. Simple attenuator; the
//! scalar is both editable as a param and driven by a knob in the node
//! body.

use crate::engine::param_spec::FloatParam;
use crate::engine::types::*;

const GAIN: FloatParam = FloatParam::new("Gain", 0.0, 1.0, 0.01, "").clamped();

pub struct GainDisplay {
    pub gain: f32,
    pub output: f32,
}

pub struct GainProcessNode {
    id: NodeId,
    input: f32,
    gain: f32,
    output: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports() -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![PortDef::new("in", PortType::Untyped)],
        vec![PortDef::new("out", PortType::Untyped)],
    )
}

impl GainProcessNode {
    pub fn new(id: NodeId) -> Self {
        let (inputs, outputs) = ports();
        Self {
            id,
            input: 0.0,
            gain: 1.0,
            output: 0.0,
            inputs,
            outputs,
        }
    }
}

impl ProcessNode for GainProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Gain" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.input = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.input } else { 0.0 }
    }

    fn process(&mut self) {
        self.output = self.input * self.gain;
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.output } else { 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![GAIN.def(self.gain)]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 {
            self.gain = GAIN.apply(&value);
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(GainDisplay {
            gain: self.gain,
            output: self.output,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gain_is_restored_by_its_param_alone() {
        let mut n = GainProcessNode::new(NodeId(1));
        assert!(n.save_data().is_none(), "the param already holds the gain");
        n.set_param(0, ParamValue::Float(0.4));
        n.write_input(0, 0.5);
        n.process();
        assert_eq!(n.read_output(0), 0.2);
        n.set_param(0, ParamValue::Float(7.0));
        assert!(matches!(n.params()[0], ParamDef::Float { value, .. } if value == 1.0));
    }
}
