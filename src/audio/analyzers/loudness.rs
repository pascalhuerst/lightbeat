//! Perceived loudness in LUFS (ITU-R BS.1770): the signal is filtered the way
//! the ear weighs frequencies, then its mean power is taken over 400 ms
//! (momentary) and 3 s (short term). Both are mapped from `floor_lufs`..0 to
//! 0..1, which suits a level that should stay comparable between quiet and
//! loud records.

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::audio::analyzers::{AnalyzerHandle, AnalyzerProc, handle_for};
use crate::engine::types::{ParamDef, ParamValue};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoudnessParams {
    /// The loudness that reads as 0.
    pub floor_lufs: f32,
}

impl Default for LoudnessParams {
    fn default() -> Self {
        Self { floor_lufs: -40.0 }
    }
}

fn describe(p: &LoudnessParams) -> Vec<ParamDef> {
    vec![ParamDef::Float { name: "floor_lufs".into(), value: p.floor_lufs, min: -70.0, max: -10.0, step: 1.0, unit: "LUFS" }]
}

fn apply(p: &mut LoudnessParams, index: usize, value: ParamValue) {
    if index == 0 {
        p.floor_lufs = value.as_f32().min(-1.0);
    }
}

/// One second-order section, direct form II transposed.
#[derive(Clone, Copy)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Biquad {
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// The two filters of K-weighting for `sample_rate`: a high shelf that
/// models the head, then a high-pass that drops the lowest octaves.
fn k_weighting(sample_rate: u32) -> [Biquad; 2] {
    let fs = sample_rate as f64;
    let (f0, gain, q) = (1681.974_450_955_533, 3.999_843_853_973_347, 0.707_175_236_955_419_6);
    let k = (std::f64::consts::PI * f0 / fs).tan();
    let vh = 10f64.powf(gain / 20.0);
    let vb = vh.powf(0.499_666_774_154_541_6);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad {
        b: [
            ((vh + vb * k / q + k * k) / a0) as f32,
            (2.0 * (k * k - vh) / a0) as f32,
            ((vh - vb * k / q + k * k) / a0) as f32,
        ],
        a: [(2.0 * (k * k - 1.0) / a0) as f32, ((1.0 - k / q + k * k) / a0) as f32],
        z: [0.0; 2],
    };
    let (f0, q) = (38.135_470_876_024_44, 0.500_327_037_323_877_3);
    let k = (std::f64::consts::PI * f0 / fs).tan();
    let a0 = 1.0 + k / q + k * k;
    let high_pass = Biquad {
        b: [1.0, -2.0, 1.0],
        a: [(2.0 * (k * k - 1.0) / a0) as f32, ((1.0 - k / q + k * k) / a0) as f32],
        z: [0.0; 2],
    };
    [shelf, high_pass]
}

/// Mean power over the most recent `window` samples, kept as per-chunk sums.
struct Window {
    window: u64,
    chunks: VecDeque<(u64, f64)>,
    samples: u64,
    sum: f64,
}

impl Window {
    fn new(window: u64) -> Self {
        Self { window, chunks: VecDeque::new(), samples: 0, sum: 0.0 }
    }

    fn push(&mut self, n: u64, sum: f64) {
        self.chunks.push_back((n, sum));
        self.samples += n;
        self.sum += sum;
        while self.samples > self.window && self.chunks.len() > 1 {
            let (n, s) = self.chunks.pop_front().unwrap();
            self.samples -= n;
            self.sum -= s;
        }
    }

    fn lufs(&self) -> f32 {
        let mean = (self.sum / self.samples.max(1) as f64).max(1e-12);
        (-0.691 + 10.0 * mean.log10()) as f32
    }
}

pub struct LoudnessProc {
    params: Arc<RwLock<LoudnessParams>>,
    filters: [Biquad; 2],
    momentary: Window,
    short_term: Window,
    out: [f32; 2],
}

impl AnalyzerProc for LoudnessProc {
    fn num_outputs(&self) -> usize { 2 }

    fn step(&mut self, samples: &[f32]) {
        let mut sum = 0.0f64;
        for &s in samples {
            let shelved = self.filters[0].process(s);
            let y = self.filters[1].process(shelved);
            sum += f64::from(y * y);
        }
        let n = samples.len() as u64;
        self.momentary.push(n, sum);
        self.short_term.push(n, sum);
        let floor = self.params.read().floor_lufs;
        let map = |lufs: f32| ((lufs - floor) / -floor).clamp(0.0, 1.0);
        self.out = [map(self.momentary.lufs()), map(self.short_term.lufs())];
    }

    fn outputs(&self) -> Vec<f32> { self.out.to_vec() }
}

pub fn create(sample_rate: u32) -> (AnalyzerHandle, Box<dyn AnalyzerProc>) {
    let params = Arc::new(RwLock::new(LoudnessParams::default()));
    let handle = handle_for(params.clone(), describe, apply);
    let rate = u64::from(sample_rate);
    let proc = LoudnessProc {
        params,
        filters: k_weighting(sample_rate),
        momentary: Window::new(rate * 4 / 10),
        short_term: Window::new(rate * 3),
        out: [0.0; 2],
    };
    (handle, Box::new(proc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, amp: f32, secs: f32) -> Vec<f32> {
        (0..(44_100.0 * secs) as usize)
            .map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / 44_100.0).sin())
            .collect()
    }

    fn lufs_of(audio: &[f32]) -> f32 {
        let mut f = k_weighting(44_100);
        let mut w = Window::new(u64::MAX);
        let mut sum = 0.0f64;
        for &s in audio {
            let shelved = f[0].process(s);
            let y = f[1].process(shelved);
            sum += f64::from(y * y);
        }
        w.push(audio.len() as u64, sum);
        w.lufs()
    }

    #[test]
    fn a_full_scale_1khz_sine_is_minus_three_lufs_and_each_6_db_is_6_lufs() {
        let loud = lufs_of(&sine(1000.0, 1.0, 2.0));
        assert!((loud + 3.0).abs() < 0.2, "{loud}");
        let quiet = lufs_of(&sine(1000.0, 0.5, 2.0));
        assert!((loud - quiet - 6.02).abs() < 0.1);
    }

    #[test]
    fn the_filter_ignores_deep_bass_that_the_ear_barely_hears() {
        assert!(lufs_of(&sine(1000.0, 0.5, 2.0)) - lufs_of(&sine(25.0, 0.5, 2.0)) > 6.0);
    }

    #[test]
    fn the_outputs_follow_the_level_and_the_short_term_one_is_slower() {
        let (_h, mut p) = create(44_100);
        for chunk in sine(1000.0, 0.5, 1.0).chunks(512) {
            p.step(chunk);
        }
        let quiet = p.outputs();
        for chunk in sine(1000.0, 0.5, 6.0).chunks(512) {
            p.step(chunk);
        }
        let steady = p.outputs();
        // -9 LUFS on a -40..0 scale.
        assert!((steady[0] - 0.775).abs() < 0.05, "{steady:?}");
        assert!((steady[1] - steady[0]).abs() < 0.02);
        assert!(quiet[1] <= steady[1] + 1e-3);
        for chunk in vec![0.0f32; 44_100].chunks(512) {
            p.step(chunk);
        }
        let after = p.outputs();
        assert!(after[0] < 0.01 && after[1] > after[0], "{after:?}");
    }
}
