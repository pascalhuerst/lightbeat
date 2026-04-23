//! Sinusoidal modelling: per-frame spectral peak detection and
//! McAulay–Quatieri partial tracking (paper §4.1, §4.3, and ref. [26]).
//!
//! Used by the Peak Amplitude Difference ODF. Given a Hann-windowed FFT of
//! a frame we:
//!
//! 1. Find local maxima of the magnitude spectrum (with quadratic interp
//!    for sub-bin precision).
//! 2. Optionally keep only the `max_peaks` largest by amplitude.
//! 3. Match peaks to existing partials from the previous frame by nearest
//!    frequency within a tolerance. Unmatched current peaks start new
//!    partials; unmatched previous partials die.
//!
//! The PAD ODF only consumes matched-pair amplitude deltas and birth
//! amplitudes, which is what [`PartialTracker::step`] returns.

/// A spectral peak with (sub-bin) frequency in bins and linear magnitude.
#[derive(Clone, Copy, Debug)]
pub struct Peak {
    pub frequency_bin: f64,
    pub amplitude: f64,
}

/// Find magnitude-spectrum peaks with parabolic interpolation.
///
/// `magnitudes` should cover non-negative frequency bins only (length N/2+1).
/// Peaks in bin 0 and the Nyquist bin are excluded (they have no neighbours
/// on both sides).
pub fn detect_peaks(magnitudes: &[f64]) -> Vec<Peak> {
    let mut peaks = Vec::new();
    if magnitudes.len() < 3 {
        return peaks;
    }
    for k in 1..magnitudes.len() - 1 {
        let a = magnitudes[k - 1];
        let b = magnitudes[k];
        let c = magnitudes[k + 1];
        if b > a && b > c && b > 0.0 {
            // Parabolic interpolation (Smith, "Spectral Audio Signal Processing").
            let denom = a - 2.0 * b + c;
            let offset = if denom.abs() > f64::EPSILON {
                0.5 * (a - c) / denom
            } else {
                0.0
            };
            let interp_amp = b - 0.25 * (a - c) * offset;
            peaks.push(Peak {
                frequency_bin: k as f64 + offset,
                amplitude: interp_amp,
            });
        }
    }
    peaks
}

/// Keep only the `max_peaks` largest-amplitude peaks, preserving order by
/// frequency (matching helpers rely on frequency-sorted input).
pub fn limit_peaks(mut peaks: Vec<Peak>, max_peaks: usize) -> Vec<Peak> {
    if peaks.len() <= max_peaks {
        return peaks;
    }
    peaks.sort_by(|a, b| b.amplitude.partial_cmp(&a.amplitude).unwrap_or(std::cmp::Ordering::Equal));
    peaks.truncate(max_peaks);
    peaks.sort_by(|a, b| a.frequency_bin.partial_cmp(&b.frequency_bin).unwrap_or(std::cmp::Ordering::Equal));
    peaks
}

/// Result of one frame of tracking, expressed in the terms the PAD ODF needs.
#[derive(Default, Debug)]
pub struct TrackingStep {
    /// Absolute amplitude difference for each matched peak from last frame.
    pub matched_amp_deltas: Vec<f64>,
    /// Amplitudes of new peaks with no predecessor (per paper: treated as
    /// "start of partial, so delta = peak amplitude itself").
    pub birth_amps: Vec<f64>,
}

/// Minimal McAulay–Quatieri-style tracker. Matches peaks between consecutive
/// frames greedily within a frequency tolerance.
pub struct PartialTracker {
    /// Peaks retained from the previous frame (sorted by frequency).
    prev_peaks: Vec<Peak>,
    /// Matching tolerance in FFT bins.
    pub match_tolerance_bins: f64,
}

impl PartialTracker {
    pub fn new(match_tolerance_bins: f64) -> Self {
        Self {
            prev_peaks: Vec::new(),
            match_tolerance_bins,
        }
    }

