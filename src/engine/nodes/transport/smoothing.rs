use std::collections::VecDeque;

use crate::engine::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmoothingMode {
    Exponential,
    MovingAverage,
    Median,
}

impl SmoothingMode {
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => Self::MovingAverage,
            2 => Self::Median,
            _ => Self::Exponential,
        }
    }
    pub fn to_index(&self) -> usize {
        match self {
            Self::Exponential => 0,
            Self::MovingAverage => 1,
            Self::Median => 2,
        }
    }
}

pub struct SmoothingProcessNode {
    id: NodeId,
    value_in: f32,
    value_out: f32,
    mode: SmoothingMode,
    /// Window length in samples (engine ticks ≈ 1 kHz, so window = ms).
    /// Also controls the exponential time constant: α = 1 - exp(-1/window).
    window: usize,
    /// Ring buffer of the last `window` samples. Only populated when mode
    /// needs history (MovingAverage / Median).
    history: VecDeque<f32>,
    /// Running sum for MovingAverage — avoids re-summing the window each tick.
    sum: f64,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl SmoothingProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            value_in: 0.0,
            value_out: 0.0,
            mode: SmoothingMode::Exponential,
            window: 32,
            history: VecDeque::new(),
            sum: 0.0,
            inputs: vec![PortDef::new("in", PortType::Untyped)],
            outputs: vec![PortDef::new("out", PortType::Untyped)],
        }
    }

    fn reset_history(&mut self) {
        self.history.clear();
        self.sum = 0.0;
    }
}

impl ProcessNode for SmoothingProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Smoothing" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.value_in = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.value_in } else { 0.0 }
    }

    fn process(&mut self) {
        let win = self.window.max(1);
        match self.mode {
            SmoothingMode::Exponential => {
                // One-pole IIR: y += α (x - y). α derived from `window` so the
                // 63 %-rise time matches the window length in ticks.
                let alpha = if win <= 1 {
                    1.0
                } else {
                    1.0 - (-1.0 / win as f32).exp()
                };
                self.value_out += alpha * (self.value_in - self.value_out);
            }
            SmoothingMode::MovingAverage => {
                self.history.push_back(self.value_in);
                self.sum += self.value_in as f64;
                while self.history.len() > win {
                    if let Some(old) = self.history.pop_front() {
                        self.sum -= old as f64;
                    }
                }
                let n = self.history.len().max(1);
                self.value_out = (self.sum / n as f64) as f32;
            }
            SmoothingMode::Median => {
                self.history.push_back(self.value_in);
                while self.history.len() > win {
                    self.history.pop_front();
                }
                let mut buf: Vec<f32> = self.history.iter().copied().collect();
                buf.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let mid = buf.len() / 2;
                self.value_out = if buf.len() % 2 == 1 {
                    buf[mid]
                } else {
                    0.5 * (buf[mid - 1] + buf[mid])
                };
            }
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.value_out } else { 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Choice {
                name: "Mode".into(),
                value: self.mode.to_index(),
                options: vec![
                    "Exponential".into(),
                    "Moving Average".into(),
                    "Median".into(),
                ],
            },
            ParamDef::Int {
                name: "Window".into(),
                value: self.window as i64,
                min: 1,
                max: 2000,
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => {
                let new_mode = SmoothingMode::from_index(value.as_usize());
                if new_mode != self.mode {
                    self.mode = new_mode;
                    self.reset_history();
                }
            }
            1 => {
                let new_win = value.as_i64().clamp(1, 2000) as usize;
                if new_win != self.window {
                    self.window = new_win;
                    self.reset_history();
                }
            }
            _ => {}
        }
    }
}
