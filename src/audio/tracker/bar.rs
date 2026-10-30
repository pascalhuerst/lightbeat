//! Where in the bar are we?
//!
//! The beat clock locks onto *some* position that the music repeats at: the
//! beat, but equally well the off-beat of a house track, whose bass and open
//! hat are as loud as its kick. This stage decides which of the sixteen
//! sixteenth-note positions in a bar the clock's zero point is, by comparing
//! what each band does around the bar with what dance music typically does:
//!
//! - kick: on the beats, strongest on 1;
//! - bass: moves to a new note on 1;
//! - snare/clap: on 2 and 4;
//! - hats: favour the off-beat.
//!
//! The comparison is made on a memory of the last several bars, so a missing
//! kick or a one-off fill does not move the answer.

use super::frontend::CHANNELS;

/// Histogram bins per beat, and per bar.
const BINS_PER_BEAT: usize = 16;
const BEATS_PER_BAR: usize = 4;
pub const BAR_BINS: usize = BINS_PER_BEAT * BEATS_PER_BAR;
pub const STEPS: usize = 16;
/// Memory of past bars, in beats.
const HALF_LIFE_BEATS: f64 = 24.0;

/// What each channel tends to do across the sixteen steps of a bar, with the
/// bar's first step at index 0. Only the shape matters; each is compared by
/// cosine similarity.
const TEMPLATES: [[f64; STEPS]; CHANNELS] = [
    // Kick: the beats, 1 most, a little off-beat syncopation.
    [1.0, 0.1, 0.2, 0.1, 0.7, 0.1, 0.25, 0.1, 0.85, 0.1, 0.3, 0.1, 0.7, 0.1, 0.2, 0.1],
    // Bass: a fresh note on 1, others anywhere on the grid.
    [1.0, 0.15, 0.4, 0.15, 0.4, 0.15, 0.4, 0.15, 0.6, 0.15, 0.4, 0.15, 0.4, 0.15, 0.4, 0.15],
    // Mid band: arps, vocals, pads; no useful shape.
    [0.5; STEPS],
    // Hats and cymbals: the off-beat, sixteenths in between.
    [0.35, 0.3, 0.6, 0.3, 0.35, 0.3, 0.6, 0.3, 0.35, 0.3, 0.6, 0.3, 0.35, 0.3, 0.6, 0.3],
    // Snare and clap: the backbeat, 2 and 4.
    [0.1, 0.0, 0.05, 0.0, 1.0, 0.05, 0.05, 0.1, 0.15, 0.0, 0.05, 0.0, 1.0, 0.05, 0.1, 0.15],
];
/// How much each channel's opinion counts.
const CHANNEL_WEIGHTS: [f64; CHANNELS] = [0.9, 0.7, 0.0, 0.6, 1.1];
/// How much a bar-start-like change in the music counts.
const NOVELTY_WEIGHT: f64 = 0.9;
/// A snare on beat 3 only (half-time) is as good a sign of the bar as 2 and 4.
const HALF_TIME_SNARE: [f64; STEPS] = [0.1, 0.0, 0.05, 0.0, 0.1, 0.0, 0.05, 0.0, 1.0, 0.05, 0.05, 0.1, 0.1, 0.0, 0.05, 0.0];
const HALF_TIME_DISCOUNT: f64 = 0.9;
/// Where the bar's strongest kick falls is usually its first beat. This adds
/// the kick's share of energy at the hypothesised bar start, which breaks the
/// tie in patterns where the kick is not the same on every beat.
const STRONGEST_KICK_WEIGHT: f64 = 0.8;

/// While the answer is young, a rival that leads for this many evaluations by
/// this margin takes over: the first bars of a track need to settle fast.
const SWITCH_AFTER: u32 = 3;
const SWITCH_MARGIN: f64 = 1.06;
/// Once an answer has held for `SETTLED_AFTER` evaluations it takes a much
/// longer and clearer lead to change it. The clock keeps running through a
/// breakdown, so staying put is nearly always right; only a real change of
/// track should move it.
const SETTLED_AFTER: u32 = 40;
const SETTLED_SWITCH_AFTER: u32 = 12;
const SETTLED_MARGIN: f64 = 1.12;
/// An evaluation only counts towards settling if the answer leads the
/// runner-up by at least this much (see `confidence`). In an intro with
/// nothing but a kick and hats every answer is as good as another, and
/// settling on one of them would pin the wrong bar start.
const SETTLE_CONFIDENCE: f64 = 0.10;
/// Smoothing of the per-hypothesis scores between evaluations.
const SCORE_ALPHA: f64 = 0.25;

pub struct BarTracker {
    hist: [[f64; BAR_BINS]; CHANNELS],
    score: [f64; STEPS],
    best: usize,
    rival: usize,
    rival_count: u32,
    held: u32,
    confidence: f64,
    seen: f64,
}

impl BarTracker {
    pub fn new() -> Self {
        Self {
            hist: [[0.0; BAR_BINS]; CHANNELS],
            score: [0.0; STEPS],
            best: 0,
            rival: 0,
            rival_count: 0,
            held: 0,
            confidence: 0.0,
            seen: 0.0,
        }
    }

