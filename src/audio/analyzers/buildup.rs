//! Breakdowns, build-ups and drops, from the bass alone: a breakdown is
//! when the bass has gone away for a while, a drop is when it comes back
//! after a real breakdown, and tension is how far the highs have risen since
//! the breakdown began (a riser or a snare roll).

use std::sync::Arc;

use parking_lot::RwLock;

use crate::audio::analyzers::fft::RollingFft;
use crate::audio::analyzers::{AnalyzerHandle, AnalyzerProc, handle_for};
use crate::engine::types::{ParamDef, ParamValue};

const FFT: usize = 2048;
const BASS_HZ: (f32, f32) = (30.0, 150.0);
const HIGH_HZ: (f32, f32) = (500.0, 12_000.0);
/// Smoothing of the band levels.
const LEVEL_MS: f32 = 120.0;
/// How slowly the bass reference falls when the bass goes away.
const REFERENCE_DECAY_S: f32 = 20.0;
/// Bass reference below which there is no music to speak of.
const MIN_REFERENCE: f32 = 0.004;
/// Rise of the highs, as a ratio, that counts as full tension.
const FULL_TENSION_RATIO: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildupParams {
    /// Bass below this share of its usual level counts as gone.
    pub bass_ratio: f32,
    /// How long the bass must be gone to count as a breakdown.
    pub gone_s: f32,
    /// A breakdown shorter than this does not end in a drop.
    pub min_breakdown_s: f32,
}

impl Default for BuildupParams {
    fn default() -> Self {
        Self { bass_ratio: 0.3, gone_s: 0.75, min_breakdown_s: 2.0 }
    }
}

fn describe(p: &BuildupParams) -> Vec<ParamDef> {
    vec![
        ParamDef::Float { name: "bass_ratio".into(), value: p.bass_ratio, min: 0.05, max: 0.9, step: 0.05, unit: "" },
        ParamDef::Float { name: "gone_s".into(), value: p.gone_s, min: 0.1, max: 8.0, step: 0.05, unit: "s" },
        ParamDef::Float { name: "min_breakdown_s".into(), value: p.min_breakdown_s, min: 0.5, max: 30.0, step: 0.5, unit: "s" },
    ]
}

fn apply(p: &mut BuildupParams, index: usize, value: ParamValue) {
    let v = value.as_f32();
    match index {
        0 => p.bass_ratio = v.clamp(0.05, 0.95),
        1 => p.gone_s = v.max(0.05),
        2 => p.min_breakdown_s = v.max(0.1),
        _ => {}
    }
}

pub struct BuildupProc {
    params: Arc<RwLock<BuildupParams>>,
    sample_rate: u32,
    fft: RollingFft,
    bass: f32,
    high: f32,
    reference: f32,
    gone_for: f32,
    breakdown: bool,
    breakdown_for: f32,
    high_at_start: f32,
    tension: f32,
    drops: u64,
}

impl BuildupProc {
    fn band(amps: &[f32], bin_hz: f32, (lo, hi): (f32, f32)) -> f32 {
        let (a, b) = ((lo / bin_hz) as usize, ((hi / bin_hz) as usize).min(amps.len() - 1));
        (amps[a.max(1)..=b].iter().map(|x| x * x).sum::<f32>()).sqrt()
    }
}

impl AnalyzerProc for BuildupProc {
    fn num_outputs(&self) -> usize { 3 }

