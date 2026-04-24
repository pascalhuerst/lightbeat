use crate::engine::types::*;

pub struct PhaseScalerProcessNode {
    id: NodeId,
    exponent: i32,
    offset: f32,
    phase_in: f32,
    prev_phase_in: f32,
    reset_in: f32,
    prev_reset: f32,
    /// Last written value on the `exp` input (index 2). Interpreted as a
    /// 0..1 input mapped linearly onto the `-6..6` exponent range.
    exp_in: f32,
    exp_connected: bool,
    /// Last written value on the `offset` input (index 3). Phase-typed so
    /// 0..1 maps directly onto the offset range.
    offset_in: f32,
    offset_connected: bool,
    phase_out: f32,
    cycle_counter: u64,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl PhaseScalerProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            exponent: 0,
            offset: 0.0,
            phase_in: 0.0,
            prev_phase_in: 0.0,
            reset_in: 0.0,
            prev_reset: 0.0,
            exp_in: 0.5,
            exp_connected: false,
            offset_in: 0.0,
            offset_connected: false,
            phase_out: 0.0,
            cycle_counter: 0,
            inputs: vec![
                PortDef::new("phase", PortType::Phase),
                PortDef::new("reset", PortType::Logic),
                PortDef::new("exp", PortType::Untyped),
                PortDef::new("offset", PortType::Phase),
            ],
            outputs: vec![PortDef::new("phase", PortType::Phase)],
        }
    }
}

/// Display state for the UI.
pub struct PhaseScalerDisplay {
    pub exponent: i32,
}

impl ProcessNode for PhaseScalerProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Phase Scaler" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn read_input(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.phase_in,
            1 => self.reset_in,
            2 => self.exp_in,
            3 => self.offset_in,
            _ => 0.0,
        }
    }

    fn write_input(&mut self, port_index: usize, value: f32) {
        match port_index {
            0 => self.phase_in = value,
            1 => self.reset_in = value,
            2 => self.exp_in = value,
            3 => self.offset_in = value,
            _ => {}
        }
    }

    fn set_input_connections(&mut self, connected: &[bool]) {
        self.exp_connected = connected.get(2).copied().unwrap_or(false);
        self.offset_connected = connected.get(3).copied().unwrap_or(false);
    }

    fn process(&mut self) {
        // Resolve exponent + offset: wired input overrides the param.
        // Exponent input maps a 0..1 signal linearly onto -6..=6 (mid at
        // 0.5 → exponent 0, matches the param default).
        let exponent = if self.exp_connected {
            let mapped = (self.exp_in.clamp(0.0, 1.0) * 12.0 - 6.0).round() as i32;
            mapped.clamp(-6, 6)
        } else {
            self.exponent
        };
        let offset = if self.offset_connected {
            self.offset_in.rem_euclid(1.0)
        } else {
            self.offset
        };

        // Rising edge on `reset` resyncs the sub-cycle phase to the input:
        // clears the multi-cycle accumulator (only used in divide mode) and
        // latches `prev_phase_in` so the next tick doesn't see a spurious
        // wrap. After this, a divide-by-N scaler starts its next window
        // from sample 0, aligned to wherever the input's phase is now.
        let reset_edge = self.prev_reset < 0.5 && self.reset_in >= 0.5;
        self.prev_reset = self.reset_in;
        if reset_edge {
            self.cycle_counter = 0;
            self.prev_phase_in = self.phase_in;
        } else {
            let delta = self.phase_in - self.prev_phase_in;
            if delta < -0.5 { self.cycle_counter += 1; }
            self.prev_phase_in = self.phase_in;
        }

        if exponent >= 0 {
            let multiplier = (1u64 << exponent) as f32;
            self.phase_out = ((self.phase_in * multiplier) + offset).rem_euclid(1.0);
        } else {
            let divisor = (1u64 << (-exponent)) as f32;
            let sub_cycle = (self.cycle_counter % (divisor as u64)) as f32;
            self.phase_out = ((sub_cycle + self.phase_in) / divisor + offset).rem_euclid(1.0);
        }
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index { 0 => self.phase_out, _ => 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Int { name: "Exponent".into(), value: self.exponent as i64, min: -6, max: 6 },
            ParamDef::Float { name: "Offset".into(), value: self.offset, min: 0.0, max: 1.0, step: 0.01, unit: "" },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.exponent = value.as_i64() as i32,
            1 => self.offset = value.as_f32(),
            _ => {}
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(PhaseScalerDisplay { exponent: self.exponent }));
    }
}
