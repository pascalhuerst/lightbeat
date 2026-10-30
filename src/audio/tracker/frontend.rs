//! Audio in, per-band onset strength out.
//!
//! The spectrum is split into four bands (kick, bass, snare/clap body, hats)
//! because dance music puts different information in each: the kick says
//! where the beat is, the claps where 2 and 4 are, the bass where chords
//! change. Each band's onset strength is normalised by its own recent level,
//! so quiet bands still count.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

pub const FFT: usize = 2048;
pub const HOP: usize = 256;
pub const BANDS: usize = 4;
/// Channels the bar stage looks at: the four bands plus a clap/snare channel.
pub const CHANNELS: usize = BANDS + 1;
pub use super::novelty::ENV_BANDS;

/// Band edges in Hz.
const EDGES_HZ: [f32; BANDS + 1] = [30.0, 120.0, 400.0, 2_500.0, 15_000.0];
/// How strongly each band feeds the combined onset strength.
const BAND_WEIGHTS: [f32; BANDS] = [1.0, 0.6, 0.8, 0.5];
/// Frames between the spectrum a frame is compared with and the frame itself.
const FLUX_LAG: usize = 2;
const LOG_COMPRESSION: f32 = 1000.0;
/// Time constant of the per-band level used for normalisation.
const LEVEL_TAU_S: f32 = 3.0;
/// Floor under the normalisation, so silence is not amplified into onsets.
const LEVEL_FLOOR: f32 = 0.05;

#[derive(Clone, Copy, Default)]
pub struct Frame {
    pub index: u64,
    /// Normalised onset strength per band.
    pub bands: [f32; BANDS],
    /// Weighted sum of `bands`.
    pub odf: f32,
    /// The bands, plus a clap/snare channel: onsets that are loud in the mid
    /// and the high band at once, which separates a clap from an arp (mid
    /// only) or a hat (high only).
    pub channels: [f32; CHANNELS],
    /// Smoothed spectral shape, for telling whether the music changed.
    pub envelope: [f32; ENV_BANDS],
}

pub struct Frontend {
    window: Vec<f32>,
    window_gain: f32,
    plan: Arc<dyn RealToComplex<f32>>,
    scratch: Vec<Complex32>,
    spectrum: Vec<Complex32>,
    input: Vec<f32>,
    pending: Vec<f32>,
    history: std::collections::VecDeque<Vec<f32>>,
    ranges: [(usize, usize); BANDS],
    level: [f32; BANDS],
    level_alpha: f32,
    next_index: u64,
    bin_hz: f32,
}

impl Frontend {
    pub fn new(sample_rate: u32) -> Self {
        let window: Vec<f32> = (0..FFT)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / FFT as f32).cos())
            .collect();
        let window_gain = 2.0 / window.iter().sum::<f32>();
        let mut planner = RealFftPlanner::<f32>::new();
        let plan = planner.plan_fft_forward(FFT);
        let bin_hz = sample_rate as f32 / FFT as f32;
        let mut ranges = [(0, 0); BANDS];
        for (b, r) in ranges.iter_mut().enumerate() {
            let lo = (EDGES_HZ[b] / bin_hz).round() as usize;
            let hi = ((EDGES_HZ[b + 1] / bin_hz).round() as usize).min(FFT / 2 - 1);
            *r = (lo.max(1), hi.max(lo + 1));
        }
        Self {
            window,
            window_gain,
            scratch: plan.make_scratch_vec(),
            spectrum: plan.make_output_vec(),
            plan,
            input: vec![0.0; FFT],
            pending: Vec::with_capacity(FFT * 2),
            history: std::collections::VecDeque::new(),
            ranges,
            level: [LEVEL_FLOOR; BANDS],
            level_alpha: HOP as f32 / sample_rate as f32 / LEVEL_TAU_S,
            next_index: 0,
            bin_hz,
        }
    }

    /// Frames per second.
    pub fn rate(sample_rate: u32) -> f64 {
        sample_rate as f64 / HOP as f64
    }

    pub fn push(&mut self, samples: &[f32], mut on_frame: impl FnMut(Frame)) {
        self.pending.extend_from_slice(samples);
        let mut consumed = 0;
        while self.pending.len() - consumed >= FFT {
            let chunk = &self.pending[consumed..consumed + FFT];
            for i in 0..FFT {
                self.input[i] = chunk[i] * self.window[i];
            }
            if self
                .plan
                .process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch)
                .is_ok()
            {
                let log: Vec<f32> = self
                    .spectrum
                    .iter()
                    .map(|c| (1.0 + LOG_COMPRESSION * c.norm() * self.window_gain).ln())
                    .collect();
                on_frame(self.analyse(log));
            }
            consumed += HOP;
        }
        self.pending.drain(..consumed);
    }

    fn analyse(&mut self, log: Vec<f32>) -> Frame {
        let mut frame = Frame { index: self.next_index, ..Default::default() };
        self.next_index += 1;

        if self.history.len() >= FLUX_LAG {
            let reference = &self.history[self.history.len() - FLUX_LAG];
            for (b, &(lo, hi)) in self.ranges.iter().enumerate() {
                // Compare with the loudest neighbour in the older frame, so a
                // steadily gliding tone does not read as a stream of onsets.
                let mut flux = 0.0;
                for k in lo..hi {
                    let r = reference[k - 1].max(reference[k]).max(reference[k + 1]);
                    flux += (log[k] - r).max(0.0);
                }
                flux /= (hi - lo) as f32;
                let strength = flux / (self.level[b] + LEVEL_FLOOR);
                self.level[b] += self.level_alpha * (flux - self.level[b]);
                frame.bands[b] = strength;
            }
        }
        frame.odf = frame.bands.iter().zip(BAND_WEIGHTS).map(|(s, w)| s * w).sum();
        frame.channels[..BANDS].copy_from_slice(&frame.bands);
        frame.channels[BANDS] = frame.bands[2].min(frame.bands[3]);

        // 32 log-spaced bands from 60 Hz to 6 kHz of compressed magnitude.
        let bin_hz = self.bin_hz;
        for (e, slot) in frame.envelope.iter_mut().enumerate() {
            let lo = 60.0 * (6_000.0f32 / 60.0).powf(e as f32 / ENV_BANDS as f32);
            let hi = 60.0 * (6_000.0f32 / 60.0).powf((e + 1) as f32 / ENV_BANDS as f32);
            let (a, b) = ((lo / bin_hz) as usize, ((hi / bin_hz) as usize).max((lo / bin_hz) as usize + 1));
            let b = b.min(log.len());
            if a < b {
                *slot = log[a..b].iter().sum::<f32>() / (b - a) as f32;
            }
        }

        self.history.push_back(log);
        if self.history.len() > FLUX_LAG {
            self.history.pop_front();
        }
        frame
    }
}
