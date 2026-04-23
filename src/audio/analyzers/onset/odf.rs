//! Onset Detection Functions (ODFs).
//!
//! Every ODF implements [`Odf::process`], which takes one audio frame and
//! returns a scalar "unpredictability" value for that frame.
//!
//! Categorisation:
//!
//! * Baselines (paper §2.3):
//!   * [`EnergyOdf`] — eqns. (3)–(4)
//!   * [`SpectralDifferenceOdf`] — eqns. (5)–(6)
//!   * [`ComplexDomainOdf`] — eqns. (7)–(10)
//! * Linear-prediction enhanced (paper §3):
//!   * [`EnergyLpOdf`] — eqn. (18)
//!   * [`SpectralDifferenceLpOdf`] — eqn. (19)
//!   * [`ComplexDomainLpOdf`] — eqn. (20)
//! * Sinusoidal-modelling (paper §4.3):
//!   * [`PeakAmplitudeDifferenceOdf`] — eqn. (23)

use std::collections::VecDeque;
use std::f64::consts::PI;
use std::sync::Arc;

use rustfft::{num_complex::Complex, FftPlanner, Fft};

use super::burg::{burg, predict_next};
use super::sinusoidal::{detect_peaks, limit_peaks, PartialTracker};

/// Common ODF interface. Each call consumes one frame of `frame_size` samples.
pub trait Odf {
    /// Process one frame, return its ODF value.
    ///
    /// # Panics
    /// Panics if `frame.len()` does not match the frame size the ODF was
    /// configured with.
    fn process(&mut self, frame: &[f32]) -> f64;
}

// ───────────────────────── Shared helpers ────────────────────────────────

/// Hann window of length `n`. The paper specifies a Hann window for the FFT
/// (eqn. 5). Recomputed once per ODF constructor.
fn hann(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / (n - 1) as f64).cos()))
        .collect()
}

/// Wrap `x` to the principal value range (−π, π].
fn princarg(x: f64) -> f64 {
    let two_pi = 2.0 * PI;
    let mut y = (x + PI).rem_euclid(two_pi) - PI;
    // rem_euclid can return −0.0; normalise to (−π, π].
    if y <= -PI {
        y += two_pi;
    }
    y
}

/// Bundles a reusable FFT plan, scratch buffers, window, and bin counts.
/// Spectral ODFs own one of these.
struct SpectralCore {
    frame_size: usize,
    window: Vec<f64>,
    fft: Arc<dyn Fft<f64>>,
    scratch: Vec<Complex<f64>>,
    buffer: Vec<Complex<f64>>,
    /// Number of non-negative-frequency bins (N/2 + 1).
    n_bins: usize,
}

impl SpectralCore {
    fn new(frame_size: usize) -> Self {
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(frame_size);
        let scratch_len = fft.get_inplace_scratch_len();
        Self {
            frame_size,
            window: hann(frame_size),
            fft,
            scratch: vec![Complex::default(); scratch_len],
            buffer: vec![Complex::default(); frame_size],
            n_bins: frame_size / 2 + 1,
        }
    }

    /// Compute the complex spectrum of the windowed frame.
    fn transform(&mut self, frame: &[f32]) -> &[Complex<f64>] {
        assert_eq!(frame.len(), self.frame_size);
        for (i, (&s, &w)) in frame.iter().zip(&self.window).enumerate() {
            self.buffer[i] = Complex::new(s as f64 * w, 0.0);
        }
        self.fft.process_with_scratch(&mut self.buffer, &mut self.scratch);
        &self.buffer[..self.n_bins]
    }
}

// ───────────────────────── 1. Energy ODF ──────────────────────────────────

/// Energy ODF (eqns. 3–4).
pub struct EnergyOdf {
    frame_size: usize,
    prev_energy: Option<f64>,
}

impl EnergyOdf {
    pub fn new(frame_size: usize) -> Self {
        Self { frame_size, prev_energy: None }
    }
}

