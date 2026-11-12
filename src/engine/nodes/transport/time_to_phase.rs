//! Turns a time into a phase: one run of 0..1 over the set time per trigger, or
//! a run that repeats without end. The time lives here and nowhere else, so
//! anything that wants a fade or a sweep takes a phase wire and this node
//! decides how long it lasts.

use crate::engine::types::*;

const DT: f32 = 0.001;
const MIN_TIME: f32 = 0.001;

pub struct TimeToPhaseProcessNode {
    id: NodeId,
    time: f32,
    looping: bool,
    trigger_in: f32,
    prev_trigger: bool,
    phase: f32,
    running: bool,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl TimeToPhaseProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            time: 1.0,
            looping: false,
            trigger_in: 0.0,
            prev_trigger: false,
            phase: 0.0,
            running: false,
            inputs: vec![PortDef::new("trigger", PortType::Logic)],
            outputs: vec![
                PortDef::new("phase", PortType::Phase),
                PortDef::new("running", PortType::Logic),
            ],
        }
    }
}

impl ProcessNode for TimeToPhaseProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Time to Phase" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.trigger_in = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.trigger_in } else { 0.0 }
    }

    fn process(&mut self) {
        let gate = self.trigger_in >= 0.5;
        if gate && !self.prev_trigger {
            self.phase = 0.0;
            self.running = true;
        }
        self.prev_trigger = gate;
        if self.looping {
            self.running = true;
        }
        if !self.running {
            return;
        }
        self.phase += DT / self.time.max(MIN_TIME);
        if self.phase >= 1.0 {
            if self.looping {
                self.phase = self.phase.fract();
            } else {
                self.phase = 1.0;
                self.running = false;
            }
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        match pi {
            0 => self.phase,
            1 => if self.running { 1.0 } else { 0.0 },
            _ => 0.0,
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float { name: "Time".into(), value: self.time, min: 0.01, max: 60.0, step: 0.01, unit: "s" },
            ParamDef::Bool { name: "Run continuously".into(), value: self.looping },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.time = value.as_f32().max(MIN_TIME),
            1 => self.looping = matches!(value, ParamValue::Bool(true)),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(n: &mut TimeToPhaseProcessNode, ticks: usize) {
        for _ in 0..ticks { n.process(); }
    }

    #[test]
    fn a_trigger_runs_one_period_and_holds() {
        let mut n = TimeToPhaseProcessNode::new(NodeId(1));
        n.set_param(0, ParamValue::Float(0.1));
        run(&mut n, 10);
        assert_eq!(n.read_output(0), 0.0);
        n.write_input(0, 1.0);
        run(&mut n, 50);
        assert!((n.read_output(0) - 0.5).abs() < 0.02);
        assert_eq!(n.read_output(1), 1.0);
        n.write_input(0, 0.0);
        run(&mut n, 100);
        assert_eq!(n.read_output(0), 1.0);
        assert_eq!(n.read_output(1), 0.0);
    }

    #[test]
    fn running_continuously_wraps() {
        let mut n = TimeToPhaseProcessNode::new(NodeId(1));
        n.set_param(0, ParamValue::Float(0.1));
        n.set_param(1, ParamValue::Bool(true));
        run(&mut n, 150);
        let p = n.read_output(0);
        assert!(p > 0.4 && p < 0.6, "{p}");
        assert_eq!(n.read_output(1), 1.0);
    }
}
