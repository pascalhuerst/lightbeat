//! Gain — multiplies the input by a 0..1 scalar. Simple attenuator; the
//! scalar is both editable as a param and driven by a knob in the node
//! body.

use crate::engine::types::*;

pub struct GainDisplay {
    pub gain: f32,
    pub input: f32,
    pub output: f32,
}

pub struct GainProcessNode {
    id: NodeId,
    input: f32,
    gain: f32,
    output: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl GainProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            input: 0.0,
            gain: 1.0,
            output: 0.0,
            inputs: vec![PortDef::new("in", PortType::Untyped)],
            outputs: vec![PortDef::new("out", PortType::Untyped)],
        }
    }
}

impl ProcessNode for GainProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Gain" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.input = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.input } else { 0.0 }
    }

    fn process(&mut self) {
        self.output = self.input * self.gain;
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.output } else { 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Float {
            name: "Gain".into(),
            value: self.gain,
            min: 0.0,
            max: 1.0,
            step: 0.01,
            unit: "",
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 {
            self.gain = value.as_f32().clamp(0.0, 1.0);
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "gain": self.gain }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(v) = data.get("gain").and_then(|v| v.as_f64()) {
            self.gain = (v as f32).clamp(0.0, 1.0);
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(GainDisplay {
            gain: self.gain,
            input: self.input,
            output: self.output,
        }));
    }
}
