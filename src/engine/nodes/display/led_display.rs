use std::any::Any;

use crate::engine::types::*;

pub struct LedDisplayData {
    pub name: String,
    pub value: f32,
}

pub struct LedDisplayProcessNode {
    id: NodeId,
    pub name: String,
    value: f32,
    inputs: Vec<PortDef>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports() -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![PortDef::new("in", PortType::Any)],
        vec![],
    )
}

impl LedDisplayProcessNode {
    pub fn new(id: NodeId) -> Self {
        let (inputs, _) = ports();
        Self {
            id,
            name: String::new(),
            value: 0.0,
            inputs,
        }
    }

    pub fn name(&self) -> &str { &self.name }
    pub fn value(&self) -> f32 { self.value }
}

impl ProcessNode for LedDisplayProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "LED Display" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.value = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.value } else { 0.0 }
    }
    fn process(&mut self) {}

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "name": self.name }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
            self.name = n.to_string();
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(LedDisplayData {
            name: self.name.clone(),
            value: self.value,
        }));
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> Option<&mut dyn Any> { Some(self) }
}
