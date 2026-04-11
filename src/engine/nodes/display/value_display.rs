use crate::engine::types::*;

pub struct ValueDisplayData {
    pub value: f32,
    pub mode: u8, // 0=number, 1=LED
}

pub struct ValueDisplayProcessNode {
    id: NodeId,
    value: f32,
    mode: usize,
    inputs: Vec<PortDef>,
}

impl ValueDisplayProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            value: 0.0,
            mode: 0,
            inputs: vec![PortDef::new("in", PortType::Any)],
        }
    }
}

impl ProcessNode for ValueDisplayProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Value Display" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.value = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.value } else { 0.0 }
    }
    fn process(&mut self) {}

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Choice {
            name: "Mode".into(),
            value: self.mode,
            options: vec!["Number".into(), "LED".into()],
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 { self.mode = value.as_usize(); }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(ValueDisplayData {
            value: self.value,
            mode: self.mode as u8,
        }));
    }
}
