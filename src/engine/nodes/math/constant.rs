use crate::engine::types::*;

pub struct ConstantProcessNode {
    id: NodeId,
    value: f32,
    port_type: PortType,
    outputs: Vec<PortDef>,
}

impl ConstantProcessNode {
    pub fn new(id: NodeId, port_type: PortType, default: f32) -> Self {
        let name = match port_type {
            PortType::Logic => "out",
            PortType::Phase => "out",
            PortType::Untyped => "out",
            PortType::Any => "out",
        };
        Self {
            id,
            value: default,
            port_type,
            outputs: vec![PortDef::new(name, port_type)],
        }
    }

    pub fn type_name_for(port_type: PortType) -> &'static str {
        match port_type {
            PortType::Logic => "Const Logic",
            PortType::Phase => "Const Phase",
            _ => "Const Value",
        }
    }
}

impl ProcessNode for ConstantProcessNode {
    fn node_id(&self) -> NodeId { self.id }

    fn type_name(&self) -> &'static str {
        Self::type_name_for(self.port_type)
    }

    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn process(&mut self) {}

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index { 0 => self.value, _ => 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        let (min, max, step) = match self.port_type {
            PortType::Logic => (0.0, 1.0, 1.0),
            PortType::Phase => (0.0, 1.0, 0.01),
            _ => (-10.0, 10.0, 0.01),
        };
        vec![ParamDef::Float {
            name: "Value".into(),
            value: self.value,
            min, max, step,
            unit: "",
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if let (0, ParamValue::Float(v)) = (index, value) {
            self.value = v;
        }
    }
}
