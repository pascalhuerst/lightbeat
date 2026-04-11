use crate::engine::types::*;

/// Detects whether an input value has changed since the last trigger.
/// On each trigger rising edge, compares current input to the snapshot
/// from the previous trigger. Outputs 1.0 if different, 0.0 if same.
pub struct ChangeDetectProcessNode {
    id: NodeId,
    trigger_in: f32,
    value_in: f32,
    prev_trigger: bool,
    snapshot: f32,
    output: f32,
    trigger_out: f32,
    threshold: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl ChangeDetectProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            trigger_in: 0.0,
            value_in: 0.0,
            prev_trigger: false,
            snapshot: 0.0,
            output: 0.0,
            trigger_out: 0.0,
            threshold: 0.001,
            inputs: vec![
                PortDef::new("trigger", PortType::Logic),
                PortDef::new("value", PortType::Any),
            ],
            outputs: vec![
                PortDef::new("changed", PortType::Logic),
                PortDef::new("trigger", PortType::Logic),
            ],
        }
    }
}

impl ProcessNode for ChangeDetectProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Change Detect" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        match pi { 0 => self.trigger_in = v, 1 => self.value_in = v, _ => {} }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi { 0 => self.trigger_in, 1 => self.value_in, _ => 0.0 }
    }

    fn process(&mut self) {
        let gate = self.trigger_in >= 0.5;
        self.trigger_out = 0.0;

        if gate && !self.prev_trigger {
            let changed = (self.value_in - self.snapshot).abs() > self.threshold;
            self.output = if changed { 1.0 } else { 0.0 };
            if changed {
                self.trigger_out = 1.0;
            }
            self.snapshot = self.value_in;
        }
        self.prev_trigger = gate;
    }

    fn read_output(&self, pi: usize) -> f32 {
        match pi {
            0 => self.output,
            1 => self.trigger_out,
            _ => 0.0,
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Float {
            name: "Threshold".into(),
            value: self.threshold,
            min: 0.0,
            max: 1.0,
            step: 0.001,
            unit: "",
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 { self.threshold = value.as_f32(); }
    }
}
