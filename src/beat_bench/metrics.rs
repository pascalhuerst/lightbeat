//! Scoring of a tracker's output against a scenario's ground truth.

use super::truth::Truth;

/// What a tracker produced while the file played through it.
#[derive(Default)]
pub struct TrackerRun {
    /// (time, bpm) estimates; bpm 0 means "no estimate yet".
    pub tempo: Vec<(f64, f64)>,
    /// Times at which the tracker fired an onset.
    pub onsets: Vec<f64>,
    /// (time, beats since start) of the tracker's beat clock.
    pub phase: Vec<(f64, f64)>,
    /// (time, position in the bar in beats, 0..4).
    pub bar: Vec<(f64, f64)>,
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

pub struct PhaseScore {
    /// Share of scored time with the clock within 50 ms of the true beat.
    pub hit50: f64,
    /// Constant offset of the clock from the true grid, ms (positive = late).
    pub offset_ms: f64,
    /// 95th percentile of the error left after removing that offset, ms.
    pub jitter_p95_ms: f64,
    /// F-measure of the clock's beat times against the true beats, 70 ms.
    pub f1: f64,
    /// First time from which the clock stays within 50 ms for five seconds.
    pub lock_s: Option<f64>,
}

/// The true beat position (in beats) at time `t`.
fn truth_beat_position(truth: &Truth, t: f64) -> Option<f64> {
    let i = truth.beats.partition_point(|&b| b <= t);
    if i == 0 || i >= truth.beats.len() {
        return None;
    }
    let (b0, b1) = (truth.beats[i - 1], truth.beats[i]);
    Some((i - 1) as f64 + (t - b0) / (b1 - b0))
}

fn wrap_half(x: f64) -> f64 {
    x - x.round()
}

pub fn score_phase(truth: &Truth, run: &TrackerRun) -> PhaseScore {
    // (time, error in beats) wherever the truth is defined.
    let errors: Vec<(f64, f64)> = run
        .phase
        .iter()
        .filter_map(|&(t, phi)| {
            let tp = truth_beat_position(truth, t)?;
            Some((t, wrap_half(phi - tp)))
        })
        .collect();
    let to_ms = |t: f64, e: f64| e * 60_000.0 / truth.bpm_at(t);

    let scored: Vec<(f64, f64)> = errors
        .iter()
        .filter(|(t, _)| *t >= WARMUP_S && !truth.is_skipped(*t))
        .map(|&(t, e)| (t, to_ms(t, e)))
        .collect();

    let hit50 = if scored.is_empty() {
        f64::NAN
    } else {
        scored.iter().filter(|(_, ms)| ms.abs() < 50.0).count() as f64 / scored.len() as f64
    };

    // Circular mean of the error: a clock locked half a beat off is still a
    // steady clock, and that shows up as offset, not jitter.
    let (mut sx, mut sy) = (0.0, 0.0);
    for &(t, e) in errors.iter().filter(|(t, _)| *t >= WARMUP_S && !truth.is_skipped(*t)) {
        let _ = t;
        sx += (std::f64::consts::TAU * e).cos();
        sy += (std::f64::consts::TAU * e).sin();
    }
    let mean_offset = sy.atan2(sx) / std::f64::consts::TAU;
    let mut resid: Vec<f64> = errors
        .iter()
        .filter(|(t, _)| *t >= WARMUP_S && !truth.is_skipped(*t))
        .map(|&(t, e)| to_ms(t, wrap_half(e - mean_offset)).abs())
        .collect();
    resid.sort_by(|a, b| a.total_cmp(b));
    let jitter_p95_ms = resid.get((resid.len() as f64 * 0.95) as usize).copied().unwrap_or(f64::NAN);
    let offset_ms = to_ms(truth.duration_s * 0.5, mean_offset);

    // Beat times the clock announced: where its phase crosses an integer.
    let mut predicted = Vec::new();
    for w in run.phase.windows(2) {
        let ((t0, p0), (t1, p1)) = (w[0], w[1]);
        if p1 > p0 && p1 - p0 < 0.5 && p1.floor() > p0.floor() {
            let f = (p1.floor() - p0) / (p1 - p0);
            predicted.push(t0 + f * (t1 - t0));
        }
    }
    let in_window = |t: f64| t >= WARMUP_S && !truth.is_skipped(t);
    let truth_beats: Vec<f64> = truth.beats.iter().copied().filter(|t| in_window(*t)).collect();
    let preds: Vec<f64> = predicted.iter().copied().filter(|t| in_window(*t)).collect();
    let near = |list: &[f64], t: f64| list.iter().any(|x| (x - t).abs() <= 0.070);
    let recall = truth_beats.iter().filter(|&&b| near(&preds, b)).count() as f64 / truth_beats.len().max(1) as f64;
    let precision = preds.iter().filter(|&&p| near(&truth_beats, p)).count() as f64 / preds.len().max(1) as f64;
    let f1 = if recall + precision > 0.0 { 2.0 * recall * precision / (recall + precision) } else { 0.0 };

    let mut lock_s = None;
    let mut since: Option<f64> = None;
    for &(t, e) in &errors {
        if truth.is_skipped(t) {
            continue;
        }
        if to_ms(t, e).abs() < 50.0 {
            let s = *since.get_or_insert(t);
            if t - s >= 5.0 {
                lock_s = Some(s);
                break;
            }
        } else {
            since = None;
        }
    }

    PhaseScore { hit50, offset_ms, jitter_p95_ms, f1, lock_s }
}

/// Share of scored time with the tracker's position in the bar within 50 ms of
/// the truth. Being a whole beat off (the right off-beat, the wrong 1) counts
/// as wrong.
pub fn score_bar(truth: &Truth, run: &TrackerRun) -> f64 {
    let (mut n, mut ok) = (0usize, 0usize);
    for &(t, pos) in &run.bar {
        if t < WARMUP_S || truth.is_skipped(t) {
            continue;
        }
        let Some(now) = truth_beat_position(truth, t) else { continue };
        let i = truth.downbeats.partition_point(|&d| d <= t);
        if i == 0 {
            continue;
        }
        let Some(start) = truth_beat_position(truth, truth.downbeats[i - 1]) else { continue };
        let want = (now - start).rem_euclid(4.0);
        let mut e = (pos - want).rem_euclid(4.0);
        if e > 2.0 {
            e -= 4.0;
        }
        n += 1;
        if e.abs() * 60_000.0 / truth.bpm_at(t) < 50.0 {
            ok += 1;
        }
    }
    if n == 0 { f64::NAN } else { ok as f64 / n as f64 }
}

/// How the bar position was off, in quarter-beat steps from -8 to +7 (a
/// whole beat is 4 steps), for diagnosing which cue is missing.
pub fn bar_error_histogram(truth: &Truth, run: &TrackerRun) -> Vec<(i32, usize)> {
    let mut counts = std::collections::BTreeMap::new();
    for &(t, pos) in &run.bar {
        if t < WARMUP_S || truth.is_skipped(t) {
            continue;
        }
        let Some(now) = truth_beat_position(truth, t) else { continue };
        let i = truth.downbeats.partition_point(|&d| d <= t);
        if i == 0 {
            continue;
        }
        let Some(start) = truth_beat_position(truth, truth.downbeats[i - 1]) else { continue };
        let want = (now - start).rem_euclid(4.0);
        let step = ((pos - want) * 4.0).round() as i32;
        *counts.entry(step.rem_euclid(16)).or_insert(0usize) += 1;
    }
    let total: usize = counts.values().sum();
    counts.into_iter().filter(|(_, c)| *c * 20 >= total).collect()
}

/// Share of each ten-second window with the bar position right, for seeing
/// when a tracker goes wrong.
pub fn bar_by_window(truth: &Truth, run: &TrackerRun) -> Vec<(f64, f64)> {
    let mut windows: std::collections::BTreeMap<u32, (usize, usize)> = std::collections::BTreeMap::new();
    for &(t, pos) in &run.bar {
        if truth.is_skipped(t) {
            continue;
        }
        let Some(now) = truth_beat_position(truth, t) else { continue };
        let i = truth.downbeats.partition_point(|&d| d <= t);
        if i == 0 {
            continue;
        }
        let Some(start) = truth_beat_position(truth, truth.downbeats[i - 1]) else { continue };
        let want = (now - start).rem_euclid(4.0);
        let mut e = (pos - want).rem_euclid(4.0);
        if e > 2.0 {
            e -= 4.0;
        }
        let w = windows.entry((t / 10.0) as u32).or_default();
        w.0 += 1;
        if e.abs() * 60_000.0 / truth.bpm_at(t) < 50.0 {
            w.1 += 1;
        }
    }
    windows.into_iter().map(|(k, (n, ok))| (k as f64 * 10.0, ok as f64 / n as f64)).collect()
}
