use std::time::Instant;

use crate::engine::types::*;

/// Longest step the glide takes in one go; a stalled engine must not make
/// the output jump straight to its target.
const MAX_DT: f32 = 0.1;

/// Random. On each rising edge of the trigger, picks a new random value
/// between `min` and `max`. With `steps` of 2 or more the choice is one of
/// that many evenly spaced values, optionally never the same twice in a row;
/// `glide` slides the output to the new value instead of jumping.
pub struct RandomProcessNode {
    id: NodeId,
    trigger_in: f32,
    prev_trigger: bool,
    /// Rising edges counted in `write_input`, so a one-tick pulse is never
    /// lost even if `process` is skipped.
    pending: u32,
    min: f32,
    max: f32,
    steps: usize,
    no_repeat: bool,
    glide_ms: f32,
    /// The value drawn last, and the step it was (for `no_repeat`).
    target: f32,
    last_step: Option<usize>,
    out: f32,
    rng: u64,
    last_tick: Option<Instant>,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl RandomProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            trigger_in: 0.0,
            prev_trigger: false,
            pending: 0,
            min: 0.0,
            max: 1.0,
            steps: 0,
            no_repeat: true,
            glide_ms: 0.0,
            target: 0.0,
            last_step: None,
            out: 0.0,
            // Different nodes should not draw the same sequence.
            rng: 0x9E37_79B9_7F4A_7C15 ^ id.0.wrapping_mul(0xBF58_476D_1CE4_E5B9),
            last_tick: None,
            inputs: vec![PortDef::new("trigger", PortType::Logic)],
            outputs: vec![PortDef::new("out", PortType::Untyped)],
        }
    }

    /// Uniform in 0..1 (xorshift64*).
    fn random(&mut self) -> f32 {
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        let x = self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (x >> 40) as f32 / (1u64 << 24) as f32
    }

    fn draw(&mut self) {
        let t = if self.steps >= 2 {
            let n = self.steps;
            let mut step = ((self.random() * n as f32) as usize).min(n - 1);
            if self.no_repeat && self.last_step == Some(step) {
                // Skip over the repeat: one of the other n - 1 steps.
                step = (step + 1 + ((self.random() * (n - 1) as f32) as usize).min(n - 2)) % n;
            }
            self.last_step = Some(step);
            step as f32 / (n - 1) as f32
        } else {
            self.last_step = None;
            self.random()
        };
        self.target = self.min + t * (self.max - self.min);
    }
}

impl ProcessNode for RandomProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Random" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi != 0 { return; }
        self.trigger_in = v;
        let high = v >= 0.5;
        if high && !self.prev_trigger {
            self.pending += 1;
        }
        self.prev_trigger = high;
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.trigger_in } else { 0.0 }
    }

    fn process(&mut self) {
        let now = Instant::now();
        let dt = self.last_tick.replace(now).map_or(0.0, |t| (now - t).as_secs_f32()).min(MAX_DT);
        if self.pending > 0 {
            self.pending = 0;
            self.draw();
        }
        if self.glide_ms <= 0.0 {
            self.out = self.target;
        } else {
            let k = 1.0 - (-dt * 1000.0 / self.glide_ms).exp();
            self.out += (self.target - self.out) * k;
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.out } else { 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float { name: "Min".into(), value: self.min, min: -1000.0, max: 1000.0, step: 0.01, unit: "" },
            ParamDef::Float { name: "Max".into(), value: self.max, min: -1000.0, max: 1000.0, step: 0.01, unit: "" },
            ParamDef::Int { name: "Steps".into(), value: self.steps as i64, min: 0, max: 64 },
            ParamDef::Bool { name: "No repeat".into(), value: self.no_repeat },
            ParamDef::Float { name: "Glide".into(), value: self.glide_ms, min: 0.0, max: 5000.0, step: 1.0, unit: "ms" },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.min = value.as_f32(),
            1 => self.max = value.as_f32(),
            2 => self.steps = value.as_f32().max(0.0) as usize,
            3 => self.no_repeat = value.as_f32() >= 0.5,
            4 => self.glide_ms = value.as_f32().max(0.0),
            _ => {}
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "value": self.target }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(v) = data.get("value").and_then(|v| v.as_f64()) {
            self.target = v as f32;
            self.out = self.target;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node() -> RandomProcessNode {
        RandomProcessNode::new(NodeId(1))
    }

    fn pulse(n: &mut RandomProcessNode) -> f32 {
        n.write_input(0, 1.0);
        n.process();
        n.write_input(0, 0.0);
        n.process();
        n.read_output(0)
    }

    #[test]
    fn it_holds_its_value_until_triggered_and_stays_in_range() {
        let mut n = node();
        n.set_param(0, ParamValue::Float(-2.0));
        n.set_param(1, ParamValue::Float(3.0));
        let mut seen = Vec::new();
        for _ in 0..200 {
            let v = pulse(&mut n);
            assert!((-2.0..=3.0).contains(&v), "{v}");
            n.process();
            assert_eq!(n.read_output(0), v, "held between triggers");
            seen.push(v);
        }
        let (lo, hi) = seen.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        assert!(lo < -1.0 && hi > 2.0, "spread over the range ({lo}..{hi})");
    }

    #[test]
    fn steps_pick_from_evenly_spaced_values_and_never_repeat_when_asked() {
        let mut n = node();
        n.set_param(2, ParamValue::Int(4));
        let mut last = f32::NAN;
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..200 {
            let v = pulse(&mut n);
            assert!([0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0].iter().any(|s| (s - v).abs() < 1e-5), "{v}");
            assert_ne!(v, last, "repeated");
            last = v;
            seen.insert((v * 3.0).round() as i32);
        }
        assert_eq!(seen.len(), 4, "all steps get used");
        n.set_param(3, ParamValue::Bool(false));
        let repeats = (0..300).filter(|_| { let a = n.read_output(0); pulse(&mut n) == a }).count();
        assert!(repeats > 0, "repeats are allowed again");
    }

    #[test]
    fn a_pulse_shorter_than_a_tick_is_not_lost_and_nodes_differ() {
        let mut n = node();
        n.write_input(0, 1.0);
        n.write_input(0, 0.0);
        n.process();
        let a = n.read_output(0);
        let mut other = RandomProcessNode::new(NodeId(2));
        other.write_input(0, 1.0);
        other.process();
        assert!(a != 0.0 && a != other.read_output(0));
    }

    #[test]
    fn glide_slides_towards_the_new_value() {
        let mut n = node();
        n.set_param(4, ParamValue::Float(200.0));
        n.write_input(0, 1.0);
        n.process();
        let first = n.read_output(0);
        std::thread::sleep(std::time::Duration::from_millis(30));
        n.process();
        let later = n.read_output(0);
        assert!((later - n.target).abs() < (first - n.target).abs() || n.target == 0.0);
        assert!(later != n.target || first == n.target);
    }
}
