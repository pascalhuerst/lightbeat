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
    /// Reused by the median so it does not allocate every tick.
    scratch: Vec<f32>,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

/// Median of `buf` (reordered in the process); the mean of the two middle
/// values for an even length. A selection, not a sort, since this runs every tick.
fn median_in_place(buf: &mut [f32]) -> f32 {
    let cmp = |a: &f32, b: &f32| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal);
    let len = buf.len();
    let mid = len / 2;
    let (lower, upper, _) = buf.select_nth_unstable_by(mid, cmp);
    let upper = *upper;
    if len % 2 == 1 {
        upper
    } else {
        let below = lower.iter().copied().max_by(cmp).unwrap_or(upper);
        0.5 * (below + upper)
    }
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
            scratch: Vec::new(),
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
                self.scratch.clear();
                self.scratch.extend(self.history.iter().copied());
                self.value_out = median_in_place(&mut self.scratch);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn by_sorting(values: &[f32]) -> f32 {
        let mut buf = values.to_vec();
        buf.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mid = buf.len() / 2;
        if buf.len() % 2 == 1 { buf[mid] } else { 0.5 * (buf[mid - 1] + buf[mid]) }
    }

    #[test]
    fn the_median_is_the_middle_value_for_odd_and_even_windows() {
        let data = [0.9, 0.1, 0.5, 0.5, -0.3, 0.7, 0.2, 0.2, 1.0, 0.0];
        for len in 1..=data.len() {
            let mut buf = data[..len].to_vec();
            assert_eq!(median_in_place(&mut buf), by_sorting(&data[..len]), "window of {len}");
        }
        assert_eq!(median_in_place(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median_in_place(&mut [4.0, 1.0, 3.0, 2.0]), 2.5);
    }

    #[test]
    fn the_node_outputs_the_median_of_its_window() {
        let mut n = SmoothingProcessNode::new(NodeId(1));
        n.set_param(0, ParamValue::Choice(2));
        n.set_param(1, ParamValue::Int(3));
        let mut out = Vec::new();
        for v in [1.0, 9.0, 2.0, 8.0] {
            n.write_input(0, v);
            n.process();
            out.push(n.read_output(0));
        }
        assert_eq!(out, vec![1.0, 5.0, 2.0, 8.0]);
    }
}
