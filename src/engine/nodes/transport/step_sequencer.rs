use crate::engine::types::*;

const DEFAULT_STEPS: usize = 8;

/// Display state for the UI.
pub struct StepSequencerDisplay {
    pub values: Vec<f32>,
    pub current_step: usize,
    pub active: bool,
}

pub struct StepSequencerProcessNode {
    id: NodeId,
    values: Vec<f32>,
    current_step: usize,
    prev_step: Option<usize>,
    active: bool,
    phase_in: f32,
    trigger_out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl StepSequencerProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            values: vec![0.0; DEFAULT_STEPS],
            current_step: 0,
            prev_step: None,
            active: false,
            phase_in: 0.0,
            trigger_out: 0.0,
            inputs: vec![PortDef::new("phase", PortType::Phase)],
            outputs: vec![
                PortDef::new("trigger", PortType::Logic),
                PortDef::new("value", PortType::Untyped),
            ],
        }
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
}

impl ProcessNode for StepSequencerProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Step Sequencer" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn read_input(&self, port_index: usize) -> f32 {
        match port_index { 0 => self.phase_in, _ => 0.0 }
    }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if port_index == 0 {
            self.phase_in = value.rem_euclid(1.0);
            self.active = true;
        }
    }

    fn process(&mut self) {
        if !self.active {
            self.trigger_out = 0.0;
            return;
        }

        let step = (self.phase_in * self.num_steps() as f32).floor() as usize;
        let step = step.min(self.num_steps() - 1);

        if self.prev_step != Some(step) {
            self.current_step = step;
            self.trigger_out = 1.0;
            self.prev_step = Some(step);
        } else {
            self.trigger_out = 0.0;
        }
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.trigger_out,
            1 => self.values[self.current_step],
            _ => 0.0,
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Int {
            name: "Steps".into(),
            value: self.num_steps() as i64,
            min: 1,
            max: 64,
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match (index, &value) {
            (0, ParamValue::Int(v)) => self.set_num_steps(*v as usize),
            // Indices 100+ are step value edits from the UI faders.
            (i, ParamValue::Float(v)) if i >= 100 => {
                let step = i - 100;
                if step < self.values.len() {
                    self.values[step] = *v;
                }
            }
            _ => {}
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "values": self.values }))
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
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(StepSequencerDisplay {
            values: self.values.clone(),
            current_step: self.current_step,
            active: self.active,
        }));
    }
}
