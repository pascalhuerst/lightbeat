//! Scale and offset: `out = in * scale + offset`.
//!
//! Scale and Offset are ordinary settings; a new node has them exposed as
//! inputs (see `ScaleOffsetWidget::default_promoted`).

use crate::engine::param_spec::FloatParam;
use crate::engine::types::*;


const SCALE: FloatParam = FloatParam::new("Scale", -10.0, 10.0, 0.01, "");
const OFFSET: FloatParam = FloatParam::new("Offset", -10.0, 10.0, 0.01, "");


pub struct ScaleOffsetProcessNode {
    id: NodeId,
    in_value: f32,
    scale: f32,
    offset: f32,
    out_value: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports() -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![
            PortDef::new("in", PortType::Untyped),
        ],
        vec![PortDef::new("out", PortType::Untyped)],
    )
}

impl ScaleOffsetProcessNode {
    pub fn new(id: NodeId) -> Self {
        let (inputs, outputs) = ports();
        Self {
            id,
            in_value: 0.0,
            scale: 1.0,
            offset: 0.0,
            out_value: 0.0,
            inputs,
            outputs,
        }
    }
}

impl ProcessNode for ScaleOffsetProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Scale Offset" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.in_value = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.in_value } else { 0.0 }
    }

    fn process(&mut self) {
        self.out_value = self.in_value * self.scale + self.offset;
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.out_value } else { 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![SCALE.def(self.scale), OFFSET.def(self.offset)]
    }

    fn unclamped_input_params(&self) -> &'static [&'static str] { &["Scale", "Offset"] }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.scale = SCALE.apply(&value),
            1 => self.offset = OFFSET.apply(&value),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(node: &mut ScaleOffsetProcessNode, input: f32) -> f32 {
        node.write_input(0, input);
        node.process();
        node.read_output(0)
    }

    #[test]
    fn it_scales_then_adds_the_offset() {
        let mut n = ScaleOffsetProcessNode::new(NodeId(1));
        n.set_param(0, ParamValue::Float(2.0));
        n.set_param(1, ParamValue::Float(0.5));
        assert_eq!(run(&mut n, 0.25), 1.0);
        assert_eq!(run(&mut n, -1.0), -1.5);
    }

    #[test]
    fn a_promoted_scale_and_offset_drive_the_settings() {
        use crate::engine::promoted::{PromotedInputs, PromotedKind, PromotedParam};
        let mut n = ScaleOffsetProcessNode::new(NodeId(1));
        let mut promoted = PromotedInputs::default();
        let wanted: Vec<_> = ["Scale", "Offset"]
            .iter()
            .map(|name| PromotedParam { name: name.to_string(), kind: PromotedKind::Float })
            .collect();
        promoted.adopt(&wanted, &n);
        promoted.note_wiring(&[true, true]);
        promoted.apply(&mut n, 0, 3.0);
        promoted.apply(&mut n, 1, -1.0);
        assert_eq!(run(&mut n, 1.0), 2.0);
    }

    #[test]
    fn unwired_it_passes_the_signal_through() {
        let mut n = ScaleOffsetProcessNode::new(NodeId(1));
        assert_eq!(run(&mut n, 0.7), 0.7);
    }
}
