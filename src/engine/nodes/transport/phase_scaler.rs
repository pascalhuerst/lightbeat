use serde::{Deserialize, Serialize};

use crate::engine::types::*;

/// Outputs besides the first one.
pub const MAX_EXTRA_OUTPUTS: usize = 15;

/// One more output with its own ratio, scaling the same input phase.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ExtraOutput {
    pub exponent: i32,
    #[serde(default)]
    pub offset: f32,
}

/// The phase `phase_in` becomes after multiplying (positive exponent) or
/// dividing (negative) its rate by two to the power of the exponent.
/// `cycle` counts the input's completed cycles, which a division needs to
/// know which part of the slower cycle it is in.
fn scale(phase_in: f32, cycle: u64, exponent: i32, offset: f32) -> f32 {
    if exponent >= 0 {
        let multiplier = (1u64 << exponent) as f32;
        ((phase_in * multiplier) + offset).rem_euclid(1.0)
    } else {
        let divisor = (1u64 << (-exponent)) as f32;
        let sub_cycle = (cycle % (divisor as u64)) as f32;
        ((sub_cycle + phase_in) / divisor + offset).rem_euclid(1.0)
    }
}

pub struct PhaseScalerProcessNode {
    id: NodeId,
    exponent: i32,
    offset: f32,
    phase_in: f32,
    prev_phase_in: f32,
    reset_in: f32,
    prev_reset: f32,
    /// Last written value on the `exp` input (index 2). Interpreted as
    /// the raw exponent value (rounded to the nearest integer and
    /// clamped into -6..=6), so `-3` on the wire gives exponent -3 —
    /// identical to setting the param to -3 with nothing wired.
    exp_in: f32,
    exp_connected: bool,
    /// Last written value on the `offset` input (index 3). Phase-typed so
    /// 0..1 maps directly onto the offset range.
    offset_in: f32,
    offset_connected: bool,
    phase_out: f32,
    cycle_counter: u64,
    /// Further outputs, each with its own ratio, and what they read now.
    extra: Vec<ExtraOutput>,
    extra_out: Vec<f32>,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl PhaseScalerProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            exponent: 0,
            offset: 0.0,
            phase_in: 0.0,
            prev_phase_in: 0.0,
            reset_in: 0.0,
            prev_reset: 0.0,
            exp_in: 0.0,
            exp_connected: false,
            offset_in: 0.0,
            offset_connected: false,
            phase_out: 0.0,
            cycle_counter: 0,
            extra: Vec::new(),
            extra_out: Vec::new(),
            inputs: vec![
                PortDef::new("phase", PortType::Phase),
                PortDef::new("reset", PortType::Logic),
                PortDef::new("exp", PortType::Untyped),
                PortDef::new("offset", PortType::Phase),
            ],
            outputs: vec![PortDef::new("phase", PortType::Phase)],
        }
    }
}

/// Display state for the UI.
pub struct PhaseScalerDisplay {
    pub exponent: i32,
}

