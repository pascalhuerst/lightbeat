use super::signal_type::{MAX_CHANNELS, publish_type, type_from_param, type_param};
use crate::engine::types::*;

/// Sample and Hold. On each rising edge of the trigger input, samples the
/// current value input and holds it on the output until the next trigger.
/// The value can be of any type; every channel is held.
pub struct SampleHoldProcessNode {
    id: NodeId,
    port_type: PortType,
    value_in: [f32; MAX_CHANNELS],
    trigger_in: f32,
    prev_trigger: f32,
    held: [f32; MAX_CHANNELS],
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl SampleHoldProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mut node = Self {
            id,
            port_type: PortType::Any,
            value_in: [0.0; MAX_CHANNELS],
            trigger_in: 0.0,
            prev_trigger: 0.0,
            held: [0.0; MAX_CHANNELS],
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        node.rebuild();
        node
    }

    fn rebuild(&mut self) {
        self.inputs = vec![
            PortDef::new("value", self.port_type),
            PortDef::new("trigger", PortType::Logic),
        ];
        self.outputs = vec![PortDef::new("out", self.port_type)];
        self.value_in = [0.0; MAX_CHANNELS];
        self.held = [0.0; MAX_CHANNELS];
    }

    fn width(&self) -> usize { self.port_type.channel_count() }
}

impl ProcessNode for SampleHoldProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Sample & Hold" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, ch: usize, v: f32) {
        let n = self.width();
        if ch < n { self.value_in[ch] = v; } else if ch == n { self.trigger_in = v; }
    }
    fn read_input(&self, ch: usize) -> f32 {
        let n = self.width();
        if ch < n { self.value_in[ch] } else if ch == n { self.trigger_in } else { 0.0 }
    }

    fn process(&mut self) {
        if self.trigger_in >= 0.5 && self.prev_trigger < 0.5 {
            self.held = self.value_in;
        }
        self.prev_trigger = self.trigger_in;
    }

    fn read_output(&self, ch: usize) -> f32 {
        if ch < self.width() { self.held[ch] } else { 0.0 }
    }

    // The type reshapes the ports.
    fn non_promotable_params(&self) -> &'static [&'static str] { &["Type"] }

    fn params(&self) -> Vec<ParamDef> { vec![type_param(self.port_type)] }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 {
            let pt = type_from_param(&value);
            if pt != self.port_type {
                self.port_type = pt;
                self.rebuild();
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        publish_type(shared, self.port_type);
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "held": &self.held[..self.width()] }))
    }
    fn load_data(&mut self, data: &serde_json::Value) {
        // Files from before the node had a type hold a single number.
        match data.get("held") {
            Some(serde_json::Value::Array(a)) => {
                for (slot, v) in self.held.iter_mut().zip(a) {
                    *slot = v.as_f64().unwrap_or(0.0) as f32;
                }
            }
            Some(v) => {
                if let Some(x) = v.as_f64() { self.held[0] = x as f32; }
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_color_is_sampled_on_the_rising_edge_and_held() {
        let mut n = SampleHoldProcessNode::new(NodeId(1));
        n.set_param(0, ParamValue::Choice(4));
        assert_eq!(n.inputs()[0].port_type, PortType::Color);
        for (c, v) in [0.1, 0.2, 0.3].into_iter().enumerate() { n.write_input(c, v); }
        n.write_input(3, 1.0);
        n.process();
        for c in 0..3 { n.write_input(c, 0.9); }
        n.write_input(3, 0.0);
        n.process();
        assert_eq!([n.read_output(0), n.read_output(1), n.read_output(2)], [0.1, 0.2, 0.3]);
    }

    #[test]
    fn a_held_number_saved_before_types_loads() {
        let mut n = SampleHoldProcessNode::new(NodeId(1));
        n.load_data(&serde_json::json!({ "held": 0.75 }));
        assert_eq!(n.read_output(0), 0.75);
    }
}
