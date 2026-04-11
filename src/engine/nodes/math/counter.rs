use crate::engine::types::*;

pub struct CounterProcessNode {
    id: NodeId,
    max_value: i64,
    direction: i64, // 1 = up, -1 = down
    count: i64,
    wrap_out: f32,
    trigger_in: f32,
    reset_in: f32,
    prev_trigger: bool,
    prev_reset: bool,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl CounterProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            max_value: 8,
            direction: 1,
            count: 0,
            wrap_out: 0.0,
            trigger_in: 0.0,
            reset_in: 0.0,
            prev_trigger: false,
            prev_reset: false,
            inputs: vec![
                PortDef::new("trigger", PortType::Logic),
                PortDef::new("reset", PortType::Logic),
            ],
            outputs: vec![
                PortDef::new("count", PortType::Untyped),
                PortDef::new("wrap", PortType::Logic),
            ],
        }
    }
}

impl ProcessNode for CounterProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Counter" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        match pi { 0 => self.trigger_in = v, 1 => self.reset_in = v, _ => {} }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi { 0 => self.trigger_in, 1 => self.reset_in, _ => 0.0 }
    }

    fn process(&mut self) {
        let trigger = self.trigger_in >= 0.5;
        let reset = self.reset_in >= 0.5;
        self.wrap_out = 0.0;

        if reset && !self.prev_reset {
            self.count = if self.direction > 0 { 0 } else { self.max_value - 1 };
        }
        self.prev_reset = reset;

        if trigger && !self.prev_trigger {
            self.count += self.direction;
            if self.count >= self.max_value {
                self.count = 0;
                self.wrap_out = 1.0;
            } else if self.count < 0 {
                self.count = self.max_value - 1;
                self.wrap_out = 1.0;
            }
        }
        self.prev_trigger = trigger;
    }

    fn read_output(&self, pi: usize) -> f32 {
        match pi {
            0 => self.count as f32,
            1 => self.wrap_out,
            _ => 0.0,
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Int { name: "Max".into(), value: self.max_value, min: 1, max: 256 },
            ParamDef::Choice {
                name: "Direction".into(),
                value: if self.direction > 0 { 0 } else { 1 },
                options: vec!["Up".into(), "Down".into()],
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => {
                self.max_value = value.as_i64().max(1);
                if self.count >= self.max_value { self.count = 0; }
            }
            1 => self.direction = if value.as_usize() == 0 { 1 } else { -1 },
            _ => {}
        }
    }
}
