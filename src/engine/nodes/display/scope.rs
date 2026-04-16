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
    width_samples: usize,
    range_min: f32,
    range_max: f32,
    /// Time-per-division multiplier. 1.0 = default sampling rate. Lower
    /// values decimate sampling (slower cursor sweep), useful when watching
    /// signals that change very slowly.
    time_scale: f32,
    /// Tick counter for decimation.
    tick_counter: u32,
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
            width_samples: 200,
            range_min: 0.0,
            range_max: 1.0,
            time_scale: 1.0,
            tick_counter: 0,
            inputs: vec![
                PortDef::new("in 1", PortType::Any),
                PortDef::new("in 2", PortType::Any),
            ],
            connected_types: [None, None],
        }
    }

    fn decimation_period(&self) -> u32 {
        if self.time_scale < 1.0 {
            (1.0 / self.time_scale).round().max(1.0) as u32
        } else {
            1
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
        let period = self.decimation_period();
        self.tick_counter = self.tick_counter.wrapping_add(1);
        if self.tick_counter % period != 0 {
            return;
        }
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
            ParamDef::Float { name: "Time/Div".into(), value: self.time_scale, min: 0.125, max: 4.0, step: 0.125, unit: "x" },
            ParamDef::Int { name: "Width".into(), value: self.width_samples as i64, min: 50, max: MAX_SAMPLES as i64 },
            ParamDef::Float { name: "Range Min".into(), value: self.range_min, min: -2.0, max: 2.0, step: 0.05, unit: "" },
            ParamDef::Float { name: "Range Max".into(), value: self.range_max, min: -2.0, max: 2.0, step: 0.05, unit: "" },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => {
                let new_scale = value.as_f32().clamp(0.125, 4.0);
                if (new_scale - self.time_scale).abs() > f32::EPSILON {
                    self.time_scale = new_scale;
                    self.tick_counter = 0;
                }
            }
            1 => self.width_samples = value.as_i64() as usize,
            2 => self.range_min = value.as_f32(),
            3 => self.range_max = value.as_f32(),
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