    /// File one frame's band onset strengths under the clock's position in
    /// the bar, `beat_phase` being beats on any fixed origin.
    pub fn add(&mut self, beat_phase: f64, channels: &[f32; CHANNELS], beats_advanced: f64) {
        let decay = 0.5f64.powf(beats_advanced / HALF_LIFE_BEATS);
        let pos = beat_phase.rem_euclid(BEATS_PER_BAR as f64) * BINS_PER_BEAT as f64;
        let i = pos.floor() as usize % BAR_BINS;
        let f = pos - pos.floor();
        for (b, h) in self.hist.iter_mut().enumerate() {
            for v in h.iter_mut() {
                *v *= decay;
            }
            let s = channels[b] as f64;
            h[i] += s * (1.0 - f);
            h[(i + 1) % BAR_BINS] += s * f;
        }
        self.seen = self.seen * decay + 1.0;
    }

    /// The clock was moved by `delta` beats; keep the memory aligned with it.
    pub fn rotate(&mut self, delta: f64) {
        let shift = delta * BINS_PER_BEAT as f64;
        for h in self.hist.iter_mut() {
            let old = *h;
            for (i, v) in h.iter_mut().enumerate() {
                let src = i as f64 - shift;
                let j = src.floor();
                let f = src - j;
                let a = old[(j as i64).rem_euclid(BAR_BINS as i64) as usize];
                let b = old[(j as i64 + 1).rem_euclid(BAR_BINS as i64) as usize];
                *v = a * (1.0 - f) + b * f;
            }
        }
    }

    /// Forget which step was the bar start, e.g. after the clock jumped.
    pub fn reset_choice(&mut self) {
        self.score = [0.0; STEPS];
        self.rival_count = 0;
        self.held = 0;
    }

    /// Re-evaluate. Returns the step (0..16) of the clock's zero point that
    /// is the start of a bar.
    pub fn evaluate(&mut self, novelty: &[f64; STEPS]) -> usize {
        // Fold the histogram into sixteenth-note steps.
        let mut steps = [[0.0f64; STEPS]; CHANNELS];
        for (b, h) in self.hist.iter().enumerate() {
            for (bin, v) in h.iter().enumerate() {
                let step = ((bin + BINS_PER_BEAT / 8) / (BINS_PER_BEAT / 4)) % STEPS;
                steps[b][step] += v;
            }
        }

        let mut raw = [0.0f64; STEPS];
        for (b, obs) in steps.iter().enumerate() {
            let norm_obs = obs.iter().map(|v| v * v).sum::<f64>().sqrt();
            if norm_obs < 1e-9 {
                continue;
            }
            let t = &TEMPLATES[b];
            let norm_t = t.iter().map(|v| v * v).sum::<f64>().sqrt();
            let half = (b == CHANNELS - 1).then_some(&HALF_TIME_SNARE);
            let half_norm = half.map(|h| h.iter().map(|v| v * v).sum::<f64>().sqrt());
            for (h, r) in raw.iter_mut().enumerate() {
                // Hypothesis h: the bar starts at observed step h.
                let dot: f64 = (0..STEPS).map(|j| t[(j + STEPS - h) % STEPS] * obs[j]).sum();
                let mut sim = dot / (norm_obs * norm_t);
                if let (Some(ht), Some(hn)) = (half, half_norm) {
                    let hdot: f64 = (0..STEPS).map(|j| ht[(j + STEPS - h) % STEPS] * obs[j]).sum();
                    sim = sim.max(HALF_TIME_DISCOUNT * hdot / (norm_obs * hn));
                }
                *r += CHANNEL_WEIGHTS[b] * sim;
            }
        }
        let kick_max = steps[0].iter().cloned().fold(0.0, f64::max);
        if kick_max > 1e-9 {
            for (h, r) in raw.iter_mut().enumerate() {
                *r += STRONGEST_KICK_WEIGHT * steps[0][h] / kick_max;
            }
        }
        for (r, n) in raw.iter_mut().zip(novelty) {
            *r += NOVELTY_WEIGHT * n.clamp(-1.0, 2.0);
        }
        for (s, r) in self.score.iter_mut().zip(raw) {
            *s += SCORE_ALPHA * (r - *s);
        }

        let (top, &top_score) = self
            .score
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap_or((0, &0.0));
        let second = self
            .score
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != top)
            .map(|(_, v)| *v)
            .fold(0.0, f64::max);
        self.confidence = if top_score > 1e-9 { (1.0 - second / top_score).clamp(0.0, 1.0) } else { 0.0 };

        let settled = self.held >= SETTLED_AFTER;
        let (margin, needed) = if settled {
            (SETTLED_MARGIN, SETTLED_SWITCH_AFTER)
        } else {
            (SWITCH_MARGIN, SWITCH_AFTER)
        };
        if self.confidence >= SETTLE_CONFIDENCE {
            self.held += 1;
        }
        if top == self.best {
            self.rival_count = 0;
        } else if self.score[top] > self.score[self.best] * margin {
            if top == self.rival {
                self.rival_count += 1;
            } else {
                self.rival = top;
                self.rival_count = 1;
            }
            if self.rival_count >= needed {
                self.best = top;
                self.rival_count = 0;
                self.held = 0;
            }
        } else {
            self.rival_count = 0;
        }
        self.best
    }

    /// The smoothed score of each of the sixteen hypotheses, for diagnosing.
    pub fn scores(&self) -> [f64; STEPS] {
        self.score
    }

    /// 0..1: how clearly the best step beats the next one.
    pub fn confidence(&self) -> f64 {
        self.confidence
    }
}
