//! Spectrum analyzer: the level in eight octave bands, 63 Hz to 8 kHz, each
//! scaled to 0..1 so it can drive a fader-like signal directly (a kick on the
//! lowest band, hats on the highest).
//!
//! Each chunk the last `FFT` samples are windowed and transformed; a band is
//! the energy of the bins inside its octave. A full-scale sine reads 0 dB.
//! Levels are mapped from `floor_db` .. 0 dB to 0..1 (after `gain_db`) and
//! smoothed with an attack and a release time.

use std::sync::Arc;

use parking_lot::RwLock;
use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

use crate::audio::analyzers::{AnalyzerHandle, AnalyzerProc};
use crate::engine::types::{ParamDef, ParamValue};

pub const FFT: usize = 2048;
/// Octave band centres in Hz. Saved wires follow this order.
pub const BAND_CENTRES: [f32; 8] = [63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpectrumParams {
    /// Level that reads as 0; anything quieter is 0.
    pub floor_db: f32,
    /// Added before scaling, to bring a quiet source up.
    pub gain_db: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
}

impl Default for SpectrumParams {
    fn default() -> Self {
        Self { floor_db: -60.0, gain_db: 0.0, attack_ms: 10.0, release_ms: 150.0 }
    }
}

fn params_for(p: &SpectrumParams) -> Vec<ParamDef> {
    vec![
        ParamDef::Float { name: "floor_db".into(), value: p.floor_db, min: -100.0, max: -20.0, step: 1.0, unit: "dB" },
        ParamDef::Float { name: "gain_db".into(), value: p.gain_db, min: -20.0, max: 40.0, step: 0.5, unit: "dB" },
        ParamDef::Float { name: "attack_ms".into(), value: p.attack_ms, min: 0.0, max: 500.0, step: 1.0, unit: "ms" },
        ParamDef::Float { name: "release_ms".into(), value: p.release_ms, min: 1.0, max: 2000.0, step: 1.0, unit: "ms" },
    ]
}

type SharedParams = Arc<RwLock<SpectrumParams>>;

fn apply_param(params: &SharedParams, index: usize, value: ParamValue) {
    let mut p = params.write();
    let v = value.as_f32();
    match index {
        0 => p.floor_db = v.min(-1.0),
        1 => p.gain_db = v,
        2 => p.attack_ms = v.max(0.0),
        3 => p.release_ms = v.max(1.0),
        _ => {}
    }
}

/// The bins `lo..=hi` an octave band around `centre` covers, at least one.
fn band_bins(centre: f32, sample_rate: u32) -> (usize, usize) {
    let bin_hz = sample_rate as f32 / FFT as f32;
    let top = FFT / 2;
    let lo = ((centre / std::f32::consts::SQRT_2 / bin_hz).round() as usize).clamp(1, top);
    let hi = ((centre * std::f32::consts::SQRT_2 / bin_hz).round() as usize).clamp(lo, top);
    (lo, hi)
}

pub struct SpectrumProc {
    params: SharedParams,
    sample_rate: u32,
    plan: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    ring: Vec<f32>,
    ring_pos: usize,
    input: Vec<f32>,
    spectrum: Vec<Complex32>,
    scratch: Vec<Complex32>,
    bands: Vec<(usize, usize)>,
    levels: [f32; 8],
}

impl SpectrumProc {
    fn new(params: SharedParams, sample_rate: u32) -> Self {
        let plan = RealFftPlanner::<f32>::new().plan_fft_forward(FFT);
        Self {
            params,
            sample_rate,
            window: (0..FFT)
                .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / FFT as f32).cos())
                .collect(),
            ring: vec![0.0; FFT],
            ring_pos: 0,
            input: plan.make_input_vec(),
            spectrum: plan.make_output_vec(),
            scratch: plan.make_scratch_vec(),
            plan,
            bands: BAND_CENTRES.iter().map(|&c| band_bins(c, sample_rate)).collect(),
            levels: [0.0; 8],
        }
    }

    /// Band amplitudes (1.0 = full-scale sine) of the last `FFT` samples.
    fn amplitudes(&mut self) -> [f32; 8] {
        for i in 0..FFT {
            self.input[i] = self.ring[(self.ring_pos + i) % FFT] * self.window[i];
        }
        let _ = self.plan.process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch);
        // A Hann-windowed sine of amplitude 1 peaks at FFT/4 and spreads its
        // energy over three bins, 1.5 times the peak bin's.
        let norm = FFT as f32 / 4.0 * 1.5f32.sqrt();
        let mut out = [0.0; 8];
        for (o, &(lo, hi)) in out.iter_mut().zip(&self.bands) {
            let energy: f32 = self.spectrum[lo..=hi].iter().map(|c| c.norm_sqr()).sum();
            *o = energy.sqrt() / norm;
        }
        out
    }
}

