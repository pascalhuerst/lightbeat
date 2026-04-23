//! Real-time peak picker and adaptive threshold.
//!
//! Implements Algorithm 1 and eqns. (1)–(2) from the paper.
//!
//! **Latency:** one buffer. We can't "look ahead", so to check whether the
//! previous ODF value is a local maximum we must wait until the *next* value
//! arrives. The returned onset index is therefore always one buffer behind
//! the most recently pushed value.

use std::collections::VecDeque;

/// Tunable parameters for the picker. Defaults match the paper (§5.2).
#[derive(Clone, Copy, Debug)]
pub struct PeakPickerConfig {
    /// History window size for the median/mean threshold.
    pub median_window: usize,
    /// Median weight λ.
    pub lambda: f64,
    /// Mean weight α.
    pub alpha: f64,
    /// Largest-peak weight w in the N term. Set to 0 for indefinite real-time
    /// use, or update periodically to track dynamics.
    pub w: f64,
}

impl Default for PeakPickerConfig {
    fn default() -> Self {
        Self {
            median_window: 7,
            lambda: 1.0,
            alpha: 2.0,
            w: 0.05,
        }
    }
}

/// Real-time peak picker.
///
/// Feed ODF values in order via [`push`](Self::push); it returns `Some(index)`
/// on the step where the *previous* value is confirmed as an onset.
pub struct PeakPicker {
    config: PeakPickerConfig,
    history: VecDeque<f64>,
    /// Two-sample backlog for local-max detection: (two_ago, one_ago).
    prev: Option<f64>,
    two_ago: Option<f64>,
    /// Index of the most-recently pushed value (0-based).
    count: u64,
    /// Largest peak seen so far, for the N term.
    largest_peak: f64,
}

impl Default for PeakPicker {
    fn default() -> Self {
        Self::with_config(PeakPickerConfig::default())
    }
}

impl PeakPicker {
    pub fn with_config(config: PeakPickerConfig) -> Self {
        Self {
            config,
            history: VecDeque::with_capacity(config.median_window),
            prev: None,
            two_ago: None,
            count: 0,
            largest_peak: 0.0,
        }
    }

    /// Adaptive threshold σ_n = λ·median(O) + α·mean(O) + w·max_peak.
    fn threshold(&self) -> f64 {
        if self.history.is_empty() {
            return f64::INFINITY; // suppress early spurious onsets
        }
        let mut sorted: Vec<f64> = self.history.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = sorted[sorted.len() / 2];
        let mean: f64 = sorted.iter().sum::<f64>() / sorted.len() as f64;
        let n_term = self.config.w * self.largest_peak;
        self.config.lambda * median + self.config.alpha * mean + n_term
    }

    /// Push the next ODF value. `index` is the 0-based index of the frame /
    /// buffer this value corresponds to; it is returned unmodified as the
    /// onset location (referring to the *previous* frame).
    ///
    /// Returns `Some(onset_index)` if the previous value is a confirmed onset.
    pub fn push(&mut self, value: f64, index: u64) -> Option<u64> {
        self.count += 1;
        let mut onset = None;

        if let (Some(t), Some(p)) = (self.two_ago, self.prev) {
            if p > value && p > t && p > self.threshold() {
                if p > self.largest_peak {
                    self.largest_peak = p;
                }
                // Onset corresponds to the frame whose ODF value was `p` —
                // one index behind the one we just pushed.
                onset = Some(index.saturating_sub(1));
            }
        }

        // Roll the two-value window forward.
        self.two_ago = self.prev;
        self.prev = Some(value);

        // Update history used by the threshold. Use the newly-pushed value so
        // that `threshold()` above was computed from the window ending at the
        // value *before* this one, which matches the paper.
        if self.history.len() == self.config.median_window {
            self.history.pop_front();
        }
        self.history.push_back(value);

        onset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_obvious_spike() {
        let mut picker = PeakPicker::with_config(PeakPickerConfig {
            median_window: 5,
            lambda: 1.0,
            alpha: 1.0,
            w: 0.0,
        });
        // Quiet, spike, quiet.
        let series = [0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 10.0, 0.1, 0.1];
        let mut onsets = vec![];
        for (i, &v) in series.iter().enumerate() {
            if let Some(idx) = picker.push(v, i as u64) {
                onsets.push(idx);
            }
        }
        // The spike is at index 6. The picker confirms it on step 7 (one-buffer lag).
        assert_eq!(onsets, vec![6]);
    }

    #[test]
    fn rejects_flat_signal() {
        let mut picker = PeakPicker::default();
        for i in 0..50 {
            assert_eq!(picker.push(0.5, i), None);
        }
    }
}
