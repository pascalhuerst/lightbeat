//! Scale and offset: `out = in * scale + offset`.
//!
//! `scale` and `offset` follow the "input-overrides-param" pattern: when the
//! port is wired, the wire value takes over and the inspector param is hidden.

use crate::engine::types::*;


pub struct ScaleOffsetProcessNode {
    id: NodeId,
    in_value: f32,
    scale: f32,
    offset: f32,
    scale_in: f32,
    offset_in: f32,
    scale_connected: bool,
    offset_connected: bool,
    out_value: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl ScaleOffsetProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            in_value: 0.0,
            scale: 1.0,
            offset: 0.0,
            scale_in: 1.0,
            offset_in: 0.0,
            scale_connected: false,
            offset_connected: false,
            out_value: 0.0,
            inputs: vec![
                PortDef::new("in", PortType::Untyped),
                PortDef::new("scale", PortType::Untyped),
                PortDef::new("offset", PortType::Untyped),
            ],
            outputs: vec![PortDef::new("out", PortType::Untyped)],
        }
    }

    fn effective_scale(&self) -> f32 {
        if self.scale_connected { self.scale_in } else { self.scale }
    }
    fn effective_offset(&self) -> f32 {
        if self.offset_connected { self.offset_in } else { self.offset }
    }
}

impl ProcessNode for ScaleOffsetProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Scale Offset" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        match pi {
            0 => self.in_value = v,
            1 => self.scale_in = v,
            2 => self.offset_in = v,
            _ => {}
        }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi {
            0 => self.in_value,
            1 => self.scale_in,
            2 => self.offset_in,
            _ => 0.0,
        }
    }

    fn set_input_connections(&mut self, connected: &[bool]) {
        self.scale_connected = connected.get(1).copied().unwrap_or(false);
        self.offset_connected = connected.get(2).copied().unwrap_or(false);
    }

    fn process(&mut self) {
        let r = self.effective_scale();
        let c = self.effective_offset();
        self.out_value = self.in_value * r + c;
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.out_value } else { 0.0 }
    }

    fn non_promotable_params(&self) -> &'static [&'static str] {
        &["Scale", "Offset"]
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float {
                name: "Scale".into(),
                value: self.scale,
                min: -10.0, max: 10.0, step: 0.01, unit: "",
            },
            ParamDef::Float {
                name: "Offset".into(),
                value: self.offset,
                min: -10.0, max: 10.0, step: 0.01, unit: "",
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.scale = value.as_f32(),
            1 => self.offset = value.as_f32(),
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
    fn wired_scale_and_offset_win_over_the_params() {
        let mut n = ScaleOffsetProcessNode::new(NodeId(1));
        n.set_param(0, ParamValue::Float(2.0));
        n.set_input_connections(&[true, true, true]);
        n.write_input(1, 3.0);
        n.write_input(2, -1.0);
        assert_eq!(run(&mut n, 1.0), 2.0);
    }

    #[test]
    fn unwired_it_passes_the_signal_through() {
        let mut n = ScaleOffsetProcessNode::new(NodeId(1));
        assert_eq!(run(&mut n, 0.7), 0.7);
    }
}
