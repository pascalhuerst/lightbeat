//! A beat clock that listens.
//!
//! The clock runs freely at the current tempo, so it keeps going through a
//! breakdown and never produces jitter of its own. Onset strength is binned
//! by the clock's phase at the moment it happened, with a slowly fading
//! memory of several beats; the position of the peak in that histogram says
//! how far the clock is from the music, and the clock is nudged by a fraction
//! of that. A small integral term pulls the tempo into line when the same
//! error keeps coming back.

use super::bar::BarTracker;
use super::frontend::{Frame, FFT, HOP};
use super::novelty::NoveltyTracker;

const BINS: usize = 48;
/// How long the memory of past onsets lasts, in beats.
const HALF_LIFE_BEATS: f64 = 6.0;
/// Frames between corrections.
const UPDATE_FRAMES: usize = 4;
/// Share of the measured error removed per update, once locked.
const GAIN: f64 = 0.12;
/// Tempo trim per update per beat of error.
const TRIM_GAIN: f64 = 0.0016;
const MAX_TRIM: f64 = 0.04;
/// Spread of the histogram smoothing, in bins.
const SMOOTH_SIGMA: f64 = 1.6;
/// Onset mass needed before the first correction (roughly beats' worth).
const MIN_EVIDENCE: f64 = 6.0;
/// How quickly the clock follows a new tempo hint, per frame.
const TEMPO_FOLLOW: f64 = 0.02;
/// Once locked the clock only follows a peak this close to its beat...
const TRACK_RADIUS_BEATS: f64 = 0.22;
/// ...unless a peak elsewhere stays this much taller for this many updates.
const REACQUIRE_RATIO: f64 = 1.7;
const REACQUIRE_UPDATES: u32 = 90;

pub struct PhaseTracker {
    dt: f64,
    /// Seconds between an onset and the end of the frame that sees it.
    delay_s: f64,
    hist: Vec<f64>,
    smooth_kernel: Vec<f64>,
    phi: f64,
    base_tempo: f64,
    trim: f64,
    evidence: f64,
    since_update: usize,
    acquired: bool,
    away_updates: u32,
    last_error: f64,
    sharpness: f64,
    bar: BarTracker,
    novelty: NoveltyTracker,
    bar_step: usize,
    since_bar: usize,
}

impl PhaseTracker {
    pub fn new(sample_rate: u32) -> Self {
        let radius = (SMOOTH_SIGMA * 3.0).ceil() as i32;
        Self {
            dt: HOP as f64 / sample_rate as f64,
            delay_s: 0.5 * FFT as f64 / sample_rate as f64,
            hist: vec![0.0; BINS],
            smooth_kernel: (-radius..=radius)
                .map(|d| (-0.5 * (d as f64 / SMOOTH_SIGMA).powi(2)).exp())
                .collect(),
            phi: 0.0,
            base_tempo: 0.0,
            trim: 0.0,
            evidence: 0.0,
            since_update: 0,
            acquired: false,
            away_updates: 0,
            last_error: 0.0,
            sharpness: 0.0,
            bar: BarTracker::new(),
            novelty: NoveltyTracker::new(),
            bar_step: 0,
            since_bar: 0,
        }
    }

    /// The clock's own phase, in beats: integers fall wherever the music
    /// repeats most, which need not be the beat.
    pub fn lock_phase(&self) -> f64 {
        self.phi
    }

    /// Beats since the start with integers on the beat (bar starts at
    /// multiples of four), as of the end of the last frame.
    pub fn phase(&self) -> f64 {
        self.phi - self.bar_step as f64 / 4.0
    }

    pub fn bar_scores(&self) -> ([f64; super::bar::STEPS], usize) {
        (self.bar.scores(), self.bar_step)
    }

    pub fn bar_confidence(&self) -> f64 {
        self.bar.confidence()
    }

    pub fn tempo(&self) -> f64 {
        self.base_tempo * (1.0 + self.trim)
    }

    /// How peaked the onset histogram is: 1 is flat, large is a clear beat.
    pub fn sharpness(&self) -> f64 {
        self.sharpness
    }

    /// Recent size of the correction, in beats.
    pub fn last_error(&self) -> f64 {
        self.last_error
    }

