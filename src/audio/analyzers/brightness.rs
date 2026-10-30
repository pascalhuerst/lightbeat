//! Where the energy sits between dark and bright: the spectral centroid on a
//! log scale from `min_hz` to `max_hz` mapped to 0..1. A bass line reads low,
//! open hats and risers read high, so it follows the "energy" of a section
//! without needing the beat.

use std::sync::Arc;

use parking_lot::RwLock;

use crate::audio::analyzers::fft::RollingFft;
use crate::audio::analyzers::{AnalyzerHandle, AnalyzerProc, handle_for};
use crate::engine::types::{ParamDef, ParamValue};

const FFT: usize = 2048;
/// Below this total amplitude the input counts as silence.
const SILENCE: f32 = 1e-3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrightnessParams {
    pub min_hz: f32,
    pub max_hz: f32,
    pub smoothing_ms: f32,
}

impl Default for BrightnessParams {
    fn default() -> Self {
        Self { min_hz: 100.0, max_hz: 8000.0, smoothing_ms: 250.0 }
    }
}

fn describe(p: &BrightnessParams) -> Vec<ParamDef> {
    vec![
        ParamDef::Float { name: "min_hz".into(), value: p.min_hz, min: 20.0, max: 2000.0, step: 10.0, unit: "Hz" },
        ParamDef::Float { name: "max_hz".into(), value: p.max_hz, min: 1000.0, max: 16000.0, step: 100.0, unit: "Hz" },
        ParamDef::Float { name: "smoothing_ms".into(), value: p.smoothing_ms, min: 0.0, max: 5000.0, step: 10.0, unit: "ms" },
    ]
}

fn apply(p: &mut BrightnessParams, index: usize, value: ParamValue) {
    let v = value.as_f32();
    match index {
        0 => p.min_hz = v.max(20.0),
        1 => p.max_hz = v.max(p.min_hz * 1.5),
        2 => p.smoothing_ms = v.max(0.0),
        _ => {}
    }
}

pub struct BrightnessProc {
    params: Arc<RwLock<BrightnessParams>>,
    sample_rate: u32,
    fft: RollingFft,
    level: f32,
}

impl AnalyzerProc for BrightnessProc {
    fn num_outputs(&self) -> usize { 1 }

    fn step(&mut self, samples: &[f32]) {
        self.fft.push(samples);
        let p = *self.params.read();
        let bin_hz = self.fft.bin_hz();
        let (mut weighted, mut total) = (0.0f32, 0.0f32);
        for (k, &a) in self.fft.analyse().iter().enumerate().skip(1) {
            let f = k as f32 * bin_hz;
            if f >= 50.0 && f <= 16_000.0 {
                weighted += f * a;
                total += a;
            }
        }
        let target = if total < SILENCE {
            0.0
        } else {
            let centroid = weighted / total;
            ((centroid / p.min_hz).ln() / (p.max_hz / p.min_hz).ln()).clamp(0.0, 1.0)
        };
        let chunk_ms = samples.len() as f32 / self.sample_rate.max(1) as f32 * 1000.0;
        let k = if p.smoothing_ms <= 0.0 { 0.0 } else { (-chunk_ms / p.smoothing_ms).exp() };
        self.level = target + (self.level - target) * k;
    }

    fn outputs(&self) -> Vec<f32> { vec![self.level] }
}

pub fn create(sample_rate: u32) -> (AnalyzerHandle, Box<dyn AnalyzerProc>) {
    let params = Arc::new(RwLock::new(BrightnessParams::default()));
    let handle = handle_for(params.clone(), describe, apply);
    let proc = BrightnessProc { params, sample_rate, fft: RollingFft::new(FFT, sample_rate), level: 0.0 };
    (handle, Box::new(proc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brightness_of(freq: f32) -> f32 {
        let (_h, mut p) = create(44_100);
        let audio: Vec<f32> = (0..44_100)
            .map(|i| 0.5 * (std::f32::consts::TAU * freq * i as f32 / 44_100.0).sin())
            .collect();
        for chunk in audio.chunks(512) {
            p.step(chunk);
        }
        p.outputs()[0]
    }

    #[test]
    fn higher_sounds_read_brighter_and_silence_reads_zero() {
        let (low, mid, high) = (brightness_of(150.0), brightness_of(1000.0), brightness_of(6000.0));
        assert!(low < 0.2 && low < mid && mid < high && high > 0.8, "{low} {mid} {high}");
        let (_h, mut p) = create(44_100);
        p.step(&[0.0; 4096]);
        assert_eq!(p.outputs()[0], 0.0);
    }
}
