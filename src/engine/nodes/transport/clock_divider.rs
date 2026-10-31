use crate::engine::types::*;

/// Outputs besides the first one.
pub const MAX_EXTRA_OUTPUTS: usize = 15;

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
    /// Further outputs, each with its own divisor, with the triggers each
    /// has counted and whether it fires this tick.
    extra: Vec<u32>,
    extra_count: Vec<u64>,
    extra_out: Vec<f32>,
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
            extra: Vec::new(),
            extra_count: Vec::new(),
            extra_out: Vec::new(),
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

        self.extra_out.iter_mut().for_each(|o| *o = 0.0);

        if gate_high && !self.prev_trigger {
            self.count += 1;
            if self.count >= self.divisor as u64 {
                self.trigger_out = 1.0;
                self.count = 0;
            }
            for ((divisor, count), out) in self.extra.iter().zip(&mut self.extra_count).zip(&mut self.extra_out) {
                *count += 1;
                if *count >= u64::from(*divisor) {
                    *out = 1.0;
                    *count = 0;
                }
            }
        }
        self.prev_trigger = gate_high;
    }

    fn read_output(&self, pi: usize) -> f32 {
        match pi {
            0 => self.trigger_out,
            i => self.extra_out.get(i - 1).copied().unwrap_or(0.0),
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "extra": self.extra }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(extra) = data.get("extra").and_then(|v| serde_json::from_value::<Vec<u32>>(v.clone()).ok()) {
            let extra: Vec<u32> = extra.into_iter().take(MAX_EXTRA_OUTPUTS).map(|d| d.clamp(2, 64)).collect();
            // A divisor that did not change keeps counting, so editing one
            // output does not restart the others.
            self.extra_count = extra
                .iter()
                .enumerate()
                .map(|(i, d)| if self.extra.get(i) == Some(d) { self.extra_count.get(i).copied().unwrap_or(0) } else { 0 })
                .collect();
            self.extra_out.resize(extra.len(), 0.0);
            self.extra = extra;
            self.outputs = std::iter::once(PortDef::new("trigger", PortType::Logic))
                .chain((0..self.extra.len()).map(|i| PortDef::new(format!("trigger {}", i + 2), PortType::Logic)))
                .collect();
        }
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
        if index == 0 {
            self.divisor = value.as_i64().max(2) as i32;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pulses(n: &mut ClockDividerProcessNode, count: usize) -> Vec<Vec<f32>> {
        (0..count)
            .map(|_| {
                n.write_input(0, 1.0);
                n.process();
                let fired = (0..n.outputs().len()).map(|i| n.read_output(i)).collect();
                n.write_input(0, 0.0);
                n.process();
                fired
            })
            .collect()
    }

    #[test]
    fn each_output_fires_on_every_nth_trigger_of_its_own() {
        let mut n = ClockDividerProcessNode::new(NodeId(1));
        n.load_data(&serde_json::json!({ "extra": [4, 3] }));
        assert_eq!(n.outputs().len(), 3);
        let fired = pulses(&mut n, 12);
        let column = |c: usize| fired.iter().map(|row| row[c]).collect::<Vec<_>>().iter().filter(|v| **v > 0.5).count();
        assert_eq!(column(0), 6, "every second");
        assert_eq!(column(1), 3, "every fourth");
        assert_eq!(column(2), 4, "every third");
        assert_eq!(fired[3], vec![1.0, 1.0, 0.0], "the fourth trigger fires ÷2 and ÷4 together");
    }

    #[test]
    fn extra_divisors_are_saved_and_kept_in_range_and_unchanged_ones_keep_counting() {
        let mut n = ClockDividerProcessNode::new(NodeId(1));
        n.load_data(&serde_json::json!({ "extra": [1, 500, 4] }));
        assert_eq!(n.save_data().unwrap()["extra"], serde_json::json!([2, 64, 4]));
        pulses(&mut n, 3);
        n.load_data(&serde_json::json!({ "extra": [2, 64, 8] }));
        assert_eq!(n.extra_count[1], 3, "the 64 kept its count");
        assert_eq!(n.extra_count[2], 0, "the changed one started again");
    }
}
