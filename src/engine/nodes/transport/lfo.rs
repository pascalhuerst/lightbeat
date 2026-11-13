use std::time::Instant;

use crate::engine::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfoWaveform {
    Sine,
    Triangle,
    Saw,
    ReverseSaw,
    Square,
    /// Sample-and-hold random — picks a new random value at each cycle start.
    Random,
}

impl LfoWaveform {
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => LfoWaveform::Sine,
            1 => LfoWaveform::Triangle,
            2 => LfoWaveform::Saw,
            3 => LfoWaveform::ReverseSaw,
            4 => LfoWaveform::Square,
            5 => LfoWaveform::Random,
            _ => LfoWaveform::Sine,
        }
    }
    pub fn to_index(&self) -> usize {
        match self {
            LfoWaveform::Sine => 0,
            LfoWaveform::Triangle => 1,
            LfoWaveform::Saw => 2,
            LfoWaveform::ReverseSaw => 3,
            LfoWaveform::Square => 4,
            LfoWaveform::Random => 5,
        }
    }
}

pub struct LfoDisplay {
    pub rate_hz: f32,
    pub phase: f32,
    pub value: f32,
}

/// Free-running low-frequency oscillator.
/// Outputs both a waveform value and a 0..1 phase signal.
pub struct LfoProcessNode {
    id: NodeId,
    rate_hz: f32,
    waveform: LfoWaveform,
    /// If true, output is mapped from -1..1 to 0..1.
    unipolar: bool,
    /// When true the phase comes from a wire and the oscillator no longer
    /// runs on its own: no sync input, no phase output.
    phase_input: bool,
    phase_in: f32,
    sync_in: f32,
    prev_sync: f32,
    phase: f64,
    last_tick: Option<Instant>,
    value_out: f32,
    /// Sample-and-hold value for Random waveform.
    sh_value: f32,
    /// Pseudo-random state for Random waveform.
    rng_state: u64,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl LfoProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            rate_hz: 1.0,
            waveform: LfoWaveform::Sine,
            unipolar: false,
            phase_input: false,
            phase_in: 0.0,
            sync_in: 0.0,
            prev_sync: 0.0,
            phase: 0.0,
            last_tick: None,
            value_out: 0.0,
            sh_value: 0.0,
            rng_state: 0x1234_5678,
            inputs: Self::input_ports(false),
            outputs: Self::output_ports(false),
        }
    }

    fn input_ports(phase_input: bool) -> Vec<PortDef> {
        if phase_input {
            vec![PortDef::new("phase", PortType::Phase)]
        } else {
            vec![PortDef::new("trigger", PortType::Logic)]
        }
    }

    fn output_ports(phase_input: bool) -> Vec<PortDef> {
        let mut ports = vec![PortDef::new("value", PortType::Untyped)];
        if !phase_input {
            ports.push(PortDef::new("phase", PortType::Phase));
        }
        ports
    }

    fn set_phase_input(&mut self, on: bool) {
        if self.phase_input != on {
            self.phase_input = on;
            self.inputs = Self::input_ports(on);
            self.outputs = Self::output_ports(on);
            self.sync_in = 0.0;
            self.prev_sync = 0.0;
            self.phase_in = 0.0;
        }
    }

    fn next_random(&mut self) -> f32 {
        // xorshift64
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state = x;
        // Map to [-1, 1].
        ((x as u32) as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn waveform_value(&self) -> f32 {
        let p = self.phase as f32;
        match self.waveform {
            LfoWaveform::Sine => (p * std::f32::consts::TAU).sin(),
            LfoWaveform::Triangle => {
                if p < 0.5 { p * 4.0 - 1.0 } else { 3.0 - p * 4.0 }
            }
            LfoWaveform::Saw => p * 2.0 - 1.0,
            LfoWaveform::ReverseSaw => 1.0 - p * 2.0,
            LfoWaveform::Square => if p < 0.5 { 1.0 } else { -1.0 },
            LfoWaveform::Random => self.sh_value,
        }
    }
}

impl ProcessNode for LfoProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "LFO" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 {
            if self.phase_input { self.phase_in = v } else { self.sync_in = v }
        }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match (pi, self.phase_input) {
            (0, true) => self.phase_in,
            (0, false) => self.sync_in,
            _ => 0.0,
        }
    }

    fn process(&mut self) {
        if self.phase_input {
            // The wire is the clock: a new random value whenever it wraps.
            let phase = (self.phase_in as f64).rem_euclid(1.0);
            if phase < self.phase && matches!(self.waveform, LfoWaveform::Random) {
                self.sh_value = self.next_random();
            }
            self.phase = phase;
            let raw = self.waveform_value();
            self.value_out = if self.unipolar { raw * 0.5 + 0.5 } else { raw };
            return;
        }

        // Sync rising edge resets phase.
        if self.sync_in >= 0.5 && self.prev_sync < 0.5 {
            self.phase = 0.0;
            self.last_tick = Some(Instant::now());
            if matches!(self.waveform, LfoWaveform::Random) {
                self.sh_value = self.next_random();
            }
        }
        self.prev_sync = self.sync_in;

        let now = Instant::now();
        let dt = match self.last_tick {
            Some(prev) => now.duration_since(prev).as_secs_f64(),
            None => 0.0,
        };
        self.last_tick = Some(now);

        let prev_phase = self.phase;
        self.phase += dt * self.rate_hz as f64;

        // On phase wrap, generate a new sample-and-hold value for Random.
        if self.phase >= 1.0 {
            let cycles = self.phase.floor();
            self.phase -= cycles;
            if matches!(self.waveform, LfoWaveform::Random) {
                self.sh_value = self.next_random();
            }
        }
        let _ = prev_phase;

        let raw = self.waveform_value();
        self.value_out = if self.unipolar {
            raw * 0.5 + 0.5
        } else {
            raw
        };
    }

    fn read_output(&self, pi: usize) -> f32 {
        match pi {
            0 => self.value_out,
            1 => self.phase as f32,
            _ => 0.0,
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float {
                name: "Rate".into(),
                value: self.rate_hz,
                min: 0.01,
                max: 20.0,
                step: 0.01,
                unit: "Hz",
            },
            ParamDef::Choice {
                name: "Waveform".into(),
                value: self.waveform.to_index(),
                options: vec![
                    "Sine".into(),
                    "Triangle".into(),
                    "Saw".into(),
                    "Reverse Saw".into(),
                    "Square".into(),
                    "Random".into(),
                ],
            },
            ParamDef::Bool {
                name: "Unipolar (0..1)".into(),
                value: self.unipolar,
            },
            ParamDef::Bool {
                name: "Phase input".into(),
                value: self.phase_input,
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.rate_hz = value.as_f32().clamp(0.01, 20.0),
            1 => self.waveform = LfoWaveform::from_index(value.as_usize()),
            2 => self.unipolar = matches!(value, ParamValue::Bool(true)),
            3 => self.set_phase_input(matches!(value, ParamValue::Bool(true))),
            _ => {}
        }
    }

    fn non_promotable_params(&self) -> &'static [&'static str] {
        &["Phase input"]
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(LfoDisplay {
            rate_hz: self.rate_hz,
            phase: self.phase as f32,
            value: self.value_out,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lfo(waveform: usize, unipolar: bool) -> LfoProcessNode {
        let mut n = LfoProcessNode::new(NodeId(1));
        n.set_param(1, ParamValue::Int(waveform as i64));
        n.set_param(2, ParamValue::Bool(unipolar));
        n.set_param(3, ParamValue::Bool(true));
        n
    }

    fn at(n: &mut LfoProcessNode, phase: f32) -> f32 {
        n.write_input(0, phase);
        n.process();
        n.read_output(0)
    }

    #[test]
    fn the_phase_input_mode_swaps_sync_for_phase_and_drops_the_phase_output() {
        let mut n = LfoProcessNode::new(NodeId(1));
        assert_eq!((n.inputs().len(), n.outputs().len()), (1, 2));
        assert_eq!(n.inputs()[0].port_type, PortType::Logic);
        n.set_param(3, ParamValue::Bool(true));
        assert_eq!((n.inputs().len(), n.outputs().len()), (1, 1));
        assert_eq!(n.inputs()[0].port_type, PortType::Phase);
        assert_eq!(n.outputs()[0].name, "value");
        n.set_param(3, ParamValue::Bool(false));
        assert_eq!(n.outputs().len(), 2);
    }

    #[test]
    fn the_wire_sets_the_phase_of_the_waveform() {
        let mut saw = lfo(2, true);
        assert!((at(&mut saw, 0.0) - 0.0).abs() < 1e-6);
        assert!((at(&mut saw, 0.5) - 0.5).abs() < 1e-6);
        let mut sine = lfo(0, false);
        assert!((at(&mut sine, 0.25) - 1.0).abs() < 1e-5);
        assert!((at(&mut sine, 1.25) - 1.0).abs() < 1e-5, "it wraps");
        assert!((at(&mut sine, -0.75) - 1.0).abs() < 1e-5, "also below zero");
    }

    #[test]
    fn sync_is_ignored_in_phase_mode_and_random_changes_when_the_phase_wraps() {
        let mut n = lfo(5, false);
        n.write_input(0, 0.5);
        n.process();
        let held = n.read_output(0);
        for p in [0.6, 0.7, 0.9] {
            assert_eq!(at(&mut n, p), held, "held within a cycle");
        }
        assert_ne!(at(&mut n, 0.1), held, "a new value after the wrap");
    }
}
