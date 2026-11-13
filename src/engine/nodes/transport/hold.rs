use crate::engine::types::*;

pub struct TriggerHoldDisplay {
    pub remaining_ms: f32,
    pub effective_duration_ms: u32,
}

/// Holds a trigger high for a number of milliseconds after each rising edge.
/// Duration is taken from the `duration` input (in ms) while it is wired,
/// otherwise from the `Duration` param. The time left is counted down by the
/// measured tick length, so the hold stays right if the loop runs late.
pub struct TriggerHoldProcessNode {
    id: NodeId,
    trigger_in: f32,
    prev_trigger: f32,
    duration_in: f32,
    duration_connected: bool,
    /// Used while the input is unwired.
    default_duration: u32,
    /// Milliseconds of hold left.
    remaining: f32,
    output: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports() -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![
            PortDef::new("trigger", PortType::Logic),
            PortDef::new("duration", PortType::Untyped),
        ],
        vec![PortDef::new("out", PortType::Logic)],
    )
}

impl TriggerHoldProcessNode {
    pub fn new(id: NodeId) -> Self {
        let (inputs, outputs) = ports();
        Self {
            id,
            trigger_in: 0.0,
            prev_trigger: 0.0,
            duration_in: 0.0,
            duration_connected: false,
            default_duration: 100,
            remaining: 0.0,
            output: 0.0,
            inputs,
            outputs,
        }
    }

    fn effective_duration(&self) -> u32 {
        if self.duration_connected {
            self.duration_in.round().max(1.0) as u32
        } else {
            self.default_duration.max(1)
        }
    }
}

impl ProcessNode for TriggerHoldProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Trigger Hold" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        match pi {
            0 => self.trigger_in = v,
            1 => self.duration_in = v,
            _ => {}
        }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi { 0 => self.trigger_in, 1 => self.duration_in, _ => 0.0 }
    }

    fn set_input_connections(&mut self, connected: &[bool]) {
        self.duration_connected = connected.get(1).copied().unwrap_or(false);
    }

    fn process(&mut self) {
        // Rising edge: (re)start the hold.
        if self.trigger_in >= 0.5 && self.prev_trigger < 0.5 {
            self.remaining = self.effective_duration() as f32;
        }
        self.prev_trigger = self.trigger_in;

        if self.remaining > 1e-3 {
            self.output = 1.0;
            self.remaining -= crate::engine::time::dt() * 1000.0;
        } else {
            self.output = 0.0;
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.output } else { 0.0 }
    }

    // The duration already has an input port of its own.
    fn non_promotable_params(&self) -> &'static [&'static str] { &["Duration"] }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Float {
            name: "Duration".into(),
            value: self.default_duration as f32,
            min: 1.0,
            max: 100_000.0,
            step: 1.0,
            unit: "ms",
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 {
            self.default_duration = (value.as_f32().round().max(1.0) as u32).min(100_000);
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "default_duration": self.default_duration }))
    }
    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(d) = data.get("default_duration").and_then(|v| v.as_u64()) {
            self.default_duration = d.min(100_000) as u32;
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(TriggerHoldDisplay {
            remaining_ms: self.remaining.max(0.0),
            effective_duration_ms: self.effective_duration(),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pulse_length(n: &mut TriggerHoldProcessNode) -> usize {
        n.write_input(0, 0.0);
        n.process();
        n.write_input(0, 1.0);
        n.process();
        n.write_input(0, 0.0);
        let mut high = if n.read_output(0) > 0.5 { 1 } else { 0 };
        for _ in 0..50 {
            n.process();
            if n.read_output(0) > 0.5 { high += 1; }
        }
        high
    }

    #[test]
    fn the_input_decides_by_being_wired_not_by_its_value() {
        let mut n = TriggerHoldProcessNode::new(NodeId(1));
        n.set_param(0, ParamValue::Float(5.0));
        assert_eq!(pulse_length(&mut n), 5);
        n.set_input_connections(&[false, true]);
        n.write_input(1, 0.0);
        assert_eq!(pulse_length(&mut n), 1, "a wired zero is the shortest hold, not the setting");
        n.write_input(1, 8.0);
        assert_eq!(pulse_length(&mut n), 8);
        n.set_input_connections(&[false, false]);
        assert_eq!(pulse_length(&mut n), 5);
    }

    #[test]
    fn the_duration_is_milliseconds_whatever_the_tick_length() {
        let mut n = TriggerHoldProcessNode::new(NodeId(1));
        n.set_param(0, ParamValue::Float(10.0));
        crate::engine::time::set_dt(0.002);
        let ticks = pulse_length(&mut n);
        crate::engine::time::set_dt(crate::engine::time::NOMINAL_DT);
        assert_eq!(ticks, 5, "10 ms at 2 ms a tick");
    }
}
