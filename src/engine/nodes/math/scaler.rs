use crate::engine::types::*;

/// Maps an input from [in_min, in_max] to [0, 1].
pub struct ScalerProcessNode {
    id: NodeId,
    in_min: f32,
    in_max: f32,
    value_in: f32,
    value_out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl ScalerProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            in_min: 0.0,
            in_max: 1.0,
            value_in: 0.0,
            value_out: 0.0,
            inputs: vec![PortDef::new("in", PortType::Any)],
            outputs: vec![PortDef::new("out", PortType::Untyped)],
        }
    }
}

impl ProcessNode for ScalerProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Scaler" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.value_in = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.value_in } else { 0.0 }
    }

    fn process(&mut self) {
        let range = self.in_max - self.in_min;
        if range.abs() > 1e-10 {
            self.value_out = ((self.value_in - self.in_min) / range).clamp(0.0, 1.0);
        } else {
            self.value_out = 0.0;
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.value_out } else { 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float { name: "In Min".into(), value: self.in_min, min: -100.0, max: 100.0, step: 0.1, unit: "" },
            ParamDef::Float { name: "In Max".into(), value: self.in_max, min: -100.0, max: 100.0, step: 0.1, unit: "" },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.in_min = value.as_f32(),
            1 => self.in_max = value.as_f32(),
            _ => {}
        }
    }
}
