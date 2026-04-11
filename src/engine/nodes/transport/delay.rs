use crate::engine::types::*;

/// Display state.
pub struct TriggerDelayDisplay {
    pub exponent: i32,
    pub has_pending: bool,
}

pub struct TriggerDelayProcessNode {
    id: NodeId,
    /// Power of 2 exponent. 0 = 1 beat, 1 = 2 beats, -1 = 1/2 beat, etc.
    exponent: i32,
    // State
    trigger_in: f32,
    phase_in: f32,
    prev_trigger: bool,
    /// Remaining delay in phase units. None = no pending trigger.
    remaining: Option<f32>,
    prev_phase: f32,
    trigger_out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl TriggerDelayProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            exponent: -2, // default: 1/4 beat
            trigger_in: 0.0,
            phase_in: 0.0,
            prev_trigger: false,
            remaining: None,
            prev_phase: 0.0,
            trigger_out: 0.0,
            inputs: vec![
                PortDef::new("trigger", PortType::Logic),
                PortDef::new("phase", PortType::Phase),
            ],
            outputs: vec![PortDef::new("trigger", PortType::Logic)],
        }
    }

    fn delay_amount(&self) -> f32 {
        2.0_f32.powi(self.exponent)
    }
}

impl ProcessNode for TriggerDelayProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Trigger Delay" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, port_index: usize, value: f32) {
        match port_index {
            0 => self.trigger_in = value,
            1 => self.phase_in = value,
            _ => {}
        }
    }

    fn read_input(&self, port_index: usize) -> f32 {
        match port_index { 0 => self.trigger_in, 1 => self.phase_in, _ => 0.0 }
    }

    fn process(&mut self) {
        let gate_high = self.trigger_in >= 0.5;
        self.trigger_out = 0.0;

        // On rising edge, (re)start the delay. If already pending, reset
        // the timer — this coalesces multiple triggers into one output.
        if gate_high && !self.prev_trigger {
            self.remaining = Some(self.delay_amount());
        }
        self.prev_trigger = gate_high;

        // Count down remaining delay using phase delta.
        if let Some(rem) = &mut self.remaining {
            let mut delta = self.phase_in - self.prev_phase;
            if delta < -0.5 {
                delta += 1.0;
            }
            if delta > 0.0 {
                *rem -= delta;
            }
            if *rem <= 0.0 {
                self.trigger_out = 1.0;
                self.remaining = None;
            }
        }

        self.prev_phase = self.phase_in;
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index { 0 => self.trigger_out, _ => 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Int {
            name: "Exponent".into(),
            value: self.exponent as i64,
            min: -6,
            max: 6,
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if let (0, ParamValue::Int(v)) = (index, value) {
            self.exponent = v as i32;
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(TriggerDelayDisplay {
            exponent: self.exponent,
            has_pending: self.remaining.is_some(),
        }));
    }
}
