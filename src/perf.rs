//! Cross-thread performance counters.
//!
//! Each producing thread keeps its own smoothing state locally and publishes
//! only the result, so reading and writing these never introduces contention
//! between the engine and the UI — an instrument that perturbs what it
//! measures is worse than none.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Weight of a new sample in the exponential moving average. Low enough that
/// a single slow tick doesn't dominate the reading, high enough to react
/// within a second or so at 1 kHz.
const EMA_ALPHA: f64 = 0.01;

#[derive(Default)]
pub struct PerfStats {
    /// Smoothed engine tick duration, microseconds.
    engine_tick_us: AtomicU64,
    /// Worst engine tick since the last reset, microseconds.
    engine_tick_max_us: AtomicU64,
    /// Smoothed cost of the per-frame undo snapshot, microseconds.
    ui_snapshot_us: AtomicU64,
    /// Worst undo snapshot since the last reset, microseconds.
    ui_snapshot_max_us: AtomicU64,
    /// Number of undo snapshots taken in the current second, published once
    /// per second. Shows whether the interaction gate is doing its job.
    ui_snapshots_per_sec: AtomicU64,
}

impl PerfStats {
    pub fn engine_tick(&self) -> (u64, u64) {
        (
            self.engine_tick_us.load(Ordering::Relaxed),
            self.engine_tick_max_us.load(Ordering::Relaxed),
        )
    }

    pub fn ui_snapshot(&self) -> (u64, u64, u64) {
        (
            self.ui_snapshot_us.load(Ordering::Relaxed),
            self.ui_snapshot_max_us.load(Ordering::Relaxed),
            self.ui_snapshots_per_sec.load(Ordering::Relaxed),
        )
    }

    pub fn reset_peaks(&self) {
        self.engine_tick_max_us.store(0, Ordering::Relaxed);
        self.ui_snapshot_max_us.store(0, Ordering::Relaxed);
    }

    fn publish(&self, avg: &AtomicU64, max: &AtomicU64, ema: &mut f64, sample_us: u64) {
        *ema = if *ema == 0.0 {
            sample_us as f64
        } else {
            *ema * (1.0 - EMA_ALPHA) + sample_us as f64 * EMA_ALPHA
        };
        avg.store(*ema as u64, Ordering::Relaxed);
        if sample_us > max.load(Ordering::Relaxed) {
            max.store(sample_us, Ordering::Relaxed);
        }
    }
}

pub type SharedPerfStats = Arc<PerfStats>;

pub fn new_shared_perf_stats() -> SharedPerfStats {
    Arc::new(PerfStats::default())
}

/// Engine-thread-local smoothing state. Owned by the engine loop so the
/// moving average needs no synchronisation.
#[derive(Default)]
pub struct EngineTickTimer {
    ema_us: f64,
}

impl EngineTickTimer {
    /// Time `body` and publish the result as the engine tick duration.
    pub fn measure<R>(&mut self, stats: &PerfStats, body: impl FnOnce() -> R) -> R {
        let start = Instant::now();
        let out = body();
        let us = start.elapsed().as_micros() as u64;
        stats.publish(
            &stats.engine_tick_us,
            &stats.engine_tick_max_us,
            &mut self.ema_us,
            us,
        );
        out
    }
}

/// UI-thread-local smoothing state for the undo snapshot cost.
#[derive(Default)]
pub struct UiSnapshotTimer {
    ema_us: f64,
    count_this_sec: u64,
    sec_started: Option<f64>,
}

impl UiSnapshotTimer {
    /// Record one snapshot's cost. Takes an already-measured duration rather
    /// than a closure so the caller can borrow other fields of the app while
    /// building the snapshot.
    pub fn record(&mut self, stats: &PerfStats, elapsed_us: u64) {
        stats.publish(
            &stats.ui_snapshot_us,
            &stats.ui_snapshot_max_us,
            &mut self.ema_us,
            elapsed_us,
        );
        self.count_this_sec += 1;
    }

    /// Roll the per-second snapshot counter. Call once per frame with the
    /// egui context time, whether or not a snapshot was taken.
    pub fn tick_second(&mut self, stats: &PerfStats, now: f64) {
        let started = *self.sec_started.get_or_insert(now);
        if now - started >= 1.0 {
            stats
                .ui_snapshots_per_sec
                .store(self.count_this_sec, Ordering::Relaxed);
            self.count_this_sec = 0;
            self.sec_started = Some(now);
        }
    }
}
