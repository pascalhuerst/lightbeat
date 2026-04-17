use crate::engine::types::*;

pub struct TriggerHoldDisplay {
    pub remaining_ticks: u32,
    pub effective_duration: u32,
}

/// Holds a trigger high for N engine ticks after each rising edge.
/// Duration is taken from the `duration` input (in ticks) when wired
/// (non-zero), otherwise falls back to the `Duration` param.
pub struct TriggerHoldProcessNode {
    id: NodeId,
    trigger_in: f32,
    prev_trigger: f32,
    duration_in: f32,
    /// Param fallback when input is 0 / unwired.
    default_duration: u32,
    remaining: u32,
    output: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl TriggerHoldProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            trigger_in: 0.0,
            prev_trigger: 0.0,
            duration_in: 0.0,
            default_duration: 100,
            remaining: 0,
            output: 0.0,
            inputs: vec![
                PortDef::new("trigger", PortType::Logic),
                PortDef::new("duration", PortType::Untyped),
            ],
            outputs: vec![PortDef::new("out", PortType::Logic)],
        }
    }

    fn effective_duration(&self) -> u32 {
        if self.duration_in > 0.0 {
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

    fn process(&mut self) {
        // Rising edge: (re)start the hold.
        if self.trigger_in >= 0.5 && self.prev_trigger < 0.5 {
            self.remaining = self.effective_duration();
        }
        self.prev_trigger = self.trigger_in;

        if self.remaining > 0 {
            self.output = 1.0;
            self.remaining -= 1;
        } else {
            self.output = 0.0;
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.output } else { 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Int {
            name: "Duration".into(),
            value: self.default_duration as i64,
            min: 1,
            max: 100_000,
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 {
            self.default_duration = (value.as_i64().max(1) as u32).min(100_000);
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
            remaining_ticks: self.remaining,
            effective_duration: self.effective_duration(),
        }));
    }
}
