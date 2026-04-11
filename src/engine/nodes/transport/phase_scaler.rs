use crate::engine::types::*;

pub struct PhaseScalerProcessNode {
    id: NodeId,
    exponent: i32,
    offset: f32,
    phase_in: f32,
    prev_phase_in: f32,
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
            phase_out: 0.0,
            cycle_counter: 0,
            inputs: vec![PortDef::new("phase", PortType::Phase)],
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
        match port_index { 0 => self.phase_in, _ => 0.0 }
    }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if port_index == 0 { self.phase_in = value; }
    }

    fn process(&mut self) {
        let delta = self.phase_in - self.prev_phase_in;
        if delta < -0.5 { self.cycle_counter += 1; }
        self.prev_phase_in = self.phase_in;

        if self.exponent >= 0 {
            let multiplier = (1u64 << self.exponent) as f32;
            self.phase_out = ((self.phase_in * multiplier) + self.offset).rem_euclid(1.0);
        } else {
            let divisor = (1u64 << (-self.exponent)) as f32;
            let sub_cycle = (self.cycle_counter % (divisor as u64)) as f32;
            self.phase_out = ((sub_cycle + self.phase_in) / divisor + self.offset).rem_euclid(1.0);
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
