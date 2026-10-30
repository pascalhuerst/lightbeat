//! Does the music change here?
//!
//! The backbeat cannot tell beat 1 from beat 3, and in a four-on-the-floor
//! loop neither can the kick. What does differ is the harmony: chords and
//! bass lines move on the bar line, and new layers come in on phrase starts.
//! For every sixteenth-note boundary the clock passes, this compares the
//! spectrum of the two beats before it with the two beats after it. Summed
//! over a few bars, the boundaries where the music keeps changing are the
//! bar starts.
//!
//! Positions are in the beat clock's own coordinates, the same ones the bar
//! stage uses, so a hypothesis there can look its boundary up directly.

use std::collections::VecDeque;

use super::bar::STEPS;

pub const ENV_BANDS: usize = 32;
/// Beats each side of a boundary that are compared.
const SPAN_BEATS: f64 = 2.0;
/// Forgetting, per boundary visited (four per beat).
const HALF_LIFE_BEATS: f64 = 32.0;

pub struct NoveltyTracker {
    /// (clock phase when stored, spectral envelope).
    frames: VecDeque<(f64, [f32; ENV_BANDS])>,
    /// Added to stored phases: the clock has been corrected since.
    offset: f64,
    next_boundary: i64,
    hist: [f64; STEPS],
    weight: [f64; STEPS],
}

impl NoveltyTracker {
    pub fn new() -> Self {
        Self {
            frames: VecDeque::new(),
            offset: 0.0,
            next_boundary: 0,
            hist: [0.0; STEPS],
            weight: [0.0; STEPS],
        }
    }

    pub fn rotate(&mut self, delta: f64) {
        self.offset += delta;
    }

    /// Add a frame. `phase` is the clock phase at the frame, `beats_per_frame`
    /// the clock's speed.
    pub fn push(&mut self, phase: f64, envelope: [f32; ENV_BANDS], beats_per_frame: f64) {
        self.frames.push_back((phase - self.offset, envelope));
        let keep = 2.0 * SPAN_BEATS + 2.0;
        while self.frames.front().is_some_and(|(p, _)| phase - self.offset - p > keep) {
            self.frames.pop_front();
        }

        // A boundary can be judged once SPAN_BEATS of the music after it exist.
        let now = phase - SPAN_BEATS;
        if self.next_boundary == 0 && self.hist.iter().all(|v| *v == 0.0) {
            self.next_boundary = ((now - SPAN_BEATS) * 4.0).floor() as i64;
        }
        let decay = 0.5f64.powf(beats_per_frame / HALF_LIFE_BEATS);
        let _ = decay;
        while (self.next_boundary as f64) / 4.0 <= now {
            let at = self.next_boundary as f64 / 4.0;
            if let Some(n) = self.novelty_at(at) {
                let step = self.next_boundary.rem_euclid(STEPS as i64) as usize;
                // Older evidence fades a little with each visit.
                let f = 0.5f64.powf(0.25 / HALF_LIFE_BEATS * STEPS as f64);
                self.hist[step] = self.hist[step] * f + n;
                self.weight[step] = self.weight[step] * f + 1.0;
            }
            self.next_boundary += 1;
        }
    }

    fn novelty_at(&self, boundary: f64) -> Option<f64> {
        let (mut before, mut after) = ([0.0f32; ENV_BANDS], [0.0f32; ENV_BANDS]);
        let (mut nb, mut na) = (0usize, 0usize);
        for (p, env) in &self.frames {
            let p = p + self.offset;
            if p >= boundary - SPAN_BEATS && p < boundary {
                nb += 1;
                for (a, e) in before.iter_mut().zip(env) {
                    *a += e;
                }
            } else if p >= boundary && p < boundary + SPAN_BEATS {
                na += 1;
                for (a, e) in after.iter_mut().zip(env) {
                    *a += e;
                }
            }
        }
        if nb < 8 || na < 8 {
            return None;
        }
        let unit = |v: &mut [f32; ENV_BANDS]| {
            let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 1e-9 {
                v.iter_mut().for_each(|x| *x /= norm);
            }
        };
        unit(&mut before);
        unit(&mut after);
        Some(before.iter().zip(&after).map(|(a, b)| ((a - b) as f64).powi(2)).sum::<f64>().sqrt())
    }

    /// Per boundary step: how much more the music changes there than at an
    /// average boundary. 0 where nothing is known yet.
    pub fn relative(&self) -> [f64; STEPS] {
        let mut mean = [0.0f64; STEPS];
        let mut total = 0.0;
        let mut seen = 0;
        for i in 0..STEPS {
            if self.weight[i] > 0.0 {
                mean[i] = self.hist[i] / self.weight[i];
                total += mean[i];
                seen += 1;
            }
        }
        let mut out = [0.0; STEPS];
        if seen < STEPS / 2 || total <= 1e-9 {
            return out;
        }
        let avg = total / seen as f64;
        for i in 0..STEPS {
            if self.weight[i] > 0.0 {
                out[i] = mean[i] / avg - 1.0;
            }
        }
        out
    }
}
