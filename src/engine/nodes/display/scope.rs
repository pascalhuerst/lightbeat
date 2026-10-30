use std::collections::VecDeque;

use crate::engine::types::*;

const MAX_SAMPLES: usize = 512;

/// Number of traces a scope can show.
pub const SCOPE_INPUTS: usize = 8;

/// Display state for the UI.
pub struct ScopeDisplay {
    pub buffers: [Vec<f32>; SCOPE_INPUTS],
    pub connected_types: [Option<PortType>; SCOPE_INPUTS],
}

pub struct ScopeProcessNode {
    id: NodeId,
    buffers: [VecDeque<f32>; SCOPE_INPUTS],
    input_values: [f32; SCOPE_INPUTS],
    /// Per-channel "extreme value seen during the current decimation period".
    /// Tracks the sample with the largest absolute value so single-tick
    /// trigger pulses survive decimation (otherwise a 1-tick high pulse
    /// landing between two sample-tick boundaries would be invisible).
    period_peak: [f32; SCOPE_INPUTS],
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
    connected_types: [Option<PortType>; SCOPE_INPUTS],
}

impl ScopeProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            buffers: std::array::from_fn(|_| VecDeque::with_capacity(MAX_SAMPLES)),
            input_values: [0.0; SCOPE_INPUTS],
            period_peak: [0.0; SCOPE_INPUTS],
            width_samples: 200,
            range_min: 0.0,
            range_max: 1.0,
            time_scale: 1.0,
            tick_counter: 0,
            inputs: (1..=SCOPE_INPUTS)
                .map(|i| PortDef::new(format!("in {i}"), PortType::Any))
                .collect(),
            connected_types: [None; SCOPE_INPUTS],
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
        self.input_values.get(port_index).copied().unwrap_or(0.0)
    }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if let Some(slot) = self.input_values.get_mut(port_index) {
            *slot = value;
        }
    }

    fn process(&mut self) {
        // Track the largest-magnitude sample seen during the current
        // decimation period (signed). For period=1 this is just the latest
        // value; for period>1 it preserves single-tick spikes that would
        // otherwise fall between sample-tick boundaries.
        for ch in 0..SCOPE_INPUTS {
            if self.connected_types[ch].is_none() {
                continue;
            }
            let v = self.input_values[ch];
            if v.abs() > self.period_peak[ch].abs() {
                self.period_peak[ch] = v;
            }
        }

        let period = self.decimation_period();
        self.tick_counter = self.tick_counter.wrapping_add(1);
        if !self.tick_counter.is_multiple_of(period) {
            return;
        }
        for ch in 0..SCOPE_INPUTS {
            if self.connected_types[ch].is_none() {
                continue;
            }
            self.buffers[ch].push_back(self.period_peak[ch]);
            while self.buffers[ch].len() > MAX_SAMPLES {
                self.buffers[ch].pop_front();
            }
            self.period_peak[ch] = self.input_values[ch];
        }
    }

    fn on_connect(&mut self, input_port: usize, source_type: PortType) {
        if input_port < SCOPE_INPUTS {
            self.connected_types[input_port] = Some(source_type);
            // The first signal sets the range; later ones are plotted on it.
            if self.connected_types.iter().filter(|t| t.is_some()).count() == 1 {
                let (lo, hi) = source_type.default_range();
                self.range_min = lo;
                self.range_max = hi;
            }
        }
    }

    fn on_disconnect(&mut self, input_port: usize) {
        if input_port < SCOPE_INPUTS {
            self.connected_types[input_port] = None;
            self.buffers[input_port].clear();
            self.period_peak[input_port] = 0.0;
            if let Some(t) = self.connected_types.iter().flatten().next() {
                let (lo, hi) = t.default_range();
                self.range_min = lo;
                self.range_max = hi;
            }
        }
    }

    fn non_promotable_params(&self) -> &'static [&'static str] {
        &["Time/Div", "Width", "Range Min", "Range Max"]
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
            buffers: std::array::from_fn(|i| self.buffers[i].iter().copied().collect()),
            connected_types: self.connected_types,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_eight_inputs_record_independently() {
        let mut scope = ScopeProcessNode::new(NodeId(1));
        assert_eq!(scope.inputs().len(), SCOPE_INPUTS);
        for ch in 0..SCOPE_INPUTS {
            scope.on_connect(ch, PortType::Untyped);
            scope.write_input(ch, ch as f32 * 0.1);
        }
        scope.process();
        scope.process();
        for ch in 0..SCOPE_INPUTS {
            assert_eq!(scope.buffers[ch].back().copied(), Some(ch as f32 * 0.1), "channel {ch}");
        }
    }

    #[test]
    fn unconnected_inputs_record_nothing() {
        let mut scope = ScopeProcessNode::new(NodeId(1));
        scope.on_connect(5, PortType::Untyped);
        scope.write_input(5, 0.5);
        scope.write_input(6, 0.9);
        scope.process();
        assert_eq!(scope.buffers[5].len(), 1);
        assert!(scope.buffers[6].is_empty());
    }

    #[test]
    fn disconnecting_clears_the_trace_and_keeps_the_remaining_range() {
        let mut scope = ScopeProcessNode::new(NodeId(1));
        scope.on_connect(0, PortType::Untyped);
        scope.on_connect(3, PortType::Logic);
        scope.process();
        scope.on_disconnect(0);
        assert!(scope.buffers[0].is_empty());
        // The Logic input is all that is left, so the range follows it.
        assert_eq!((scope.range_min, scope.range_max), (0.0, 1.0));
    }
}
