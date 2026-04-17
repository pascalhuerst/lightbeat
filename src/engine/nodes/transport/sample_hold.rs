use crate::engine::types::*;

/// Sample and Hold. On each rising edge of the trigger input, samples the
/// current value input and holds it on the output until the next trigger.
pub struct SampleHoldProcessNode {
    id: NodeId,
    value_in: f32,
    trigger_in: f32,
    prev_trigger: f32,
    held: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl SampleHoldProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            value_in: 0.0,
            trigger_in: 0.0,
            prev_trigger: 0.0,
            held: 0.0,
            inputs: vec![
                PortDef::new("value", PortType::Untyped),
                PortDef::new("trigger", PortType::Logic),
            ],
            outputs: vec![PortDef::new("out", PortType::Untyped)],
        }
    }
}

impl ProcessNode for SampleHoldProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Sample & Hold" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        match pi { 0 => self.value_in = v, 1 => self.trigger_in = v, _ => {} }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi { 0 => self.value_in, 1 => self.trigger_in, _ => 0.0 }
    }

    fn process(&mut self) {
        if self.trigger_in >= 0.5 && self.prev_trigger < 0.5 {
            self.held = self.value_in;
        }
        self.prev_trigger = self.trigger_in;
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.held } else { 0.0 }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "held": self.held }))
    }
    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(v) = data.get("held").and_then(|v| v.as_f64()) {
            self.held = v as f32;
        }
    }
}
