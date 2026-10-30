//! Ground truth that travels with a generated test signal.

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Truth {
    pub name: String,
    pub description: String,
    pub sample_rate: u32,
    pub duration_s: f64,
    /// (time, bpm) of the dominant grid, sampled twice a second.
    pub bpm: Vec<(f64, f64)>,
    /// Beat times of the dominant grid. In a mix the dominant grid switches
    /// from the outgoing to the incoming track at the middle of the crossfade.
    pub beats: Vec<f64>,
    /// Beats that start a bar (4/4, so every fourth beat).
    pub downbeats: Vec<f64>,
    /// Regions to leave out of steady-state scoring (two grids audible, or
    /// the grids deliberately disagree), with the reason.
    pub skip: Vec<(f64, f64, String)>,
    /// Stretches without any kick drum, for reading results by.
    pub no_kick: Vec<(f64, f64)>,
}

impl Truth {
    pub fn bpm_at(&self, t: f64) -> f64 {
        match self.bpm.binary_search_by(|(bt, _)| bt.total_cmp(&t)) {
            Ok(i) => self.bpm[i].1,
            Err(0) => self.bpm.first().map(|p| p.1).unwrap_or(0.0),
            Err(i) if i >= self.bpm.len() => self.bpm.last().map(|p| p.1).unwrap_or(0.0),
            Err(i) => {
                let (t0, b0) = self.bpm[i - 1];
                let (t1, b1) = self.bpm[i];
                b0 + (b1 - b0) * (t - t0) / (t1 - t0)
            }
        }
    }

    pub fn is_skipped(&self, t: f64) -> bool {
        self.skip.iter().any(|(a, b, _)| t >= *a && t < *b)
    }
}
