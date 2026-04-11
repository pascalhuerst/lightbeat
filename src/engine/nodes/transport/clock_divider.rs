use crate::engine::types::*;

pub struct ClockDividerDisplay {
    pub divisor: i32,
    pub count: u64,
}

pub struct ClockDividerProcessNode {
    id: NodeId,
    divisor: i32,
    trigger_in: f32,
    prev_trigger: bool,
    count: u64,
    trigger_out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl ClockDividerProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            divisor: 2,
            trigger_in: 0.0,
            prev_trigger: false,
            count: 0,
            trigger_out: 0.0,
            inputs: vec![PortDef::new("trigger", PortType::Logic)],
            outputs: vec![PortDef::new("trigger", PortType::Logic)],
        }
    }
}

impl ProcessNode for ClockDividerProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Clock Divider" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.trigger_in = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.trigger_in } else { 0.0 }
    }

    fn process(&mut self) {
        let gate_high = self.trigger_in >= 0.5;
        self.trigger_out = 0.0;

        if gate_high && !self.prev_trigger {
            self.count += 1;
            if self.count >= self.divisor as u64 {
                self.trigger_out = 1.0;
                self.count = 0;
            }
        }
        self.prev_trigger = gate_high;
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.trigger_out } else { 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Int {
            name: "Every Nth".into(),
            value: self.divisor as i64,
            min: 2,
            max: 64,
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if let (0, ParamValue::Int(v)) = (index, value) {
            self.divisor = v.max(2) as i32;
            self.count = 0;
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(ClockDividerDisplay {
            divisor: self.divisor,
            count: self.count,
        }));
    }
}
