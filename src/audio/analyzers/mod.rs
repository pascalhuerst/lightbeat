//! Audio analyzer implementations.
//!
//! Architecture:
//!
//! - One worker thread per audio input (`worker::AnalyzerWorker`) runs every
//!   analyzer in lockstep on each incoming audio chunk. No cross-thread race
//!   between analyzers.
//! - Each analyzer implements `AnalyzerProc` — called only from the worker
//!   thread — plus exposes an `AnalyzerHandle` that the UI/engine use to
//!   read/set params (thread-safe via an internal `Arc<RwLock<…>>`).
//! - Analyzers declare their algorithmic `output_latency_samples()`. The
//!   worker maintains small per-analyzer FIFOs and holds the faster outputs
//!   back to match the slowest, so all values in an `AnalyzerFrame` refer to
//!   the same audio sample window. This is what makes onsets, envelope
//!   levels, and peak meters phase-coherent in the graph.

pub mod beat;
pub mod envelope;
pub mod onset;
pub mod peak_level;
pub mod worker;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::engine::types::{ParamDef, ParamValue, PortDef, PortType};

pub use worker::{AnalyzerFrame, AnalyzerWorker, SharedAnalyzerFrame};

/// Persistent identifier for an analyzer type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalyzerKind {
    Beat,
    Onset,
    PeakLevel,
    Envelope,
}

impl AnalyzerKind {
    pub const ALL: [Self; 4] = [Self::Beat, Self::Onset, Self::PeakLevel, Self::Envelope];

    pub fn label(&self) -> &'static str {
        match self {
            AnalyzerKind::Beat => "Beat (onset + BPM)",
            AnalyzerKind::Onset => "Onset (LP)",
            AnalyzerKind::PeakLevel => "Peak Level",
            AnalyzerKind::Envelope => "Envelope",
        }
    }

    /// Output port layout for a given analyzer kind. Used both for engine
    /// port declaration and for display slicing in the Audio Input node.
    pub fn outputs(&self) -> Vec<PortDef> {
        match self {
            AnalyzerKind::Beat => vec![
                PortDef::new("onset", PortType::Logic),
                PortDef::new("bpm", PortType::Untyped),
            ],
            AnalyzerKind::Onset => vec![
                PortDef::new("onset", PortType::Logic),
                PortDef::new("odf", PortType::Untyped),
            ],
            AnalyzerKind::PeakLevel => vec![
                PortDef::new("peak", PortType::Untyped),
                PortDef::new("rms", PortType::Untyped),
            ],
            AnalyzerKind::Envelope => vec![
                PortDef::new("envelope", PortType::Untyped),
            ],
        }
    }

    /// Default `ParamDef` list for this analyzer kind. Used by the UI when
    /// the kind is first added; once the analyzer is live, `AnalyzerHandle`
    /// reflects the actual (possibly-edited) param values.
    pub fn default_params(&self) -> Vec<ParamDef> {
        match self {
            AnalyzerKind::Beat => beat::beat_params(),
            AnalyzerKind::Onset => onset::onset_params(),
            AnalyzerKind::PeakLevel => peak_level::peak_params(),
            AnalyzerKind::Envelope => envelope::envelope_params(),
        }
    }
}

/// Persistent per-analyzer config (currently just kind; per-kind params are
/// edited live on the audio input node and not part of the saved setup).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzerConfig {
    pub kind: AnalyzerKind,
}

impl AnalyzerConfig {
    pub fn new(kind: AnalyzerKind) -> Self { Self { kind } }
}

/// Worker-side trait: only called from the analyzer worker thread. Each
/// analyzer implementation owns its DSP state here.
pub trait AnalyzerProc: Send {
    fn kind(&self) -> AnalyzerKind;
    fn num_outputs(&self) -> usize;
    /// When true, index 0 of this analyzer's outputs is a trigger (edge-
    /// detected downstream via `onset_count`).
    fn first_output_is_trigger(&self) -> bool { false }
    /// How many audio samples *behind* the latest chunk's end does the
    /// current output represent? Fast analyzers (envelope, peak meter)
    /// return 0; onset detectors return their picker lookback.
    fn output_latency_samples(&self) -> u32 { 0 }
    /// Process one chunk of mono f32 audio. Implementations update their
    /// internal state so that `outputs()` / `onset_count()` reflect the
    /// post-chunk result.
    fn step(&mut self, samples: &[f32]);
    /// Current analyzer output values, in declared port order.
    fn outputs(&self) -> Vec<f32>;
    /// Monotonic counter of onsets detected so far. Defaults to 0 for
    /// analyzers that don't produce a trigger output.
    fn onset_count(&self) -> u64 { 0 }
}

/// Thread-safe handle to an analyzer's parameters. Created alongside the
/// `AnalyzerProc`; lives on the manager / UI side. The closures capture the
/// analyzer's internal `Arc<RwLock<…>>` so the worker and handle share one
/// source of truth for params.
#[derive(Clone)]
pub struct AnalyzerHandle {
    pub kind: AnalyzerKind,
    get_params: Arc<dyn Fn() -> Vec<ParamDef> + Send + Sync>,
    set_param: Arc<dyn Fn(usize, ParamValue) + Send + Sync>,
}

impl AnalyzerHandle {
    pub fn new(
        kind: AnalyzerKind,
        get_params: Arc<dyn Fn() -> Vec<ParamDef> + Send + Sync>,
        set_param: Arc<dyn Fn(usize, ParamValue) + Send + Sync>,
    ) -> Self {
        Self { kind, get_params, set_param }
    }

    pub fn current_params(&self) -> Vec<ParamDef> { (self.get_params)() }
    pub fn set_param(&self, index: usize, value: ParamValue) { (self.set_param)(index, value) }
}

/// Factory: create the (handle, proc) pair for a given analyzer kind at the
/// given stream sample rate. Used by the worker when spawning analyzers.
pub fn create_analyzer(
    kind: AnalyzerKind,
    sample_rate: u32,
) -> (AnalyzerHandle, Box<dyn AnalyzerProc>) {
    match kind {
        AnalyzerKind::Beat => beat::create(sample_rate),
        AnalyzerKind::Onset => onset::create(sample_rate),
        AnalyzerKind::PeakLevel => peak_level::create(sample_rate),
        AnalyzerKind::Envelope => envelope::create(sample_rate),
    }
}
