//! Tempo from the onset strength, followed over time.
//!
//! Every half second the recent onset strength is autocorrelated and each
//! candidate tempo is scored by how well it explains the repetition at one,
//! two, three and four beats. The scores are fed into a forward filter over
//! tempo, so the estimate moves smoothly the way a record's pitch does, and a
//! single bad second cannot throw it. A broad preference for dance tempos
//! settles the half/double ambiguity.

use std::collections::VecDeque;

pub const DEFAULT_MIN_BPM: f64 = 80.0;
pub const DEFAULT_MAX_BPM: f64 = 180.0;
/// Spacing of tempo candidates, as a ratio.
const BIN_RATIO: f64 = 1.004;
/// Seconds of onset strength each estimate looks at.
const WINDOW_S: f64 = 6.0;
const MIN_WINDOW_S: f64 = 3.0;
const UPDATE_S: f64 = 0.5;
/// Preferred tempo and the width of the preference, in octaves.
const PRIOR_CENTRE_BPM: f64 = 122.0;
const PRIOR_WIDTH_OCT: f64 = 0.5;
/// How much the preference counts at every update (it compounds).
const PRIOR_EXPONENT: f64 = 0.12;
/// How sharply the autocorrelation score separates candidates.
const SHARPNESS: f64 = 7.0;
/// Spread of the tempo change expected between two updates, in bins.
const TRANSITION_SIGMA_BINS: f64 = 1.4;
/// Probability mass that may jump anywhere at an update (tempo changes).
const JUMP_FLOOR: f64 = 0.004;
/// Weights of the autocorrelation at one to four beats.
const HARMONICS: [f64; 4] = [1.0, 0.8, 0.6, 0.6];

pub struct TempoTracker {
    rate: f64,
    odf: VecDeque<f32>,
    bpm: Vec<f64>,
    prior: Vec<f64>,
    posterior: Vec<f64>,
    kernel: Vec<f64>,
    since_update: usize,
    tempo: f64,
    confidence: f64,
}

impl TempoTracker {
    pub fn new(frame_rate: f64, min_bpm: f64, max_bpm: f64) -> Self {
        let mut bpm = Vec::new();
        let mut b = min_bpm;
        while b <= max_bpm {
            bpm.push(b);
            b *= BIN_RATIO;
        }
        let prior: Vec<f64> = bpm
            .iter()
            .map(|b| (-0.5 * ((b / PRIOR_CENTRE_BPM).log2() / PRIOR_WIDTH_OCT).powi(2)).exp())
            .collect();
        let total: f64 = prior.iter().sum();
        let posterior = prior.iter().map(|p| p / total).collect();
        let radius = (TRANSITION_SIGMA_BINS * 3.0).ceil() as i32;
        let kernel = (-radius..=radius)
            .map(|d| (-0.5 * (d as f64 / TRANSITION_SIGMA_BINS).powi(2)).exp())
            .collect();
        Self {
            rate: frame_rate,
            odf: VecDeque::new(),
            bpm,
            prior,
            posterior,
            kernel,
            since_update: 0,
            tempo: 0.0,
            confidence: 0.0,
        }
    }

    /// Current estimate in BPM, 0 until there is enough audio.
    pub fn tempo(&self) -> f64 {
        self.tempo
    }

    /// Probability mass near the estimate, 0..1.
    pub fn confidence(&self) -> f64 {
        self.confidence
    }

    pub fn push(&mut self, onset_strength: f32) {
        self.odf.push_back(onset_strength);
        let max = (WINDOW_S * self.rate) as usize;
        while self.odf.len() > max {
            self.odf.pop_front();
        }
        self.since_update += 1;
        if self.since_update as f64 >= UPDATE_S * self.rate && self.odf.len() as f64 >= MIN_WINDOW_S * self.rate {
            self.since_update = 0;
            self.update();
        }
    }

    fn update(&mut self) {
        let x: Vec<f32> = self.odf.iter().copied().collect();
        let mean = x.iter().sum::<f32>() / x.len() as f32;
        let x: Vec<f32> = x.iter().map(|v| v - mean).collect();
        let n = x.len();

        let slowest = *self.bpm.first().unwrap_or(&DEFAULT_MIN_BPM);
        let max_lag = ((60.0 / slowest * self.rate) * 4.0) as usize + 2;
        let max_lag = max_lag.min(n - 1);
        let mut acf = vec![0.0f64; max_lag + 1];
        for (lag, a) in acf.iter_mut().enumerate() {
            let mut s = 0.0f64;
            for i in lag..n {
                s += x[i] as f64 * x[i - lag] as f64;
            }
            *a = s / (n - lag) as f64;
        }
        let at = |lag: f64| -> f64 {
            if lag < 0.0 || lag >= max_lag as f64 {
                return 0.0;
            }
            let i = lag as usize;
            let f = lag - i as f64;
            acf[i] * (1.0 - f) + acf[(i + 1).min(max_lag)] * f
        };

        let weight_sum: f64 = HARMONICS.iter().sum::<f64>() + 0.3;
        let scores: Vec<f64> = self
            .bpm
            .iter()
            .map(|b| {
                let period = 60.0 / b * self.rate;
                let mut s = 0.3 * at(period * 0.5);
                for (h, w) in HARMONICS.iter().enumerate() {
                    s += w * at(period * (h + 1) as f64);
                }
                (s / weight_sum).max(0.0)
            })
            .collect();
        let best = scores.iter().cloned().fold(0.0, f64::max);
        if best <= 1e-9 {
            return;
        }

        // Forward filter: blur the old belief by the tempo change allowed in
        // half a second, then weigh it by how well each tempo fits now.
        let radius = (self.kernel.len() / 2) as i32;
        let nb = self.bpm.len() as i32;
        let mut next = vec![0.0f64; self.bpm.len()];
        for i in 0..nb {
            let mut acc = 0.0;
            for (k, w) in self.kernel.iter().enumerate() {
                let j = i + k as i32 - radius;
                if (0..nb).contains(&j) {
                    acc += w * self.posterior[j as usize];
                }
            }
            let evidence = (SHARPNESS * (scores[i as usize] / best - 1.0)).exp();
            next[i as usize] = (acc + JUMP_FLOOR / nb as f64)
                * evidence
                * self.prior[i as usize].powf(PRIOR_EXPONENT);
        }
        let total: f64 = next.iter().sum();
        if total <= 0.0 {
            return;
        }
        for v in next.iter_mut() {
            *v /= total;
        }
        self.posterior = next;

        let (peak, _) = self
            .posterior
            .iter()
            .enumerate()
            .fold((0, 0.0), |m, (i, &p)| if p > m.1 { (i, p) } else { m });
        // Parabolic interpolation in log-tempo.
        let mut tempo = self.bpm[peak];
        if peak > 0 && peak + 1 < self.bpm.len() {
            let (a, b, c) = (self.posterior[peak - 1], self.posterior[peak], self.posterior[peak + 1]);
            let denom = a - 2.0 * b + c;
            if denom < -1e-12 {
                let delta = (0.5 * (a - c) / denom).clamp(-1.0, 1.0);
                tempo = self.bpm[peak] * BIN_RATIO.powf(delta);
            }
        }
        self.tempo = tempo;
        let lo = peak.saturating_sub(3);
        let hi = (peak + 3).min(self.bpm.len() - 1);
        self.confidence = self.posterior[lo..=hi].iter().sum();
    }
}
