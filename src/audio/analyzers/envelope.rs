//! Envelope follower. Tracks the amplitude envelope of incoming audio with
//! separate attack and release time constants — useful as a control signal
//! for things like "open this fader when the kick hits" or "ducking".

use std::sync::Arc;

use parking_lot::RwLock;

use crate::audio::analyzers::{AnalyzerHandle, AnalyzerKind, AnalyzerProc};
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
    params_for(&d)
}

fn params_for(p: &EnvelopeParams) -> Vec<ParamDef> {
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

pub type SharedEnvelopeParams = Arc<RwLock<EnvelopeParams>>;

fn apply_param(params: &SharedEnvelopeParams, index: usize, value: ParamValue) {
    let mut p = params.write();
    match index {
        0 => p.attack_ms = value.as_f32().max(0.1),
        1 => p.release_ms = value.as_f32().max(0.1),
        _ => {}
    }
}

pub struct EnvelopeProc {
    params: SharedEnvelopeParams,
    sample_rate: u32,
    env: f32,
    out: f32,
}

impl AnalyzerProc for EnvelopeProc {
    fn kind(&self) -> AnalyzerKind { AnalyzerKind::Envelope }
    fn num_outputs(&self) -> usize { 1 }
    fn output_latency_samples(&self) -> u32 { 0 }

    fn step(&mut self, samples: &[f32]) {
        // Rectified peak of this chunk drives the follower.
        let chunk_peak = samples.iter().copied().fold(0.0_f32, |acc, s| acc.max(s.abs()));
        let p = *self.params.read();
        let n = samples.len().max(1);
        let chunk_dur_s = n as f32 / self.sample_rate.max(1) as f32;
        let alpha = if chunk_peak > self.env {
            (-chunk_dur_s * 1000.0 / p.attack_ms.max(0.1)).exp()
        } else {
            (-chunk_dur_s * 1000.0 / p.release_ms.max(0.1)).exp()
        };
        self.env = self.env * alpha + chunk_peak * (1.0 - alpha);
        self.out = self.env.min(2.0);
    }

    fn outputs(&self) -> Vec<f32> { vec![self.out] }
}

/// Build `(handle, proc)` for this analyzer.
pub fn create(sample_rate: u32) -> (AnalyzerHandle, Box<dyn AnalyzerProc>) {
    let params = Arc::new(RwLock::new(EnvelopeParams::default()));

    let p_get = params.clone();
    let get_params = Arc::new(move || params_for(&*p_get.read()))
        as Arc<dyn Fn() -> Vec<ParamDef> + Send + Sync>;

    let p_set = params.clone();
    let set_param = Arc::new(move |idx: usize, v: ParamValue| apply_param(&p_set, idx, v))
        as Arc<dyn Fn(usize, ParamValue) + Send + Sync>;

    let proc = Box::new(EnvelopeProc {
        params,
        sample_rate,
        env: 0.0,
        out: 0.0,
    });
    (AnalyzerHandle::new(AnalyzerKind::Envelope, get_params, set_param), proc)
}
