use std::collections::VecDeque;

use crate::engine::types::*;

const MAX_SAMPLES: usize = 512;

/// Display state for the UI.
pub struct ScopeDisplay {
    pub buffers: [Vec<f32>; 2],
    pub connected_types: [Option<PortType>; 2],
}

pub struct ScopeProcessNode {
    id: NodeId,
    buffers: [VecDeque<f32>; 2],
    input_values: [f32; 2],
    trigger_threshold: f32,
    width_samples: usize,
    range_min: f32,
    range_max: f32,
    inputs: Vec<PortDef>,
    connected_types: [Option<PortType>; 2],
}

impl ScopeProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            buffers: [
                VecDeque::with_capacity(MAX_SAMPLES),
                VecDeque::with_capacity(MAX_SAMPLES),
            ],
            input_values: [0.0; 2],
            trigger_threshold: 0.5,
            width_samples: 200,
            range_min: 0.0,
            range_max: 1.0,
            inputs: vec![
                PortDef::new("in 1", PortType::Any),
                PortDef::new("in 2", PortType::Any),
            ],
            connected_types: [None, None],
        }
    }
}

impl ProcessNode for ScopeProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Scope" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn read_input(&self, port_index: usize) -> f32 {
        if port_index < 2 { self.input_values[port_index] } else { 0.0 }
    }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if port_index < 2 { self.input_values[port_index] = value; }
    }

    fn process(&mut self) {
        for ch in 0..2 {
            self.buffers[ch].push_back(self.input_values[ch]);
            while self.buffers[ch].len() > MAX_SAMPLES {
                self.buffers[ch].pop_front();
            }
        }
    }

    fn on_connect(&mut self, input_port: usize, source_type: PortType) {
        if input_port < 2 {
            self.connected_types[input_port] = Some(source_type);
            if self.connected_types.iter().filter(|t| t.is_some()).count() == 1 {
                let (lo, hi) = source_type.default_range();
                self.range_min = lo;
                self.range_max = hi;
            }
        }
    }

    fn on_disconnect(&mut self, input_port: usize) {
        if input_port < 2 {
            self.connected_types[input_port] = None;
            let other = 1 - input_port;
            if let Some(t) = self.connected_types[other] {
                let (lo, hi) = t.default_range();
                self.range_min = lo;
                self.range_max = hi;
            }
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float { name: "Threshold".into(), value: self.trigger_threshold, min: 0.0, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Int { name: "Width".into(), value: self.width_samples as i64, min: 50, max: MAX_SAMPLES as i64 },
            ParamDef::Float { name: "Range Min".into(), value: self.range_min, min: -2.0, max: 2.0, step: 0.05, unit: "" },
            ParamDef::Float { name: "Range Max".into(), value: self.range_max, min: -2.0, max: 2.0, step: 0.05, unit: "" },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match (index, value) {
            (0, ParamValue::Float(v)) => self.trigger_threshold = v,
            (1, ParamValue::Int(v)) => self.width_samples = v as usize,
            (2, ParamValue::Float(v)) => self.range_min = v,
            (3, ParamValue::Float(v)) => self.range_max = v,
            _ => {}
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(ScopeDisplay {
            buffers: [
                self.buffers[0].iter().copied().collect(),
                self.buffers[1].iter().copied().collect(),
            ],
            connected_types: self.connected_types,
        }));
    }
}
