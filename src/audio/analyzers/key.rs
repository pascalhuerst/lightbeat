//! Musical key. Energy is folded into the twelve pitch classes over the last
//! several seconds and compared with the Krumhansl-Schmuckler major and minor
//! profiles. `key` is the tonic (0 = C ... 11 = B), `major` says which mode,
//! and `confidence` how clearly the best key beats the rest.
//!
//! Tuned to hold a key through a track and follow a change over some seconds,
//! not to catch every modulation; percussion-only passages give no key.

use std::sync::Arc;

use parking_lot::RwLock;

use crate::audio::analyzers::fft::RollingFft;
use crate::audio::analyzers::{AnalyzerHandle, AnalyzerProc, handle_for};
use crate::engine::types::{ParamDef, ParamValue};

const FFT: usize = 8192;
/// New samples between two looks at the spectrum.
const HOP: usize = 4096;
const MIN_HZ: f32 = 55.0;
const MAX_HZ: f32 = 2000.0;
/// Chroma total below which there is nothing to judge.
const SILENCE: f32 = 1e-3;
/// A key must beat the current one by this much of a correlation to take over.
const SWITCH_MARGIN: f32 = 0.03;

const MAJOR: [f32; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
const MINOR: [f32; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyParams {
    /// How long the chroma remembers, in seconds.
    pub memory_s: f32,
}

impl Default for KeyParams {
    fn default() -> Self {
        Self { memory_s: 8.0 }
    }
}

fn describe(p: &KeyParams) -> Vec<ParamDef> {
    vec![ParamDef::Float { name: "memory_s".into(), value: p.memory_s, min: 1.0, max: 60.0, step: 0.5, unit: "s" }]
}

fn apply(p: &mut KeyParams, index: usize, value: ParamValue) {
    if index == 0 {
        p.memory_s = value.as_f32().max(0.5);
    }
}

/// Pearson correlation of `a` with `b` rotated so that `b[0]` lands on `tonic`.
fn correlate(chroma: &[f32; 12], profile: &[f32; 12], tonic: usize) -> f32 {
    let mean = |v: &[f32; 12]| v.iter().sum::<f32>() / 12.0;
    let (mc, mp) = (mean(chroma), mean(profile));
    let (mut num, mut dc, mut dp) = (0.0, 0.0, 0.0);
    for pc in 0..12 {
        let c = chroma[pc] - mc;
        let p = profile[(pc + 12 - tonic) % 12] - mp;
        num += c * p;
        dc += c * c;
        dp += p * p;
    }
    if dc <= 0.0 || dp <= 0.0 { 0.0 } else { num / (dc * dp).sqrt() }
}

/// Best (tonic, major, score) and the best score of any other key.
fn best_key(chroma: &[f32; 12]) -> (usize, bool, f32, f32) {
    let mut scores: Vec<(usize, bool, f32)> = Vec::with_capacity(24);
    for tonic in 0..12 {
        scores.push((tonic, true, correlate(chroma, &MAJOR, tonic)));
        scores.push((tonic, false, correlate(chroma, &MINOR, tonic)));
    }
    scores.sort_by(|a, b| b.2.total_cmp(&a.2));
    (scores[0].0, scores[0].1, scores[0].2, scores[1].2)
}

pub struct KeyProc {
    params: Arc<RwLock<KeyParams>>,
    sample_rate: u32,
    fft: RollingFft,
    since_look: usize,
    chroma: [f32; 12],
    key: usize,
    major: bool,
    score: f32,
    confidence: f32,
}

impl AnalyzerProc for KeyProc {
    fn num_outputs(&self) -> usize { 3 }

    fn step(&mut self, samples: &[f32]) {
        self.fft.push(samples);
        self.since_look += samples.len();
        if self.since_look < HOP {
            return;
        }
        let dt = self.since_look as f32 / self.sample_rate.max(1) as f32;
        self.since_look = 0;

        let bin_hz = self.fft.bin_hz();
        let mut frame = [0.0f32; 12];
        for (k, &a) in self.fft.analyse().iter().enumerate().skip(1) {
            let f = k as f32 * bin_hz;
            if (MIN_HZ..=MAX_HZ).contains(&f) {
                let midi = 69.0 + 12.0 * (f / 440.0).log2();
                frame[(midi.round() as i32).rem_euclid(12) as usize] += a;
            }
        }
        let keep = (-dt / self.params.read().memory_s).exp();
        for (c, f) in self.chroma.iter_mut().zip(frame) {
            *c = *c * keep + f * (1.0 - keep);
        }
        if self.chroma.iter().sum::<f32>() < SILENCE {
            self.confidence = 0.0;
            return;
        }

        let (tonic, major, score, second) = best_key(&self.chroma);
        let current = correlate(&self.chroma, if self.major { &MAJOR } else { &MINOR }, self.key);
        if self.score == 0.0 || score > current + SWITCH_MARGIN {
            self.key = tonic;
            self.major = major;
        }
        self.score = score;
        self.confidence = ((score - second) / 0.1).clamp(0.0, 1.0);
    }

    fn outputs(&self) -> Vec<f32> {
        vec![self.key as f32, f32::from(u8::from(self.major)), self.confidence]
    }
}

pub fn create(sample_rate: u32) -> (AnalyzerHandle, Box<dyn AnalyzerProc>) {
    let params = Arc::new(RwLock::new(KeyParams::default()));
    let handle = handle_for(params.clone(), describe, apply);
    let proc = KeyProc {
        params,
        sample_rate,
        fft: RollingFft::new(FFT, sample_rate),
        since_look: 0,
        chroma: [0.0; 12],
        key: 0,
        major: true,
        score: 0.0,
        confidence: 0.0,
    };
    (handle, Box::new(proc))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    /// A chord of notes (MIDI) with a few harmonics each, like a pad.
    fn chord(notes: &[f32], seconds: f32) -> Vec<f32> {
        (0..(RATE as f32 * seconds) as usize)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                notes
                    .iter()
                    .map(|&n| {
                        let f = 440.0 * 2f32.powf((n - 69.0) / 12.0);
                        (1..=4).map(|h| (std::f32::consts::TAU * f * h as f32 * t).sin() / (h * h) as f32).sum::<f32>()
                    })
                    .sum::<f32>()
                    * 0.1
            })
            .collect()
    }

    fn key_of(progression: &[&[f32]]) -> Vec<f32> {
        let (_h, mut p) = create(RATE);
        for _ in 0..4 {
            for notes in progression {
                for chunk in chord(notes, 2.0).chunks(512) {
                    p.step(chunk);
                }
            }
        }
        p.outputs()
    }

    #[test]
    fn a_minor_progression_is_found_in_a_minor() {
        // Am - Dm - E - Am
        let out = key_of(&[&[57.0, 60.0, 64.0], &[62.0, 65.0, 69.0], &[52.0, 56.0, 59.0], &[57.0, 60.0, 64.0]]);
        assert_eq!((out[0], out[1]), (9.0, 0.0), "{out:?}");
        assert!(out[2] > 0.3);
    }

    #[test]
    fn a_major_progression_is_found_in_its_major_key() {
        // C - Am - F - G
        let out = key_of(&[&[48.0, 52.0, 55.0], &[57.0, 60.0, 64.0], &[53.0, 57.0, 60.0], &[55.0, 59.0, 62.0]]);
        assert_eq!((out[0], out[1]), (0.0, 1.0), "{out:?}");
        // Transposed up a whole tone: D major.
        let out = key_of(&[&[50.0, 54.0, 57.0], &[59.0, 62.0, 66.0], &[55.0, 59.0, 62.0], &[57.0, 61.0, 64.0]]);
        assert_eq!((out[0], out[1]), (2.0, 1.0), "{out:?}");
    }

    #[test]
    fn silence_has_no_key() {
        let (_h, mut p) = create(RATE);
        for _ in 0..40 {
            p.step(&[0.0; 512]);
        }
        assert_eq!(p.outputs()[2], 0.0);
    }
}
