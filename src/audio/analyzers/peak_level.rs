//! Peak-level meter analyzer. Outputs the smoothed peak amplitude (0..1) of
//! incoming audio chunks. Peak attack is instant; release is an exponential
//! decay controlled by `release_ms`.

use std::sync::Arc;

use parking_lot::RwLock;

use crate::audio::analyzers::{AnalyzerHandle, AnalyzerKind, AnalyzerProc};
use crate::engine::types::{ParamDef, ParamValue};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeakLevelParams {
    /// Exponential release time constant in milliseconds. Larger = slower
    /// decay (more visual stability); smaller = snappier.
    pub release_ms: f32,
}

impl Default for PeakLevelParams {
    fn default() -> Self { Self { release_ms: 200.0 } }
}

pub fn peak_params() -> Vec<ParamDef> {
    let d = PeakLevelParams::default();
    params_for(&d)
}

fn params_for(p: &PeakLevelParams) -> Vec<ParamDef> {
    vec![ParamDef::Float {
        name: "release_ms".into(),
        value: p.release_ms, min: 1.0, max: 2000.0, step: 1.0, unit: "ms",
    }]
}

pub type SharedPeakParams = Arc<RwLock<PeakLevelParams>>;

fn apply_param(params: &SharedPeakParams, index: usize, value: ParamValue) {
    if index == 0 {
        params.write().release_ms = value.as_f32().max(1.0);
    }
}

pub struct PeakLevelProc {
    params: SharedPeakParams,
    sample_rate: u32,
    peak: f32,
    rms: f32,
}

impl AnalyzerProc for PeakLevelProc {
    fn kind(&self) -> AnalyzerKind { AnalyzerKind::PeakLevel }
    fn num_outputs(&self) -> usize { 2 }
    fn output_latency_samples(&self) -> u32 { 0 }

    fn step(&mut self, samples: &[f32]) {
        let n = samples.len().max(1);
        let mut chunk_peak = 0.0f32;
        let mut sq_sum = 0.0f32;
        for &s in samples {
            let a = s.abs();
            if a > chunk_peak { chunk_peak = a; }
            sq_sum += s * s;
        }
        let chunk_rms = (sq_sum / n as f32).sqrt();

        let release_ms = self.params.read().release_ms.max(1.0);
        let chunk_dur_s = n as f32 / self.sample_rate.max(1) as f32;
        let alpha = (-chunk_dur_s * 1000.0 / release_ms).exp();

        // Peak: instant attack, exponential release.
        self.peak = (self.peak * alpha).max(chunk_peak);
        // RMS: full one-pole IIR (slow attack and release).
        self.rms = self.rms * alpha + chunk_rms * (1.0 - alpha);
    }

    fn outputs(&self) -> Vec<f32> { vec![self.peak.min(2.0), self.rms.min(2.0)] }
}

pub fn create(sample_rate: u32) -> (AnalyzerHandle, Box<dyn AnalyzerProc>) {
    let params = Arc::new(RwLock::new(PeakLevelParams::default()));

    let p_get = params.clone();
    let get_params = Arc::new(move || params_for(&*p_get.read()))
        as Arc<dyn Fn() -> Vec<ParamDef> + Send + Sync>;

    let p_set = params.clone();
    let set_param = Arc::new(move |idx: usize, v: ParamValue| apply_param(&p_set, idx, v))
        as Arc<dyn Fn(usize, ParamValue) + Send + Sync>;

    let proc = Box::new(PeakLevelProc { params, sample_rate, peak: 0.0, rms: 0.0 });
    (AnalyzerHandle::new(AnalyzerKind::PeakLevel, get_params, set_param), proc)
}
