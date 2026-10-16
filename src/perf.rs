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

/// A smoothed average plus a running peak, both in microseconds.
#[derive(Default)]
pub struct Gauge {
    avg_us: AtomicU64,
    max_us: AtomicU64,
}

impl Gauge {
    /// `ema` is the caller's thread-local smoothing state.
    fn publish(&self, ema: &mut f64, sample_us: u64) {
        *ema = if *ema == 0.0 {
            sample_us as f64
        } else {
            *ema * (1.0 - EMA_ALPHA) + sample_us as f64 * EMA_ALPHA
        };
        self.avg_us.store(*ema as u64, Ordering::Relaxed);
        if sample_us > self.max_us.load(Ordering::Relaxed) {
            self.max_us.store(sample_us, Ordering::Relaxed);
        }
    }

    /// Returns (average, peak) in microseconds.
    pub fn read(&self) -> (u64, u64) {
        (
            self.avg_us.load(Ordering::Relaxed),
            self.max_us.load(Ordering::Relaxed),
        )
    }

    fn reset_peak(&self) {
        self.max_us.store(0, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub struct PerfStats {
    /// One engine tick: all node processing plus signal propagation.
    pub engine_tick: Gauge,
    /// One UI frame's `update()` — our own layout and widget work, not the
    /// GPU pass that follows it.
    pub ui_frame: Gauge,
    /// The node graph canvas alone, as a share of `ui_frame`.
    pub ui_graph: Gauge,
    /// Building and comparing the undo snapshot.
    pub ui_snapshot: Gauge,
    /// Undo snapshots taken in the last whole second.
    snapshots_per_sec: AtomicU64,
    /// Wall time between frames, i.e. what the display actually gets.
    frame_interval_us: AtomicU64,
}

impl PerfStats {
    pub fn snapshots_per_sec(&self) -> u64 {
        self.snapshots_per_sec.load(Ordering::Relaxed)
    }

    pub fn frame_interval_us(&self) -> u64 {
        self.frame_interval_us.load(Ordering::Relaxed)
    }

    pub fn reset_peaks(&self) {
        self.engine_tick.reset_peak();
        self.ui_frame.reset_peak();
        self.ui_graph.reset_peak();
        self.ui_snapshot.reset_peak();
    }
}

pub type SharedPerfStats = Arc<PerfStats>;

pub fn new_shared_perf_stats() -> SharedPerfStats {
    Arc::new(PerfStats::default())
}

/// Engine-thread-local smoothing state, so the moving average needs no
/// synchronisation.
#[derive(Default)]
pub struct EngineTickTimer {
    ema_us: f64,
}

impl EngineTickTimer {
    /// Time `body` and publish the result as the engine tick duration.
    pub fn measure<R>(&mut self, stats: &PerfStats, body: impl FnOnce() -> R) -> R {
        let start = Instant::now();
        let out = body();
        stats
            .engine_tick
            .publish(&mut self.ema_us, start.elapsed().as_micros() as u64);
        out
    }
}

/// UI-thread-local smoothing state for the per-frame gauges.
#[derive(Default)]
pub struct UiTimers {
    frame_ema: f64,
    graph_ema: f64,
    snapshot_ema: f64,
    snapshots_this_sec: u64,
    sec_started: Option<f64>,
}

impl UiTimers {
    /// Each of these takes an already-measured duration rather than a closure
    /// so callers can borrow other fields of the app while doing the work.
    pub fn record_frame(&mut self, stats: &PerfStats, elapsed_us: u64) {
        stats.ui_frame.publish(&mut self.frame_ema, elapsed_us);
    }

    pub fn record_graph(&mut self, stats: &PerfStats, elapsed_us: u64) {
        stats.ui_graph.publish(&mut self.graph_ema, elapsed_us);
    }

    pub fn record_snapshot(&mut self, stats: &PerfStats, elapsed_us: u64) {
        stats.ui_snapshot.publish(&mut self.snapshot_ema, elapsed_us);
        self.snapshots_this_sec += 1;
    }

    /// Call once per frame with the egui context time and frame delta,
    /// whether or not a snapshot was taken.
    pub fn tick_frame(&mut self, stats: &PerfStats, now: f64, dt_secs: f32) {
        stats
            .frame_interval_us
            .store((dt_secs as f64 * 1_000_000.0) as u64, Ordering::Relaxed);
        let started = *self.sec_started.get_or_insert(now);
        if now - started >= 1.0 {
            stats
                .snapshots_per_sec
                .store(self.snapshots_this_sec, Ordering::Relaxed);
            self.snapshots_this_sec = 0;
            self.sec_started = Some(now);
        }
    }
}

// ---------------------------------------------------------------------------
// Per-node-type process() profiler
// ---------------------------------------------------------------------------

/// Opt-in breakdown of where engine tick time goes, by node type. Enable with
/// `LIGHTBEAT_PROFILE_NODES=1`; it costs two clock reads per node per tick, so
/// it is off unless asked for. Reports to stderr every few seconds.
pub struct NodeProfiler {
    totals: std::collections::HashMap<&'static str, (u64, u64)>,
    /// Nodes inside subgraphs. Kept apart because a subgraph's own time
    /// already includes them, so mixing the two would double count.
    inner_totals: std::collections::HashMap<&'static str, (u64, u64)>,
    ticks: u64,
    report_every: u64,
}

impl NodeProfiler {
    /// Returns `None` unless `LIGHTBEAT_PROFILE_NODES` is set.
    pub fn from_env() -> Option<Self> {
        std::env::var("LIGHTBEAT_PROFILE_NODES").ok().map(|_| Self {
            totals: std::collections::HashMap::new(),
            inner_totals: std::collections::HashMap::new(),
            ticks: 0,
            report_every: 2000,
        })
    }

    pub fn record(&mut self, type_name: &'static str, nanos: u64) {
        let e = self.totals.entry(type_name).or_insert((0, 0));
        e.0 += nanos;
        e.1 += 1;
    }

    /// Call once per tick after all nodes have been recorded.
    pub fn end_tick(&mut self) {
        INNER_SAMPLES.with(|samples| {
            for (name, nanos) in samples.borrow_mut().drain(..) {
                let e = self.inner_totals.entry(name).or_insert((0, 0));
                e.0 += nanos;
                e.1 += 1;
            }
        });
        self.ticks += 1;
        if self.ticks < self.report_every {
            return;
        }

        let ticks = self.ticks;
        report_table("node process()", &self.totals, ticks);
        if !self.inner_totals.is_empty() {
            report_table("nodes inside subgraphs", &self.inner_totals, ticks);
        }

        self.inner_totals.clear();
        self.totals.clear();
        self.ticks = 0;
    }
}

fn report_table(
    title: &str,
    totals: &std::collections::HashMap<&'static str, (u64, u64)>,
    ticks: u64,
) {
    let mut rows: Vec<(&'static str, u64, u64)> = totals
        .iter()
        .map(|(name, (ns, calls))| (*name, *ns, *calls))
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1));

    let total_ns: u64 = rows.iter().map(|r| r.1).sum();
    eprintln!(
        "\n--- {} over {} ticks: {:.1} us/tick total ---",
        title,
        ticks,
        total_ns as f64 / ticks as f64 / 1000.0
    );
    eprintln!(
        "{:<28} {:>6} {:>12} {:>11} {:>7}",
        "node type", "count", "us/tick", "us/call", "share"
    );
    for (name, ns, calls) in rows.iter().take(20) {
        eprintln!(
            "{:<28} {:>6.0} {:>12.1} {:>11.3} {:>6.1}%",
            name,
            *calls as f64 / ticks as f64,
            *ns as f64 / ticks as f64 / 1000.0,
            *ns as f64 / *calls as f64 / 1000.0,
            100.0 * *ns as f64 / total_ns.max(1) as f64
        );
    }
}

thread_local! {
    static INNER_SAMPLES: std::cell::RefCell<Vec<(&'static str, u64)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Whether `LIGHTBEAT_PROFILE_NODES` is set. Cached so subgraphs can test it
/// every tick without a syscall.
pub fn node_profiling_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("LIGHTBEAT_PROFILE_NODES").is_ok())
}

/// Called by subgraphs for each inner node; the engine's profiler collects
/// the samples at the end of the tick. Both run on the engine thread.
pub fn record_inner_node(type_name: &'static str, nanos: u64) {
    INNER_SAMPLES.with(|s| s.borrow_mut().push((type_name, nanos)));
}