impl AnalyzerProc for SpectrumProc {
    fn num_outputs(&self) -> usize { BAND_CENTRES.len() }

    fn step(&mut self, samples: &[f32]) {
        for &s in samples.iter().rev().take(FFT).rev() {
            self.ring[self.ring_pos] = s;
            self.ring_pos = (self.ring_pos + 1) % FFT;
        }
        let p = *self.params.read();
        let chunk_ms = samples.len() as f32 / self.sample_rate.max(1) as f32 * 1000.0;
        let coeff = |ms: f32| if ms <= 0.0 { 0.0 } else { (-chunk_ms / ms).exp() };
        let (attack, release) = (coeff(p.attack_ms), coeff(p.release_ms));
        let amps = self.amplitudes();
        for (level, amp) in self.levels.iter_mut().zip(amps) {
            let db = 20.0 * amp.max(1e-6).log10() + p.gain_db;
            let target = ((db - p.floor_db) / -p.floor_db).clamp(0.0, 1.0);
            let k = if target > *level { attack } else { release };
            *level = target + (*level - target) * k;
        }
    }

    fn outputs(&self) -> Vec<f32> { self.levels.to_vec() }
}

pub fn create(sample_rate: u32) -> (AnalyzerHandle, Box<dyn AnalyzerProc>) {
    let params = Arc::new(RwLock::new(SpectrumParams::default()));

    let p_get = params.clone();
    let get_params = Arc::new(move || params_for(&*p_get.read()))
        as Arc<dyn Fn() -> Vec<ParamDef> + Send + Sync>;

    let p_set = params.clone();
    let set_param = Arc::new(move |idx: usize, v: ParamValue| apply_param(&p_set, idx, v))
        as Arc<dyn Fn(usize, ParamValue) + Send + Sync>;

    (AnalyzerHandle::new(get_params, set_param), Box::new(SpectrumProc::new(params, sample_rate)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    fn sine(freq: f32, n: usize, amp: f32) -> Vec<f32> {
        (0..n).map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / RATE as f32).sin()).collect()
    }

    fn run(freq: f32, amp: f32) -> Vec<f32> {
        let mut proc = SpectrumProc::new(Arc::new(RwLock::new(SpectrumParams::default())), RATE);
        let audio = sine(freq, RATE as usize / 2, amp);
        for chunk in audio.chunks(512) {
            proc.step(chunk);
        }
        proc.outputs()
    }

    #[test]
    fn a_tone_lights_its_own_octave_band_and_little_else() {
        for (band, &centre) in BAND_CENTRES.iter().enumerate() {
            let out = run(centre, 0.5);
            let loudest = (0..8).max_by(|&a, &b| out[a].total_cmp(&out[b])).unwrap();
            assert_eq!(loudest, band, "{centre} Hz lit {out:?}");
            // Half scale is -6 dB: 54/60 of the way up the default range.
            assert!((out[band] - 0.9).abs() < 0.08, "{centre} Hz read {}", out[band]);
            for (other, &v) in out.iter().enumerate() {
                if other.abs_diff(band) >= 2 {
                    assert!(v < out[band] * 0.5, "{centre} Hz leaked {v} into band {other}");
                }
            }
        }
    }

    #[test]
    fn silence_reads_zero_and_levels_fall_after_a_tone() {
        assert!(run(1000.0, 0.0).iter().all(|&v| v == 0.0));
        let mut proc = SpectrumProc::new(Arc::new(RwLock::new(SpectrumParams::default())), RATE);
        for chunk in sine(1000.0, 8192, 0.5).chunks(512) {
            proc.step(chunk);
        }
        let loud = proc.outputs()[4];
        for _ in 0..40 {
            proc.step(&[0.0; 512]);
        }
        assert!(proc.outputs()[4] < loud * 0.2);
    }
}
