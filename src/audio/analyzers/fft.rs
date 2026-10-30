//! A rolling FFT over the most recent samples, for analyzers that look at
//! the spectrum.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

pub struct RollingFft {
    size: usize,
    sample_rate: u32,
    plan: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    ring: Vec<f32>,
    pos: usize,
    input: Vec<f32>,
    spectrum: Vec<Complex32>,
    scratch: Vec<Complex32>,
    amplitudes: Vec<f32>,
}

impl RollingFft {
    pub fn new(size: usize, sample_rate: u32) -> Self {
        let plan = RealFftPlanner::<f32>::new().plan_fft_forward(size);
        Self {
            size,
            sample_rate,
            window: (0..size)
                .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / size as f32).cos())
                .collect(),
            ring: vec![0.0; size],
            pos: 0,
            input: plan.make_input_vec(),
            spectrum: plan.make_output_vec(),
            scratch: plan.make_scratch_vec(),
            amplitudes: vec![0.0; size / 2 + 1],
            plan,
        }
    }

    pub fn bin_hz(&self) -> f32 {
        self.sample_rate as f32 / self.size as f32
    }

    pub fn push(&mut self, samples: &[f32]) {
        for &s in samples.iter().rev().take(self.size).rev() {
            self.ring[self.pos] = s;
            self.pos = (self.pos + 1) % self.size;
        }
    }

    /// Amplitude of each bin of the last `size` samples; a full-scale sine
    /// centred on a bin reads 1.0.
    pub fn analyse(&mut self) -> &[f32] {
        for i in 0..self.size {
            self.input[i] = self.ring[(self.pos + i) % self.size] * self.window[i];
        }
        let norm = self.size as f32 / 4.0;
        if self.plan.process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch).is_ok() {
            for (a, c) in self.amplitudes.iter_mut().zip(&self.spectrum) {
                *a = c.norm() / norm;
            }
        }
        &self.amplitudes
    }
}