    /// Process one frame's worth of (already amplitude-limited,
    /// frequency-sorted) peaks. Returns per-partial amplitude deltas suitable
    /// for the PAD ODF sum.
    pub fn step(&mut self, mut current: Vec<Peak>) -> TrackingStep {
        // Ensure both lists are sorted by frequency for greedy matching.
        current.sort_by(|a, b| a.frequency_bin.partial_cmp(&b.frequency_bin).unwrap_or(std::cmp::Ordering::Equal));

        let mut step = TrackingStep::default();
        let mut current_matched = vec![false; current.len()];

        // Greedy nearest-neighbour match: for each previous peak, find the
        // closest un-matched current peak within tolerance.
        for prev in &self.prev_peaks {
            let mut best: Option<(usize, f64)> = None;
            for (i, cur) in current.iter().enumerate() {
                if current_matched[i] {
                    continue;
                }
                let d = (cur.frequency_bin - prev.frequency_bin).abs();
                if d > self.match_tolerance_bins {
                    continue;
                }
                if best.map_or(true, |(_, best_d)| d < best_d) {
                    best = Some((i, d));
                }
            }
            match best {
                Some((i, _)) => {
                    current_matched[i] = true;
                    step.matched_amp_deltas.push((current[i].amplitude - prev.amplitude).abs());
                }
                // Unmatched previous peak: partial dies. Its amplitude going
                // to zero also counts towards unpredictability, per the PAD
                // intent ("the difference between the parameters of the noisy
                // peak and … will often differ significantly"). We treat this
                // as |0 - prev.amp| = prev.amp and fold it into births bucket
                // for clarity (same numerical effect on the sum).
                None => step.birth_amps.push(prev.amplitude),
            }
        }

        // Remaining unmatched current peaks are births.
        for (i, peak) in current.iter().enumerate() {
            if !current_matched[i] {
                step.birth_amps.push(peak.amplitude);
            }
        }

        self.prev_peaks = current;
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_peaks_finds_parabola_top() {
        // Triangle-like hump with max at index 3.
        let mags = vec![0.0, 0.5, 1.0, 2.0, 1.0, 0.5, 0.0];
        let peaks = detect_peaks(&mags);
        assert_eq!(peaks.len(), 1);
        assert!((peaks[0].frequency_bin - 3.0).abs() < 0.01);
        assert!(peaks[0].amplitude >= 2.0 - 1e-9);
    }

    #[test]
    fn limit_peaks_keeps_loudest() {
        let peaks = vec![
            Peak { frequency_bin: 10.0, amplitude: 0.1 },
            Peak { frequency_bin: 20.0, amplitude: 0.9 },
            Peak { frequency_bin: 30.0, amplitude: 0.5 },
            Peak { frequency_bin: 40.0, amplitude: 0.2 },
        ];
        let kept = limit_peaks(peaks, 2);
        assert_eq!(kept.len(), 2);
        // Still sorted by frequency.
        assert!(kept[0].frequency_bin < kept[1].frequency_bin);
        // The 0.9 and 0.5 should survive.
        let amps: Vec<f64> = kept.iter().map(|p| p.amplitude).collect();
        assert!(amps.contains(&0.9) && amps.contains(&0.5));
    }

    #[test]
    fn tracker_matches_stable_peak() {
        let mut t = PartialTracker::new(2.0);
        let _ = t.step(vec![Peak { frequency_bin: 50.0, amplitude: 1.0 }]);
        let step = t.step(vec![Peak { frequency_bin: 50.2, amplitude: 1.1 }]);
        assert_eq!(step.matched_amp_deltas.len(), 1);
        assert!((step.matched_amp_deltas[0] - 0.1).abs() < 1e-9);
        assert!(step.birth_amps.is_empty());
    }

    #[test]
    fn tracker_births_new_peaks() {
        let mut t = PartialTracker::new(2.0);
        let _ = t.step(vec![]);
        let step = t.step(vec![Peak { frequency_bin: 50.0, amplitude: 0.7 }]);
        assert_eq!(step.birth_amps, vec![0.7]);
    }
}