    pub fn step(&mut self, frame: &Frame, tempo_hint: f64) {
        let onset_strength = frame.odf;
        if tempo_hint <= 0.0 {
            return;
        }
        if self.base_tempo <= 0.0 {
            self.base_tempo = tempo_hint;
        } else {
            self.base_tempo += (tempo_hint - self.base_tempo) * TEMPO_FOLLOW;
        }
        let tempo = self.tempo();
        let beats_per_frame = tempo * self.dt / 60.0;
        self.phi += beats_per_frame;

        // Fade the memory, then file this frame's onset under the clock's
        // phase at the moment it actually happened.
        let decay = 0.5f64.powf(beats_per_frame / HALF_LIFE_BEATS);
        for h in self.hist.iter_mut() {
            *h *= decay;
        }
        self.evidence *= decay;
        let at_onset = (self.phi - tempo * self.delay_s / 60.0).rem_euclid(1.0);
        let pos = at_onset * BINS as f64;
        let i = pos.floor() as usize % BINS;
        let f = pos - pos.floor();
        let s = onset_strength as f64;
        self.hist[i] += s * (1.0 - f);
        self.hist[(i + 1) % BINS] += s * f;
        self.evidence += s;
        self.bar.add(self.phi - tempo * self.delay_s / 60.0, &frame.channels, beats_per_frame);
        self.novelty.push(self.phi - tempo * self.delay_s / 60.0, frame.envelope, beats_per_frame);

        self.since_bar += 1;
        if self.since_bar >= 64 {
            self.since_bar = 0;
            self.bar_step = self.bar.evaluate(&self.novelty.relative());
        }

        self.since_update += 1;
        if self.since_update >= UPDATE_FRAMES {
            self.since_update = 0;
            self.correct();
        }
    }

    fn correct(&mut self) {
        let smoothed = self.smoothed();
        let mean = smoothed.iter().sum::<f64>() / BINS as f64;
        if mean <= 1e-9 {
            return;
        }
        let global = self.peak_in(&smoothed, None);
        self.sharpness = smoothed[global.0] / mean;
        if self.evidence < MIN_EVIDENCE {
            return;
        }

        // Follow the peak nearest the clock's beat; only abandon it for a
        // taller one elsewhere after that has been so for a while. A house
        // track has a beat and an off-beat of similar weight, and flipping
        // between them every few seconds would be worse than either.
        let (error, jump) = if !self.acquired {
            (global.1, true)
        } else {
            let local = self.peak_in(&smoothed, Some(TRACK_RADIUS_BEATS));
            if smoothed[global.0] > REACQUIRE_RATIO * smoothed[local.0] {
                self.away_updates += 1;
            } else {
                self.away_updates = 0;
            }
            if self.away_updates >= REACQUIRE_UPDATES {
                self.away_updates = 0;
                (global.1, true)
            } else {
                (local.1, false)
            }
        };

        // The peak sits `error` beats after the clock's beat, so the clock is
        // early by that much: pull it back, and move the memories with it.
        let gain = if jump { 1.0 } else { GAIN };
        self.acquired = true;
        let delta = -gain * error;
        self.phi += delta;
        self.rotate(delta);
        self.bar.rotate(delta);
        self.novelty.rotate(delta);
        if jump {
            self.bar.reset_choice();
        } else {
            self.trim = (self.trim - TRIM_GAIN * error).clamp(-MAX_TRIM, MAX_TRIM);
        }
        self.last_error += (error.abs() - self.last_error) * 0.2;
    }

    /// The tallest bin of `smoothed`, optionally only within `radius` beats of
    /// the clock's beat, as (bin, error in beats with sign).
    fn peak_in(&self, smoothed: &[f64], radius: Option<f64>) -> (usize, f64) {
        let limit = radius.map(|r| (r * BINS as f64).round() as i64);
        let mut best = (0usize, f64::NEG_INFINITY);
        for (i, &v) in smoothed.iter().enumerate() {
            let signed = if i > BINS / 2 { i as i64 - BINS as i64 } else { i as i64 };
            if limit.is_some_and(|l| signed.abs() > l) {
                continue;
            }
            if v > best.1 {
                best = (i, v);
            }
        }
        let peak = best.0;
        let l = smoothed[(peak + BINS - 1) % BINS];
        let r = smoothed[(peak + 1) % BINS];
        let top = smoothed[peak];
        let denom = l - 2.0 * top + r;
        let offset = if denom < -1e-12 { (0.5 * (l - r) / denom).clamp(-1.0, 1.0) } else { 0.0 };
        let mut error = (peak as f64 + offset) / BINS as f64;
        if error > 0.5 {
            error -= 1.0;
        }
        (peak, error)
    }

    fn smoothed(&self) -> Vec<f64> {
        let radius = (self.smooth_kernel.len() / 2) as i32;
        (0..BINS as i32)
            .map(|i| {
                self.smooth_kernel
                    .iter()
                    .enumerate()
                    .map(|(k, w)| w * self.hist[(i + k as i32 - radius).rem_euclid(BINS as i32) as usize])
                    .sum::<f64>()
            })
            .collect()
    }

    /// Move the stored onsets by `delta` beats, keeping them aligned with the
    /// clock after it has been corrected.
    fn rotate(&mut self, delta: f64) {
        let shift = delta * BINS as f64;
        let old = self.hist.clone();
        for (i, h) in self.hist.iter_mut().enumerate() {
            let src = i as f64 - shift;
            let j = src.floor();
            let f = src - j;
            let a = old[(j as i64).rem_euclid(BINS as i64) as usize];
            let b = old[(j as i64 + 1).rem_euclid(BINS as i64) as usize];
            *h = a * (1.0 - f) + b * f;
        }
    }
}
