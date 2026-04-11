use crate::engine::types::*;

/// Generates N evenly spaced triggers per phase cycle.
/// E.g. count=4 fires at phase 0.0, 0.25, 0.5, 0.75.
pub struct ClockGenProcessNode {
    id: NodeId,
    count: i32,
    phase_in: f32,
    prev_phase: f32,
    trigger_out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl ClockGenProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            count: 4,
            phase_in: 0.0,
            prev_phase: 0.0,
            trigger_out: 0.0,
            inputs: vec![PortDef::new("phase", PortType::Phase)],
            outputs: vec![PortDef::new("trigger", PortType::Logic)],
        }
    }
}

pub struct ClockGenDisplay {
    pub count: i32,
}

impl ProcessNode for ClockGenProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Clock Gen" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.phase_in = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.phase_in } else { 0.0 }
    }

    fn process(&mut self) {
        self.trigger_out = 0.0;

        if self.count <= 0 {
            self.prev_phase = self.phase_in;
            return;
        }

        let step = 1.0 / self.count as f32;

        // Quantize current and previous phase to step boundaries.
        let cur_slot = (self.phase_in / step).floor() as i32;
        let prev_slot = (self.prev_phase / step).floor() as i32;

        // Detect if we crossed a step boundary (including phase wrap).
        let mut delta = self.phase_in - self.prev_phase;
        if delta < -0.5 { delta += 1.0; }

        if delta > 0.0 && cur_slot != prev_slot {
            self.trigger_out = 1.0;
        }

        // Also fire on phase wrap (0 crossing).
        if delta > 0.0 && self.phase_in < self.prev_phase && self.prev_phase > 0.5 {
            self.trigger_out = 1.0;
        }

        self.prev_phase = self.phase_in;
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.trigger_out } else { 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Int {
            name: "Count".into(),
            value: self.count as i64,
            min: 1,
            max: 64,
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 { self.count = value.as_i64().max(1) as i32; }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(ClockGenDisplay { count: self.count }));
    }
}
