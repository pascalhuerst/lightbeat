use crate::engine::types::*;

const DEFAULT_STEPS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMode {
    /// Single Phase input — step index = floor(phase * num_steps).
    Phase,
    /// Single Logic input — current step advances by one on each rising edge.
    Trigger,
}

impl StepMode {
    pub const ALL: [Self; 2] = [Self::Phase, Self::Trigger];
    pub fn label(&self) -> &'static str {
        match self {
            StepMode::Phase => "Phase",
            StepMode::Trigger => "Trigger",
        }
    }
    pub fn to_index(&self) -> usize {
        Self::ALL.iter().position(|m| m == self).unwrap_or(0)
    }
    pub fn from_index(i: usize) -> Self {
        Self::ALL.get(i).copied().unwrap_or(Self::Phase)
    }
}

/// Display state for the UI.
pub struct StepSequencerDisplay {
    pub values: Vec<f32>,
    pub current_step: usize,
    pub active: bool,
    pub mode: StepMode,
}

pub struct StepSequencerProcessNode {
    id: NodeId,
    values: Vec<f32>,
    current_step: usize,
    prev_step: Option<usize>,
    active: bool,
    mode: StepMode,
    /// Phase mode: most recent phase 0..1.
    phase_in: f32,
    /// Trigger mode: most recent input value (rising-edge detected).
    trigger_in: f32,
    prev_trigger_in: f32,
    trigger_out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl StepSequencerProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mut node = Self {
            id,
            values: vec![0.0; DEFAULT_STEPS],
            current_step: 0,
            prev_step: None,
            active: false,
            mode: StepMode::Phase,
            phase_in: 0.0,
            trigger_in: 0.0,
            prev_trigger_in: 0.0,
            trigger_out: 0.0,
            inputs: Vec::new(),
            outputs: vec![
                PortDef::new("trigger", PortType::Logic),
                PortDef::new("value", PortType::Untyped),
                PortDef::new("step", PortType::Untyped),
            ],
        };
        node.rebuild_inputs();
        node
    }

    fn num_steps(&self) -> usize {
        self.values.len()
    }

    fn set_num_steps(&mut self, n: usize) {
        let n = n.max(1);
        self.values.resize(n, 0.0);
        if self.current_step >= n {
            self.current_step = 0;
            self.prev_step = None;
        }
    }

    fn rebuild_inputs(&mut self) {
        self.inputs = match self.mode {
            StepMode::Phase => vec![PortDef::new("phase", PortType::Phase)],
            StepMode::Trigger => vec![PortDef::new("trigger", PortType::Logic)],
        };
    }

    fn set_mode(&mut self, m: StepMode) {
        if m == self.mode { return; }
        self.mode = m;
        self.rebuild_inputs();
        self.phase_in = 0.0;
        self.trigger_in = 0.0;
        self.prev_trigger_in = 0.0;
        self.current_step = 0;
        self.prev_step = None;
        self.active = false;
        self.trigger_out = 0.0;
    }
}

impl ProcessNode for StepSequencerProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Step Sequencer" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn read_input(&self, port_index: usize) -> f32 {
        if port_index != 0 { return 0.0; }
        match self.mode {
            StepMode::Phase => self.phase_in,
            StepMode::Trigger => self.trigger_in,
        }
    }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if port_index != 0 { return; }
        match self.mode {
            StepMode::Phase => {
                self.phase_in = value.rem_euclid(1.0);
                self.active = true;
            }
            StepMode::Trigger => {
                self.trigger_in = value;
                self.active = true;
            }
        }
    }

    fn process(&mut self) {
        if !self.active {
            self.trigger_out = 0.0;
            return;
        }

        let new_step = match self.mode {
            StepMode::Phase => {
                let s = (self.phase_in * self.num_steps() as f32).floor() as usize;
                Some(s.min(self.num_steps() - 1))
            }
            StepMode::Trigger => {
                let prev = self.prev_trigger_in;
                self.prev_trigger_in = self.trigger_in;
                if self.trigger_in >= 0.5 && prev < 0.5 {
                    // First rising edge after activation lands on step 0;
                    // subsequent edges advance.
                    let next = if self.prev_step.is_none() {
                        0
                    } else {
                        (self.current_step + 1) % self.num_steps()
                    };
                    Some(next)
                } else {
                    None
                }
            }
        };

        if let Some(step) = new_step {
            if self.prev_step != Some(step) {
                self.current_step = step;
                self.trigger_out = 1.0;
                self.prev_step = Some(step);
            } else {
                self.trigger_out = 0.0;
            }
        } else {
            self.trigger_out = 0.0;
        }
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.trigger_out,
            1 => self.values[self.current_step],
            2 => self.current_step as f32,
            _ => 0.0,
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Int {
                name: "Steps".into(),
                value: self.num_steps() as i64,
                min: 1,
                max: 64,
            },
            ParamDef::Choice {
                name: "Mode".into(),
                value: self.mode.to_index(),
                options: StepMode::ALL.iter().map(|m| m.label().to_string()).collect(),
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.set_num_steps(value.as_i64() as usize),
            1 => self.set_mode(StepMode::from_index(value.as_i64() as usize)),
            i if i >= 100 => {
                let step = i - 100;
                if step < self.values.len() {
                    self.values[step] = value.as_f32();
                }
            }
            _ => {}
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "values": self.values,
            "mode": match self.mode {
                StepMode::Phase => "phase",
                StepMode::Trigger => "trigger",
            },
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(vals) = data.get("values").and_then(|v| v.as_array()) {
            self.values = vals
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();
            if self.values.is_empty() {
                self.values = vec![0.0; DEFAULT_STEPS];
            }
        }
        if let Some(s) = data.get("mode").and_then(|v| v.as_str()) {
            let m = match s {
                "trigger" => StepMode::Trigger,
                _ => StepMode::Phase,
            };
            self.set_mode(m);
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(StepSequencerDisplay {
            values: self.values.clone(),
            current_step: self.current_step,
            active: self.active,
            mode: self.mode,
        }));
    }
}
