use crate::engine::types::*;

/// Detects when an input `value` changes. Two modes, selected
/// automatically by whether the `trigger` input is wired:
///
/// - **Gated** (trigger wired): on each `trigger` rising edge, compare
///   current `value` to the snapshot taken on the previous trigger.
///   `changed` latches to 1 if different, 0 if same (holds until the
///   next trigger). `trigger` output pulses one tick when a change is
///   detected.
/// - **Continuous** (trigger unwired): compare `value` to the previous
///   tick's value. Both `changed` and `trigger` outputs pulse one tick
///   whenever the value shifts more than `threshold`.
pub struct ChangeDetectProcessNode {
    id: NodeId,
    trigger_in: f32,
    value_in: f32,
    trigger_connected: bool,
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
            trigger_connected: false,
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

    fn set_input_connections(&mut self, connected: &[bool]) {
        self.trigger_connected = connected.first().copied().unwrap_or(false);
    }

    fn process(&mut self) {
        self.trigger_out = 0.0;

        if self.trigger_connected {
            // Gated mode — only check on trigger rising edges.
            let gate = self.trigger_in >= 0.5;
            if gate && !self.prev_trigger {
                let changed = (self.value_in - self.snapshot).abs() > self.threshold;
                self.output = if changed { 1.0 } else { 0.0 };
                if changed {
                    self.trigger_out = 1.0;
                }
                self.snapshot = self.value_in;
            }
            self.prev_trigger = gate;
        } else {
            // Continuous mode — pulse whenever the value shifts between
            // consecutive ticks. Both outputs carry the same one-tick
            // pulse; `snapshot` holds the previous-tick value.
            let changed = (self.value_in - self.snapshot).abs() > self.threshold;
            self.output = if changed { 1.0 } else { 0.0 };
            self.trigger_out = self.output;
            self.snapshot = self.value_in;
            self.prev_trigger = false;
        }
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
