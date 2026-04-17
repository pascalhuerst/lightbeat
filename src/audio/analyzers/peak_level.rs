//! Peak-level meter analyzer. Outputs the smoothed peak amplitude (0..1) of
//! incoming audio chunks. Peak attack is instant; release is an exponential
//! decay controlled by `release_ms`.

use std::sync::Arc;
use std::thread;

use crossbeam_channel::Receiver;
use parking_lot::{Mutex, RwLock};

use crate::audio::device::AudioChunk;
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
    vec![
        ParamDef::Float {
            name: "release_ms".into(),
            value: d.release_ms, min: 1.0, max: 2000.0, step: 1.0, unit: "ms",
        },
    ]
}

pub type SharedPeakParams = Arc<RwLock<PeakLevelParams>>;

pub struct PeakLevelOutputs {
    /// Latest smoothed peak in 0..1.
    pub level: Mutex<f32>,
    /// Latest smoothed RMS in 0..1.
    pub rms: Mutex<f32>,
}

pub struct PeakLevelAnalyzer {
    pub params: SharedPeakParams,
    pub outputs: Arc<PeakLevelOutputs>,
    _join: Option<thread::JoinHandle<()>>,
}

impl PeakLevelAnalyzer {
    pub fn spawn(rx: Receiver<AudioChunk>, sample_rate: u32) -> Self {
        let params = Arc::new(RwLock::new(PeakLevelParams::default()));
        let outputs = Arc::new(PeakLevelOutputs {
            level: Mutex::new(0.0),
            rms: Mutex::new(0.0),
        });
        let p = params.clone();
        let o = outputs.clone();
        let join = thread::Builder::new()
            .name("lightbeat-peak-analyzer".into())
            .spawn(move || run(rx, sample_rate, p, o))
            .expect("spawn peak analyzer thread");
        Self { params, outputs, _join: Some(join) }
    }

    pub fn read_outputs(&self) -> Vec<f32> {
        vec![*self.outputs.level.lock(), *self.outputs.rms.lock()]
    }

    pub fn current_params(&self) -> Vec<ParamDef> {
        let p = *self.params.read();
        vec![ParamDef::Float {
            name: "release_ms".into(),
            value: p.release_ms, min: 1.0, max: 2000.0, step: 1.0, unit: "ms",
        }]
    }

    pub fn set_param(&self, index: usize, value: ParamValue) {
        let mut p = self.params.write();
        if index == 0 { p.release_ms = value.as_f32().max(1.0); }
    }
}

fn run(rx: Receiver<AudioChunk>, sample_rate: u32, params: SharedPeakParams, outputs: Arc<PeakLevelOutputs>) {
    let mut smoothed_peak: f32 = 0.0;
    let mut smoothed_rms: f32 = 0.0;
    while let Ok(chunk) = rx.recv() {
        let n = chunk.mono_f32.len().max(1);
        let mut chunk_peak = 0.0_f32;
        let mut sq_sum = 0.0_f32;
        for &s in chunk.mono_f32.iter() {
            let a = s.abs();
            if a > chunk_peak { chunk_peak = a; }
            sq_sum += s * s;
        }
        let chunk_rms = (sq_sum / n as f32).sqrt();

        let release_ms = params.read().release_ms.max(1.0);
        let chunk_dur_s = n as f32 / sample_rate.max(1) as f32;
        let alpha = (-chunk_dur_s * 1000.0 / release_ms).exp();

        // Peak: instant attack, exponential release.
        smoothed_peak = (smoothed_peak * alpha).max(chunk_peak);
        // RMS: full one-pole IIR (slow attack and release).
        smoothed_rms = smoothed_rms * alpha + chunk_rms * (1.0 - alpha);

        *outputs.level.lock() = smoothed_peak.min(2.0);
        *outputs.rms.lock() = smoothed_rms.min(2.0);
    }
}