    fn step(&mut self, samples: &[f32]) {
        let p = *self.params.read();
        let dt = samples.len() as f32 / self.sample_rate.max(1) as f32;
        self.fft.push(samples);
        let bin_hz = self.fft.bin_hz();
        let amps = self.fft.analyse();
        let (bass, high) = (Self::band(amps, bin_hz, BASS_HZ), Self::band(amps, bin_hz, HIGH_HZ));
        let k = (-dt * 1000.0 / LEVEL_MS).exp();
        self.bass = bass + (self.bass - bass) * k;
        self.high = high + (self.high - high) * k;

        self.reference = (self.reference * (-dt / REFERENCE_DECAY_S).exp()).max(self.bass);
        let gone = self.reference > MIN_REFERENCE && self.bass < p.bass_ratio * self.reference;
        self.gone_for = if gone { self.gone_for + dt } else { 0.0 };

        if !self.breakdown && self.gone_for >= p.gone_s {
            self.breakdown = true;
            self.breakdown_for = self.gone_for;
            self.high_at_start = self.high.max(1e-4);
        } else if self.breakdown {
            self.breakdown_for += dt;
            // Back to most of its usual level: the breakdown is over.
            if self.bass > (p.bass_ratio + 0.2).min(0.95) * self.reference {
                if self.breakdown_for >= p.min_breakdown_s {
                    self.drops += 1;
                }
                self.breakdown = false;
                self.gone_for = 0.0;
            }
        }

        let target = if self.breakdown {
            ((self.high / self.high_at_start).max(1.0).ln() / FULL_TENSION_RATIO.ln()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // Rises follow the music, but it falls away at once at the drop.
        self.tension = if target > self.tension { target } else { target + (self.tension - target) * (-dt / 0.3).exp() };
    }

    fn outputs(&self) -> Vec<f32> {
        vec![self.tension, f32::from(u8::from(self.breakdown)), 0.0]
    }

    fn trigger_counts(&self) -> Vec<u64> { vec![self.drops] }
}

pub fn create(sample_rate: u32) -> (AnalyzerHandle, Box<dyn AnalyzerProc>) {
    let params = Arc::new(RwLock::new(BuildupParams::default()));
    let handle = handle_for(params.clone(), describe, apply);
    let proc = BuildupProc {
        params,
        sample_rate,
        fft: RollingFft::new(FFT, sample_rate),
        bass: 0.0,
        high: 0.0,
        reference: 0.0,
        gone_for: 0.0,
        breakdown: false,
        breakdown_for: 0.0,
        high_at_start: 1e-4,
        tension: 0.0,
        drops: 0,
    };
    (handle, Box::new(proc))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    fn feed(p: &mut dyn AnalyzerProc, seconds: f32, mut sample: impl FnMut(usize) -> f32) {
        let n = (RATE as f32 * seconds) as usize;
        let audio: Vec<f32> = (0..n).map(&mut sample).collect();
        for chunk in audio.chunks(512) {
            p.step(chunk);
        }
    }

    fn sine(freq: f32, amp: f32) -> impl FnMut(usize) -> f32 {
        move |i| amp * (std::f32::consts::TAU * freq * i as f32 / RATE as f32).sin()
    }

    #[test]
    fn a_breakdown_with_a_riser_builds_tension_and_ends_in_a_drop() {
        let (_h, mut p) = create(RATE);
        feed(p.as_mut(), 6.0, sine(60.0, 0.5));
        assert_eq!(p.outputs()[1], 0.0);

        // The bass leaves and a tone climbs from 1 kHz upwards, getting louder.
        let mut phase = 0.0f32;
        feed(p.as_mut(), 6.0, |i| {
            let t = i as f32 / RATE as f32;
            phase += std::f32::consts::TAU * (1000.0 + 600.0 * t) / RATE as f32;
            (0.02 + 0.1 * t / 6.0) * phase.sin()
        });
        let mid = p.outputs();
        assert_eq!(mid[1], 1.0, "in a breakdown");
        assert!(mid[0] > 0.6, "tension {}", mid[0]);
        assert_eq!(p.trigger_counts(), vec![0]);

        feed(p.as_mut(), 3.0, sine(60.0, 0.5));
        assert_eq!(p.trigger_counts(), vec![1], "the bass comes back as a drop");
        let after = p.outputs();
        assert_eq!(after[1], 0.0);
        assert!(after[0] < 0.2);
    }

    #[test]
    fn a_short_gap_in_the_bass_is_not_a_breakdown() {
        let (_h, mut p) = create(RATE);
        feed(p.as_mut(), 6.0, sine(60.0, 0.5));
        feed(p.as_mut(), 0.4, |_| 0.0);
        feed(p.as_mut(), 4.0, sine(60.0, 0.5));
        assert_eq!(p.trigger_counts(), vec![0]);
        assert_eq!(p.outputs()[1], 0.0);
    }

    #[test]
    fn the_trance_arrangement_is_a_breakdown_where_the_kick_and_bass_stop() {
        let g = crate::beat_bench::generated("trance_138_breakdown");
        let sr = g.truth.sample_rate as usize;
        let (_h, mut p) = create(g.truth.sample_rate);
        let &(from, to) = g.truth.no_kick.first().expect("a no-kick stretch");
        let at = |t: f64| ((t * sr as f64) as usize).min(g.audio.len());
        let mut state = |end: usize, start: usize, p: &mut Box<dyn AnalyzerProc>| {
            for chunk in g.audio[start..end].chunks(512) {
                p.step(chunk);
            }
            p.outputs()[1]
        };
        let before = state(at(from - 1.0), 0, &mut p);
        assert_eq!(before, 0.0, "not in a breakdown while the full track plays");
        let inside = state(at(from + 6.0), at(from - 1.0), &mut p);
        assert_eq!(inside, 1.0, "in a breakdown six seconds into the stretch without kick");
        let _ = to;
    }
}