impl Odf for EnergyOdf {
    fn process(&mut self, frame: &[f32]) -> f64 {
        assert_eq!(frame.len(), self.frame_size);
        let e: f64 = frame.iter().map(|&x| { let v = x as f64; v * v }).sum();
        let odf = match self.prev_energy {
            Some(p) => (e - p).abs(),
            None => 0.0,
        };
        self.prev_energy = Some(e);
        odf
    }
}

// ─────────────────────── 2. Spectral Difference ODF ───────────────────────

/// Spectral Difference ODF (eqn. 6).
pub struct SpectralDifferenceOdf {
    core: SpectralCore,
    prev_mag: Vec<f64>,
    have_prev: bool,
}

impl SpectralDifferenceOdf {
    pub fn new(frame_size: usize) -> Self {
        let core = SpectralCore::new(frame_size);
        let prev_mag = vec![0.0; core.n_bins];
        Self { core, prev_mag, have_prev: false }
    }
}

impl Odf for SpectralDifferenceOdf {
    fn process(&mut self, frame: &[f32]) -> f64 {
        let spectrum = self.core.transform(frame);
        let mut odf = 0.0;
        if self.have_prev {
            for (k, bin) in spectrum.iter().enumerate() {
                let m = bin.norm();
                odf += (m - self.prev_mag[k]).abs();
                self.prev_mag[k] = m;
            }
        } else {
            for (k, bin) in spectrum.iter().enumerate() {
                self.prev_mag[k] = bin.norm();
            }
            self.have_prev = true;
        }
        odf
    }
}

// ───────────────────────── 3. Complex Domain ODF ──────────────────────────

/// Complex Domain ODF (eqns. 7–10).
pub struct ComplexDomainOdf {
    core: SpectralCore,
    prev_mag: Vec<f64>,
    prev_phase: Vec<f64>,
    prev_prev_phase: Vec<f64>,
    frames_seen: u32,
}

impl ComplexDomainOdf {
    pub fn new(frame_size: usize) -> Self {
        let core = SpectralCore::new(frame_size);
        let n_bins = core.n_bins;
        Self {
            core,
            prev_mag: vec![0.0; n_bins],
            prev_phase: vec![0.0; n_bins],
            prev_prev_phase: vec![0.0; n_bins],
            frames_seen: 0,
        }
    }
}

impl Odf for ComplexDomainOdf {
    fn process(&mut self, frame: &[f32]) -> f64 {
        let spectrum = self.core.transform(frame);
        let mut odf = 0.0;
        // We need two prior frames for the phase prediction. Warm-up: return 0
        // until we have them.
        let warm = self.frames_seen >= 2;
        for (k, bin) in spectrum.iter().enumerate() {
            let r = bin.norm();
            let phi = bin.arg();
            if warm {
                let r_hat = self.prev_mag[k]; // eqn. 7
                let phi_hat = princarg(2.0 * self.prev_phase[k] - self.prev_prev_phase[k]); // eqn. 8
                // eqn. 9: Euclidean distance between complex phasors.
                let dphi = phi - phi_hat;
                let gamma_sq = r * r + r_hat * r_hat - 2.0 * r * r_hat * dphi.cos();
                odf += gamma_sq.max(0.0).sqrt();
            }
            self.prev_prev_phase[k] = self.prev_phase[k];
            self.prev_phase[k] = phi;
            self.prev_mag[k] = r;
        }
        self.frames_seen = self.frames_seen.saturating_add(1);
        odf
    }
}

// ───────────────────────── 4. Energy + LP ODF ─────────────────────────────

/// Energy with Linear Prediction ODF (eqn. 18).
pub struct EnergyLpOdf {
    frame_size: usize,
    order: usize,
    // History of past frame energies; newest pushed at the back.
    history: VecDeque<f64>,
    max_history: usize,
}

impl EnergyLpOdf {
    pub fn new(frame_size: usize, order: usize) -> Self {
        // Burg needs more samples than order; keep a healthy buffer.
        // The paper uses order 5; keeping 4× order is plenty and cheap.
        let max_history = (4 * order).max(order + 2);
        Self {
            frame_size,
            order,
            history: VecDeque::with_capacity(max_history + 1),
            max_history,
        }
    }
}

