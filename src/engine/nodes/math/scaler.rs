use crate::engine::types::*;

/// Maps an input from [in_min, in_max] to [0, 1]. `in_min` and `in_max`
/// can be overridden by wiring their input ports — follows the
/// "input-overrides-param" pattern so an unwired port means "use the
/// inspector value" and wiring it hides the param.
pub struct ScalerProcessNode {
    id: NodeId,
    in_min: f32,
    in_max: f32,
    value_in: f32,
    min_in: f32,
    max_in: f32,
    min_connected: bool,
    max_connected: bool,
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
            min_in: 0.0,
            max_in: 1.0,
            min_connected: false,
            max_connected: false,
            value_out: 0.0,
            inputs: vec![
                PortDef::new("in", PortType::Any),
                PortDef::new("min", PortType::Untyped),
                PortDef::new("max", PortType::Untyped),
            ],
            outputs: vec![PortDef::new("out", PortType::Untyped)],
        }
    }

    fn effective_min(&self) -> f32 {
        if self.min_connected { self.min_in } else { self.in_min }
    }
    fn effective_max(&self) -> f32 {
        if self.max_connected { self.max_in } else { self.in_max }
    }
}

impl ProcessNode for ScalerProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Scaler" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        match pi {
            0 => self.value_in = v,
            1 => self.min_in = v,
            2 => self.max_in = v,
            _ => {}
        }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi {
            0 => self.value_in,
            1 => self.min_in,
            2 => self.max_in,
            _ => 0.0,
        }
    }

    fn set_input_connections(&mut self, connected: &[bool]) {
        self.min_connected = connected.get(1).copied().unwrap_or(false);
        self.max_connected = connected.get(2).copied().unwrap_or(false);
    }

    fn process(&mut self) {
        let lo = self.effective_min();
        let hi = self.effective_max();
        let range = hi - lo;
        if range.abs() > 1e-10 {
            self.value_out = ((self.value_in - lo) / range).clamp(0.0, 1.0);
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
