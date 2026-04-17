//! Envelope follower. Tracks the amplitude envelope of incoming audio with
//! separate attack and release time constants — useful as a control signal
//! for things like "open this fader when the kick hits" or "ducking".

use std::sync::Arc;
use std::thread;

use crossbeam_channel::Receiver;
use parking_lot::{Mutex, RwLock};

use crate::audio::device::AudioChunk;
use crate::engine::types::{ParamDef, ParamValue};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvelopeParams {
    /// Time constant (ms) for rising values.
    pub attack_ms: f32,
    /// Time constant (ms) for falling values.
    pub release_ms: f32,
}

impl Default for EnvelopeParams {
    fn default() -> Self { Self { attack_ms: 5.0, release_ms: 100.0 } }
}

pub fn envelope_params() -> Vec<ParamDef> {
    let d = EnvelopeParams::default();
    vec![
        ParamDef::Float {
            name: "attack_ms".into(),
            value: d.attack_ms, min: 0.1, max: 1000.0, step: 0.5, unit: "ms",
        },
        ParamDef::Float {
            name: "release_ms".into(),
            value: d.release_ms, min: 0.1, max: 5000.0, step: 1.0, unit: "ms",
        },
    ]
}

pub type SharedEnvelopeParams = Arc<RwLock<EnvelopeParams>>;

pub struct EnvelopeOutputs {
    pub envelope: Mutex<f32>,
}

pub struct EnvelopeAnalyzer {
    pub params: SharedEnvelopeParams,
    pub outputs: Arc<EnvelopeOutputs>,
    _join: Option<thread::JoinHandle<()>>,
}

impl EnvelopeAnalyzer {
    pub fn spawn(rx: Receiver<AudioChunk>, sample_rate: u32) -> Self {
        let params = Arc::new(RwLock::new(EnvelopeParams::default()));
        let outputs = Arc::new(EnvelopeOutputs { envelope: Mutex::new(0.0) });
        let p = params.clone();
        let o = outputs.clone();
        let join = thread::Builder::new()
            .name("lightbeat-envelope-analyzer".into())
            .spawn(move || run(rx, sample_rate, p, o))
            .expect("spawn envelope analyzer thread");
        Self { params, outputs, _join: Some(join) }
    }

    pub fn read_outputs(&self) -> Vec<f32> {
        vec![*self.outputs.envelope.lock()]
    }

    pub fn current_params(&self) -> Vec<ParamDef> {
        let p = *self.params.read();
        vec![
            ParamDef::Float {
                name: "attack_ms".into(),
                value: p.attack_ms, min: 0.1, max: 1000.0, step: 0.5, unit: "ms",
            },
            ParamDef::Float {
                name: "release_ms".into(),
                value: p.release_ms, min: 0.1, max: 5000.0, step: 1.0, unit: "ms",
            },
        ]
    }

    pub fn set_param(&self, index: usize, value: ParamValue) {
        let mut p = self.params.write();
        match index {
            0 => p.attack_ms = value.as_f32().max(0.1),
            1 => p.release_ms = value.as_f32().max(0.1),
            _ => {}
        }
    }
}

fn run(rx: Receiver<AudioChunk>, sample_rate: u32, params: SharedEnvelopeParams, outputs: Arc<EnvelopeOutputs>) {
    let mut env: f32 = 0.0;
    while let Ok(chunk) = rx.recv() {
        // Rectified peak of this chunk — drives the follower.
        let chunk_peak = chunk.mono_f32.iter()
            .copied()
            .fold(0.0_f32, |acc, s| acc.max(s.abs()));

        let p = *params.read();
        let chunk_dur_s = chunk.mono_f32.len() as f32 / sample_rate.max(1) as f32;
        // Asymmetric one-pole IIR: pick the attack or release coefficient
        // depending on whether the input is above or below the current env.
        let alpha = if chunk_peak > env {
            (-chunk_dur_s * 1000.0 / p.attack_ms.max(0.1)).exp()
        } else {
            (-chunk_dur_s * 1000.0 / p.release_ms.max(0.1)).exp()
        };
        env = env * alpha + chunk_peak * (1.0 - alpha);
        *outputs.envelope.lock() = env.min(2.0);
    }
}