impl Odf for EnergyLpOdf {
    fn process(&mut self, frame: &[f32]) -> f64 {
        assert_eq!(frame.len(), self.frame_size);
        let e: f64 = frame.iter().map(|&x| { let v = x as f64; v * v }).sum();

        let odf = if self.history.len() > self.order {
            let signal: Vec<f64> = self.history.iter().copied().collect();
            let coeffs = burg(&signal, self.order);
            let start = signal.len() - self.order;
            let predicted = predict_next(&signal[start..], &coeffs);
            (e - predicted).abs()
        } else {
            0.0
        };

        if self.history.len() == self.max_history {
            self.history.pop_front();
        }
        self.history.push_back(e);
        odf
    }
}

// ─────────────────────── 5. Spectral Diff + LP ODF ────────────────────────

/// Spectral Difference with Linear Prediction ODF (eqn. 19).
///
/// For each FFT bin we maintain a short history of magnitudes and run Burg
/// to predict the next magnitude. The ODF is the sum over bins of the
/// absolute prediction error.
pub struct SpectralDifferenceLpOdf {
    core: SpectralCore,
    order: usize,
    max_history: usize,
    /// Per-bin history: bin_history[k] is a ring-like Vec, oldest-first.
    bin_history: Vec<VecDeque<f64>>,
}

impl SpectralDifferenceLpOdf {
    pub fn new(frame_size: usize, order: usize) -> Self {
        let core = SpectralCore::new(frame_size);
        let max_history = (4 * order).max(order + 2);
        let bin_history = (0..core.n_bins)
            .map(|_| VecDeque::with_capacity(max_history + 1))
            .collect();
        Self { core, order, max_history, bin_history }
    }
}

impl Odf for SpectralDifferenceLpOdf {
    fn process(&mut self, frame: &[f32]) -> f64 {
        let spectrum = self.core.transform(frame);
        let mut odf = 0.0;
        let mut signal_buf: Vec<f64> = Vec::with_capacity(self.max_history);

        for (k, bin) in spectrum.iter().enumerate() {
            let m = bin.norm();
            let hist = &mut self.bin_history[k];

            if hist.len() > self.order {
                signal_buf.clear();
                signal_buf.extend(hist.iter().copied());
                let coeffs = burg(&signal_buf, self.order);
                // Most recent `order` values for prediction.
                let start = signal_buf.len() - self.order;
                let predicted = predict_next(&signal_buf[start..], &coeffs);
                odf += (m - predicted).abs();
            }

            if hist.len() == self.max_history {
                hist.pop_front();
            }
            hist.push_back(m);
        }

        odf
    }
}

// ─────────────────────── 6. Complex Domain + LP ODF ───────────────────────

/// Complex Domain with Linear Prediction ODF (eqn. 20).
///
/// Per bin we compute Γ(k,n) — the Euclidean distance between consecutive
/// complex phasors — and LP-predict its next value.
pub struct ComplexDomainLpOdf {
    core: SpectralCore,
    order: usize,
    max_history: usize,
    prev_mag: Vec<f64>,
    prev_phase: Vec<f64>,
    have_prev: bool,
    /// History of Γ values per bin.
    gamma_history: Vec<VecDeque<f64>>,
}

impl ComplexDomainLpOdf {
    pub fn new(frame_size: usize, order: usize) -> Self {
        let core = SpectralCore::new(frame_size);
        let n_bins = core.n_bins;
        let max_history = (4 * order).max(order + 2);
        Self {
            core,
            order,
            max_history,
            prev_mag: vec![0.0; n_bins],
            prev_phase: vec![0.0; n_bins],
            have_prev: false,
            gamma_history: (0..n_bins)
                .map(|_| VecDeque::with_capacity(max_history + 1))
                .collect(),
        }
    }
}

