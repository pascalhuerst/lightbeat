//! Scoring of a tracker's output against a scenario's ground truth.

use super::truth::Truth;

/// What a tracker produced while the file played through it.
#[derive(Default)]
pub struct TrackerRun {
    /// (time, bpm) estimates; bpm 0 means "no estimate yet".
    pub tempo: Vec<(f64, f64)>,
    /// Times at which the tracker fired an onset.
    pub onsets: Vec<f64>,
}

/// Steady-state scoring starts this long into a file, so start-up is
/// reported separately as time to lock.
const WARMUP_S: f64 = 10.0;

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

pub struct TempoScore {
    /// Share of scored time with the estimate within 2% of the truth.
    pub acc1: f64,
    /// The same, also accepting half, double, a third and three times.
    pub acc2: f64,
    /// Median estimate divided by the true tempo.
    pub median_ratio: f64,
    /// First time from which the estimate stays within 2% for five seconds.
    pub lock_s: Option<f64>,
}

pub fn score_tempo(truth: &Truth, run: &TrackerRun) -> TempoScore {
    let mut n = 0usize;
    let (mut ok1, mut ok2) = (0usize, 0usize);
    let mut ratios = Vec::new();
    for &(t, est) in &run.tempo {
        if t < WARMUP_S || truth.is_skipped(t) || est <= 0.0 {
            continue;
        }
        let want = truth.bpm_at(t);
        n += 1;
        let ratio = est / want;
        ratios.push(ratio);
        if (ratio - 1.0).abs() < 0.02 {
            ok1 += 1;
        }
        if [1.0, 2.0, 0.5, 3.0, 1.0 / 3.0].iter().any(|f| (ratio / f - 1.0).abs() < 0.02) {
            ok2 += 1;
        }
    }

    // Time to lock: scan for the first point after which five seconds in a
    // row are right.
    let mut lock_s = None;
    let mut run_start: Option<f64> = None;
    for &(t, est) in &run.tempo {
        let good = est > 0.0 && (est / truth.bpm_at(t) - 1.0).abs() < 0.02;
        match (good, run_start) {
            (true, None) => run_start = Some(t),
            (true, Some(s)) if t - s >= 5.0 => {
                lock_s = Some(s);
                break;
            }
            (false, _) => run_start = None,
            _ => {}
        }
    }

    TempoScore {
        acc1: if n > 0 { ok1 as f64 / n as f64 } else { f64::NAN },
        acc2: if n > 0 { ok2 as f64 / n as f64 } else { f64::NAN },
        median_ratio: median(&mut ratios),
        lock_s,
    }
}

pub struct OnsetScore {
    pub onsets_per_beat: f64,
    /// Share of true beats with an onset within 50 ms.
    pub beat_hit: f64,
    /// Share of onsets within 40 ms of some sixteenth of the true grid.
    pub on_grid: f64,
}

pub fn score_onsets(truth: &Truth, run: &TrackerRun) -> OnsetScore {
    let beats: Vec<f64> = truth.beats.iter().copied().filter(|t| *t >= WARMUP_S && !truth.is_skipped(*t)).collect();
    let onsets: Vec<f64> = run.onsets.iter().copied().filter(|t| *t >= WARMUP_S && !truth.is_skipped(*t)).collect();

    let hits = beats
        .iter()
        .filter(|&&b| onsets.iter().any(|&o| (o - b).abs() <= 0.05))
        .count();

    let on_grid = onsets
        .iter()
        .filter(|&&o| {
            let i = truth.beats.partition_point(|&b| b <= o);
            if i == 0 || i >= truth.beats.len() {
                return false;
            }
            let (b0, b1) = (truth.beats[i - 1], truth.beats[i]);
            let quarter = (b1 - b0) / 4.0;
            let pos = (o - b0) / quarter;
            (pos - pos.round()).abs() * quarter <= 0.04
        })
        .count();

    OnsetScore {
        onsets_per_beat: if beats.is_empty() { f64::NAN } else { onsets.len() as f64 / beats.len() as f64 },
        beat_hit: if beats.is_empty() { f64::NAN } else { hits as f64 / beats.len() as f64 },
        on_grid: if onsets.is_empty() { f64::NAN } else { on_grid as f64 / onsets.len() as f64 },
    }
}
