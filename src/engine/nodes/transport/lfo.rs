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
    pub waveform: LfoWaveform,
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
            sync_in: 0.0,
            prev_sync: 0.0,
            phase: 0.0,
            last_tick: None,
            value_out: 0.0,
            sh_value: 0.0,
            rng_state: 0x1234_5678,
            inputs: vec![PortDef::new("sync", PortType::Logic)],
            outputs: vec![
                PortDef::new("value", PortType::Untyped),
                PortDef::new("phase", PortType::Phase),
            ],
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
        if pi == 0 { self.sync_in = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.sync_in } else { 0.0 }
    }

    fn process(&mut self) {
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
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.rate_hz = value.as_f32().clamp(0.01, 20.0),
            1 => self.waveform = LfoWaveform::from_index(value.as_usize()),
            2 => self.unipolar = matches!(value, ParamValue::Bool(true)),
            _ => {}
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(LfoDisplay {
            waveform: self.waveform,
            rate_hz: self.rate_hz,
            phase: self.phase as f32,
            value: self.value_out,
        }));
    }
}
