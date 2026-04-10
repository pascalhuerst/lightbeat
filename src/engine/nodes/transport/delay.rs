use crate::engine::types::*;

/// Delays a trigger by a fraction of a phase cycle.
/// Input: a Logic trigger signal.
/// The delay amount is specified as a fraction (numerator/denominator) of
/// the incoming phase cycle. When a rising edge is detected on the trigger
/// input, it captures the current phase and fires the output trigger
/// after the specified phase offset has elapsed.
pub struct TriggerDelayProcessNode {
    id: NodeId,
    // Params
    numerator: i64,
    denominator: i64,
    // State
    trigger_in: f32,
    phase_in: f32,
    prev_trigger: bool,
    /// Pending trigger: Some(phase_at_which_to_fire)
    pending: Option<f32>,
    prev_phase: f32,
    trigger_out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl TriggerDelayProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            numerator: 1,
            denominator: 4,
            trigger_in: 0.0,
            phase_in: 0.0,
            prev_trigger: false,
            pending: None,
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
        self.numerator as f32 / self.denominator.max(1) as f32
    }
}

/// Display state.
pub struct TriggerDelayDisplay {
    pub numerator: i64,
    pub denominator: i64,
    pub has_pending: bool,
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

        // Detect rising edge -> schedule delayed trigger.
        if gate_high && !self.prev_trigger {
            let fire_at = (self.phase_in + self.delay_amount()).rem_euclid(1.0);
            self.pending = Some(fire_at);
        }
        self.prev_trigger = gate_high;

        // Check if we've crossed the fire point.
        if let Some(fire_at) = self.pending {
            // Detect if the phase crossed the fire point since last tick.
            let crossed = if self.prev_phase <= fire_at {
                self.phase_in >= fire_at || self.phase_in < self.prev_phase // wrapped
            } else {
                self.phase_in >= fire_at && self.phase_in < self.prev_phase // wrapped back
            };

            if crossed {
                self.trigger_out = 1.0;
                self.pending = None;
            }
        }

        self.prev_phase = self.phase_in;
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index { 0 => self.trigger_out, _ => 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Int { name: "Numerator".into(), value: self.numerator, min: 1, max: 64 },
            ParamDef::Int { name: "Denominator".into(), value: self.denominator, min: 1, max: 64 },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match (index, value) {
            (0, ParamValue::Int(v)) => self.numerator = v.max(1),
            (1, ParamValue::Int(v)) => self.denominator = v.max(1),
            _ => {}
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(TriggerDelayDisplay {
            numerator: self.numerator,
            denominator: self.denominator,
            has_pending: self.pending.is_some(),
        }));
    }
}