impl Odf for ComplexDomainLpOdf {
    fn process(&mut self, frame: &[f32]) -> f64 {
        let spectrum = self.core.transform(frame);
        let mut odf = 0.0;
        let mut signal_buf: Vec<f64> = Vec::with_capacity(self.max_history);

        for (k, bin) in spectrum.iter().enumerate() {
            let r = bin.norm();
            let phi = bin.arg();

            if self.have_prev {
                let r_prev = self.prev_mag[k];
                let phi_prev = self.prev_phase[k];
                let dphi = phi - phi_prev;
                let gamma_sq = r * r + r_prev * r_prev - 2.0 * r * r_prev * dphi.cos();
                let gamma = gamma_sq.max(0.0).sqrt();

                let hist = &mut self.gamma_history[k];
                if hist.len() > self.order {
                    signal_buf.clear();
                    signal_buf.extend(hist.iter().copied());
                    let coeffs = burg(&signal_buf, self.order);
                    let start = signal_buf.len() - self.order;
                    let predicted = predict_next(&signal_buf[start..], &coeffs);
                    odf += (gamma - predicted).abs();
                }
                if hist.len() == self.max_history {
                    hist.pop_front();
                }
                hist.push_back(gamma);
            }

            self.prev_mag[k] = r;
            self.prev_phase[k] = phi;
        }
        self.have_prev = true;
        odf
    }
}

// ─────────────────── 7. Peak Amplitude Difference ODF ─────────────────────

/// Peak Amplitude Difference ODF (eqn. 23) — the paper's novel sinusoidal
/// contribution.
///
/// Note the additional **one-buffer latency** on top of the peak-picker
/// latency: total 1536 samples (34.8 ms @ 44.1 kHz) per paper §4.3.
/// That delay is intrinsic to the partial-tracking step, which needs the
/// *next* frame's peaks to confirm matching. In this streaming-friendly
/// implementation we approximate by matching against only the previous
/// frame (greedy), which keeps latency at one frame; this is a common
/// simplification and is what the equation as written requires.
pub struct PeakAmplitudeDifferenceOdf {
    core: SpectralCore,
    max_peaks: usize,
    tracker: PartialTracker,
}

impl PeakAmplitudeDifferenceOdf {
    /// `max_peaks` caps the number of peaks kept per frame (paper uses 20).
    /// `match_tolerance_bins` is the peak-matching tolerance in FFT bins;
    /// a reasonable value is 2–4 bins for a 2048-sample frame.
    pub fn new(frame_size: usize, max_peaks: usize, match_tolerance_bins: f64) -> Self {
        Self {
            core: SpectralCore::new(frame_size),
            max_peaks,
            tracker: PartialTracker::new(match_tolerance_bins),
        }
    }
}

impl Odf for PeakAmplitudeDifferenceOdf {
    fn process(&mut self, frame: &[f32]) -> f64 {
        let spectrum = self.core.transform(frame);
        let magnitudes: Vec<f64> = spectrum.iter().map(|c| c.norm()).collect();
        let peaks = detect_peaks(&magnitudes);
        let peaks = limit_peaks(peaks, self.max_peaks);
        let step = self.tracker.step(peaks);
        let sum_matched: f64 = step.matched_amp_deltas.iter().sum();
        let sum_births: f64 = step.birth_amps.iter().sum();
        sum_matched + sum_births
    }
}

