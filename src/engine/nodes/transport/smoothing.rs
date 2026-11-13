use std::collections::VecDeque;

use super::signal_type::{MAX_CHANNELS, publish_type, type_from_param, type_param};
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

/// A guard for the sample buffers when the loop runs far faster than 1 kHz.
const MAX_HISTORY: usize = 4000;

pub struct SmoothingProcessNode {
    id: NodeId,
    port_type: PortType,
    value_in: [f32; MAX_CHANNELS],
    value_out: [f32; MAX_CHANNELS],
    mode: SmoothingMode,
    /// Window length in milliseconds. It sets the exponential time constant
    /// and, divided by the measured tick length, the number of samples the
    /// averaging modes keep.
    window: f32,
    /// Ring buffer of the last `window` samples of each channel. Only
    /// populated when mode needs history (MovingAverage / Median).
    history: Vec<VecDeque<f32>>,
    /// Running sum for MovingAverage — avoids re-summing the window each tick.
    sum: Vec<f64>,
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

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports_for(port_type: PortType) -> (Vec<PortDef>, Vec<PortDef>) {
    (vec![PortDef::new("in", port_type)], vec![PortDef::new("out", port_type)])
}

impl SmoothingProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            port_type: PortType::Any,
            value_in: [0.0; MAX_CHANNELS],
            value_out: [0.0; MAX_CHANNELS],
            mode: SmoothingMode::Exponential,
            window: 32.0,
            history: vec![VecDeque::new(); MAX_CHANNELS],
            sum: vec![0.0; MAX_CHANNELS],
            scratch: Vec::new(),
            inputs: ports_for(PortType::Any).0,
            outputs: ports_for(PortType::Any).1,
        }
    }

    fn width(&self) -> usize { self.port_type.channel_count() }

    fn reset_history(&mut self) {
        for h in &mut self.history { h.clear(); }
        self.sum.fill(0.0);
    }
}

impl ProcessNode for SmoothingProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Smoothing" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi < self.width() { self.value_in[pi] = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi < self.width() { self.value_in[pi] } else { 0.0 }
    }

    fn process(&mut self) {
        // The history is a count of ticks, so it follows the real tick length:
        // a late loop then still averages over the same stretch of time.
        let tick_ms = crate::engine::time::dt() * 1000.0;
        let win = ((self.window / tick_ms).round() as usize).clamp(1, MAX_HISTORY);
        // One-pole IIR: y += α (x - y). α derived from `window` so the
        // 63 %-rise time matches the window length.
        let alpha = if win <= 1 { 1.0 } else { 1.0 - (-tick_ms / self.window).exp() };
        for c in 0..self.width() {
            let x = self.value_in[c];
            match self.mode {
                SmoothingMode::Exponential => {
                    self.value_out[c] += alpha * (x - self.value_out[c]);
                }
                SmoothingMode::MovingAverage => {
                    let history = &mut self.history[c];
                    history.push_back(x);
                    self.sum[c] += x as f64;
                    while history.len() > win {
                        if let Some(old) = history.pop_front() {
                            self.sum[c] -= old as f64;
                        }
                    }
                    let n = history.len().max(1);
                    self.value_out[c] = (self.sum[c] / n as f64) as f32;
                }
                SmoothingMode::Median => {
                    let history = &mut self.history[c];
                    history.push_back(x);
                    while history.len() > win {
                        history.pop_front();
                    }
                    self.scratch.clear();
                    self.scratch.extend(history.iter().copied());
                    self.value_out[c] = median_in_place(&mut self.scratch);
                }
            }
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi < self.width() { self.value_out[pi] } else { 0.0 }
    }

    // The type reshapes the ports; Mode and Window stay promotable.
    fn non_promotable_params(&self) -> &'static [&'static str] { &["Type"] }

    fn update_display(&self, shared: &mut NodeSharedState) {
        publish_type(shared, self.port_type);
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
            ParamDef::Float {
                name: "Window".into(),
                value: self.window,
                min: 1.0,
                max: 2000.0,
                step: 1.0,
                unit: "ms",
            },
            // Last, because saved settings go by position.
            type_param(self.port_type),
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
                let new_win = value.as_f32().round().clamp(1.0, 2000.0);
                if new_win != self.window {
                    self.window = new_win;
                    self.reset_history();
                }
            }
            2 => {
                let pt = type_from_param(&value);
                if pt != self.port_type {
                    self.port_type = pt;
                    (self.inputs, self.outputs) = ports_for(pt);
                    self.value_in.fill(0.0);
                    self.value_out.fill(0.0);
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
        n.set_param(1, ParamValue::Float(3.0));
        let mut out = Vec::new();
        for v in [1.0, 9.0, 2.0, 8.0] {
            n.write_input(0, v);
            n.process();
            out.push(n.read_output(0));
        }
        assert_eq!(out, vec![1.0, 5.0, 2.0, 8.0]);
    }

    #[test]
    fn a_color_is_smoothed_channel_by_channel() {
        let mut n = SmoothingProcessNode::new(NodeId(1));
        n.set_param(2, ParamValue::Choice(4));
        n.set_param(0, ParamValue::Choice(1));
        n.set_param(1, ParamValue::Float(2.0));
        assert_eq!(n.inputs()[0].port_type, PortType::Color);
        for (r, g) in [(1.0, 0.0), (0.0, 1.0)] {
            n.write_input(0, r);
            n.write_input(1, g);
            n.write_input(2, 0.5);
            n.process();
        }
        assert_eq!([n.read_output(0), n.read_output(1), n.read_output(2)], [0.5, 0.5, 0.5]);
    }

    #[test]
    fn the_window_is_milliseconds_whatever_the_tick_length() {
        let mut n = SmoothingProcessNode::new(NodeId(1));
        n.set_param(0, ParamValue::Choice(1));
        n.set_param(1, ParamValue::Float(4.0));
        crate::engine::time::set_dt(0.002);
        let mut out = Vec::new();
        for v in [1.0, 0.0, 0.0] {
            n.write_input(0, v);
            n.process();
            out.push(n.read_output(0));
        }
        crate::engine::time::set_dt(crate::engine::time::NOMINAL_DT);
        assert_eq!(out, vec![1.0, 0.5, 0.0], "4 ms at 2 ms a tick is two samples");
    }
}