impl ProcessNode for PhaseScalerProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Phase Scaler" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn read_input(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.phase_in,
            1 => self.reset_in,
            2 => self.exp_in,
            3 => self.offset_in,
            _ => 0.0,
        }
    }

    fn write_input(&mut self, port_index: usize, value: f32) {
        match port_index {
            0 => self.phase_in = value,
            1 => self.reset_in = value,
            2 => self.exp_in = value,
            3 => self.offset_in = value,
            _ => {}
        }
    }

    fn set_input_connections(&mut self, connected: &[bool]) {
        self.exp_connected = connected.get(2).copied().unwrap_or(false);
        self.offset_connected = connected.get(3).copied().unwrap_or(false);
    }

    fn process(&mut self) {
        // Resolve exponent + offset: wired input overrides the param.
        // Exponent input is taken directly — a wire carrying `-3` gives
        // exponent -3, matching the inspector's "Exponent" slider. The
        // value is rounded and clamped to the engine's supported range.
        let exponent = if self.exp_connected {
            (self.exp_in.round() as i32).clamp(-6, 6)
        } else {
            self.exponent
        };
        let offset = if self.offset_connected {
            self.offset_in.rem_euclid(1.0)
        } else {
            self.offset
        };

        // Rising edge on `reset` resyncs the sub-cycle phase to the input:
        // clears the multi-cycle accumulator (only used in divide mode) and
        // latches `prev_phase_in` so the next tick doesn't see a spurious
        // wrap. After this, a divide-by-N scaler starts its next window
        // from sample 0, aligned to wherever the input's phase is now.
        let reset_edge = self.prev_reset < 0.5 && self.reset_in >= 0.5;
        self.prev_reset = self.reset_in;
        if reset_edge {
            self.cycle_counter = 0;
            self.prev_phase_in = self.phase_in;
        } else {
            let delta = self.phase_in - self.prev_phase_in;
            if delta < -0.5 { self.cycle_counter += 1; }
            self.prev_phase_in = self.phase_in;
        }

        self.phase_out = scale(self.phase_in, self.cycle_counter, exponent, offset);
        for (out, x) in self.extra_out.iter_mut().zip(&self.extra) {
            *out = scale(self.phase_in, self.cycle_counter, x.exponent, x.offset);
        }
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.phase_out,
            i => self.extra_out.get(i - 1).copied().unwrap_or(0.0),
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "extra": self.extra }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(extra) = data.get("extra").and_then(|v| serde_json::from_value::<Vec<ExtraOutput>>(v.clone()).ok()) {
            self.extra = extra
                .into_iter()
                .take(MAX_EXTRA_OUTPUTS)
                .map(|x| ExtraOutput { exponent: x.exponent.clamp(-6, 6), offset: x.offset.rem_euclid(1.0) })
                .collect();
            self.extra_out.resize(self.extra.len(), 0.0);
            self.outputs = std::iter::once(PortDef::new("phase", PortType::Phase))
                .chain((0..self.extra.len()).map(|i| PortDef::new(format!("phase {}", i + 2), PortType::Phase)))
                .collect();
        }
    }

    fn non_promotable_params(&self) -> &'static [&'static str] {
        &["Exponent", "Offset"]
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Int { name: "Exponent".into(), value: self.exponent as i64, min: -6, max: 6 },
            ParamDef::Float { name: "Offset".into(), value: self.offset, min: 0.0, max: 1.0, step: 0.01, unit: "" },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.exponent = value.as_i64() as i32,
            1 => self.offset = value.as_f32(),
            _ => {}
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(PhaseScalerDisplay { exponent: self.exponent }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(n: &mut PhaseScalerProcessNode, phases: &[f32]) -> Vec<Vec<f32>> {
        phases
            .iter()
            .map(|&p| {
                n.write_input(0, p);
                n.process();
                (0..n.outputs().len()).map(|i| n.read_output(i)).collect()
            })
            .collect()
    }

    #[test]
    fn every_output_scales_the_same_phase_by_its_own_ratio() {
        let mut n = PhaseScalerProcessNode::new(NodeId(1));
        n.set_param(0, ParamValue::Int(-1));
        n.load_data(&serde_json::json!({ "extra": [{"exponent": -2}, {"exponent": 1}] }));
        assert_eq!(n.outputs().len(), 3);
        // Two input cycles: a quarter-way through the first, then the second.
        let out = run(&mut n, &[0.0, 0.5, 0.99, 0.0, 0.5]);
        let first_cycle = &out[1];
        assert!((first_cycle[0] - 0.25).abs() < 1e-6, "÷2 at half a cycle");
        assert!((first_cycle[1] - 0.125).abs() < 1e-6, "÷4 at half a cycle");
        assert!((first_cycle[2] - 0.0).abs() < 1e-6, "×2 wraps at half a cycle");
        let second_cycle = &out[4];
        assert!((second_cycle[0] - 0.75).abs() < 1e-6, "÷2 is in the second half");
        assert!((second_cycle[1] - 0.375).abs() < 1e-6, "÷4 is a quarter in");
    }

    #[test]
    fn extra_outputs_are_saved_clamped_and_capped() {
        let mut n = PhaseScalerProcessNode::new(NodeId(1));
        let many: Vec<_> = (0..40).map(|_| serde_json::json!({"exponent": 99, "offset": 1.25})).collect();
        n.load_data(&serde_json::json!({ "extra": many }));
        assert_eq!(n.outputs().len(), 1 + MAX_EXTRA_OUTPUTS);
        let saved = n.save_data().unwrap();
        assert_eq!(saved["extra"][0]["exponent"], 6);
        assert_eq!(saved["extra"][0]["offset"], 0.25);
        // A single-output node saves an empty list and reads past its outputs as 0.
        let mut single = PhaseScalerProcessNode::new(NodeId(2));
        single.load_data(&serde_json::json!({}));
        assert_eq!(single.save_data().unwrap()["extra"].as_array().map(Vec::len), Some(0));
        assert_eq!(single.read_output(3), 0.0);
    }

    #[test]
    fn the_reset_input_realigns_every_output() {
        let mut n = PhaseScalerProcessNode::new(NodeId(1));
        n.set_param(0, ParamValue::Int(-1));
        n.load_data(&serde_json::json!({ "extra": [{"exponent": -2}] }));
        run(&mut n, &[0.0, 0.9, 0.1, 0.9, 0.1]);
        n.write_input(1, 1.0);
        let out = run(&mut n, &[0.0]);
        assert_eq!(out[0], vec![0.0, 0.0], "both start their slow cycle from the beginning");
    }
}