// ─────────────────────────────── Tests ────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a frame of `n` samples of a sine at frequency `f` (cycles/sample).
    fn sine(n: usize, f: f64, amp: f32) -> Vec<f32> {
        (0..n).map(|i| (amp as f64 * (2.0 * PI * f * i as f64).sin()) as f32).collect()
    }

    /// Build a frame of `n` samples of white-ish noise (deterministic LCG).
    fn noise(n: usize, seed: u64, amp: f32) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let v = ((s >> 33) as u32) as f32 / u32::MAX as f32 * 2.0 - 1.0;
                v * amp
            })
            .collect()
    }

    #[test]
    fn energy_odf_spikes_on_amplitude_jump() {
        let mut odf = EnergyOdf::new(2048);
        // Warm up with a quiet sine.
        for _ in 0..5 {
            odf.process(&sine(2048, 0.01, 0.1));
        }
        let quiet = odf.process(&sine(2048, 0.01, 0.1));
        let loud = odf.process(&sine(2048, 0.01, 1.0));
        assert!(loud > quiet * 10.0, "loud={loud}, quiet={quiet}");
    }

    #[test]
    fn spectral_difference_detects_frequency_change() {
        let mut odf = SpectralDifferenceOdf::new(2048);
        // Warm up with one sine.
        for _ in 0..5 {
            odf.process(&sine(2048, 0.01, 0.5));
        }
        let steady = odf.process(&sine(2048, 0.01, 0.5));
        let changed = odf.process(&sine(2048, 0.05, 0.5));
        assert!(changed > steady * 5.0, "steady={steady}, changed={changed}");
    }

    #[test]
    fn complex_domain_detects_change() {
        let mut odf = ComplexDomainOdf::new(2048);
        for _ in 0..5 {
            odf.process(&sine(2048, 0.01, 0.5));
        }
        let steady = odf.process(&sine(2048, 0.01, 0.5));
        let changed = odf.process(&sine(2048, 0.05, 0.5));
        assert!(changed > steady * 2.0, "steady={steady}, changed={changed}");
    }

    #[test]
    fn energy_lp_runs_and_spikes() {
        let mut odf = EnergyLpOdf::new(2048, 5);
        for _ in 0..20 {
            odf.process(&sine(2048, 0.01, 0.1));
        }
        let quiet = odf.process(&sine(2048, 0.01, 0.1));
        let loud = odf.process(&sine(2048, 0.01, 1.0));
        assert!(loud > quiet, "lp energy: loud={loud}, quiet={quiet}");
    }

    #[test]
    fn spectral_lp_runs() {
        let mut odf = SpectralDifferenceLpOdf::new(2048, 5);
        for _ in 0..10 {
            let _ = odf.process(&sine(2048, 0.01, 0.5));
        }
        let v = odf.process(&sine(2048, 0.01, 0.5));
        assert!(v.is_finite());
    }

    #[test]
    fn complex_lp_runs() {
        let mut odf = ComplexDomainLpOdf::new(2048, 5);
        for _ in 0..10 {
            let _ = odf.process(&sine(2048, 0.01, 0.5));
        }
        let v = odf.process(&sine(2048, 0.01, 0.5));
        assert!(v.is_finite());
    }

    #[test]
    fn pad_detects_onset_from_silence_to_tone() {
        let mut odf = PeakAmplitudeDifferenceOdf::new(2048, 20, 2.0);
        // A couple of silent frames then a tone.
        let silent = vec![0.0f32; 2048];
        let tone = sine(2048, 0.02, 0.8);
        let _ = odf.process(&silent);
        let _ = odf.process(&silent);
        let before = odf.process(&silent);
        let after = odf.process(&tone);
        assert!(after > before + 1.0, "PAD: before={before}, after={after}");
    }

    #[test]
    fn pad_settles_during_steady_state() {
        let mut odf = PeakAmplitudeDifferenceOdf::new(2048, 20, 2.0);
        let tone = sine(2048, 0.02, 0.5);
        for _ in 0..3 {
            let _ = odf.process(&tone);
        }
        // Now compare steady tone vs noisy burst — noise should register higher.
        let steady = odf.process(&tone);
        let noisy = odf.process(&noise(2048, 42, 0.5));
        assert!(noisy > steady, "PAD: steady={steady}, noisy={noisy}");
    }

    #[test]
    fn princarg_wraps_correctly() {
        assert!((princarg(0.0)).abs() < 1e-12);
        assert!((princarg(PI) - PI).abs() < 1e-12);
        assert!((princarg(PI + 0.1) - (-PI + 0.1)).abs() < 1e-9);
        assert!((princarg(3.0 * PI) - PI).abs() < 1e-9);
    }
}
