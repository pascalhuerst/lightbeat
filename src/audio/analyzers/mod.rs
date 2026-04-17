//! Audio analyzer implementations.
//!
//! Each analyzer:
//! - Spawns a worker thread reading f32 audio chunks from a crossbeam
//!   subscriber.
//! - Owns a small `Shared*Output` block that the engine reads each tick.
//! - Owns a `SharedParams` block that the UI/inspector mutates.
//!
//! Analyzer kinds are identified by `AnalyzerKind`. New analyzers are added
//! by extending the enum and `AnalyzerInstance`.

pub mod beat;
pub mod envelope;
pub mod peak_level;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::engine::types::{ParamDef, ParamValue, PortDef, PortType};

/// Persistent identifier for an analyzer type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalyzerKind {
    Beat,
    PeakLevel,
    Envelope,
}

impl AnalyzerKind {
    pub const ALL: [Self; 3] = [Self::Beat, Self::PeakLevel, Self::Envelope];

    pub fn label(&self) -> &'static str {
        match self {
            AnalyzerKind::Beat => "Beat",
            AnalyzerKind::PeakLevel => "Peak Level",
            AnalyzerKind::Envelope => "Envelope",
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

/// One running analyzer instance.
pub struct AnalyzerInstance {
    pub kind: AnalyzerKind,
    /// Implementation-specific live state.
    pub state: AnalyzerState,
}

pub enum AnalyzerState {
    Beat(beat::BeatAnalyzer),
    PeakLevel(peak_level::PeakLevelAnalyzer),
    Envelope(envelope::EnvelopeAnalyzer),
}

impl AnalyzerInstance {
    pub fn outputs_for_kind(kind: AnalyzerKind) -> Vec<PortDef> {
        match kind {
            AnalyzerKind::Beat => vec![
                PortDef::new("onset", PortType::Logic),
                PortDef::new("bpm", PortType::Untyped),
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

    pub fn params_for_kind(kind: AnalyzerKind) -> Vec<ParamDef> {
        match kind {
            AnalyzerKind::Beat => beat::beat_params(),
            AnalyzerKind::PeakLevel => peak_level::peak_params(),
            AnalyzerKind::Envelope => envelope::envelope_params(),
        }
    }

    /// Read the current output values (one per output port).
    pub fn read_outputs(&self) -> Vec<f32> {
        match &self.state {
            AnalyzerState::Beat(b) => b.read_outputs(),
            AnalyzerState::PeakLevel(p) => p.read_outputs(),
            AnalyzerState::Envelope(e) => e.read_outputs(),
        }
    }

    /// `read_onset_count` style trigger detection — engine compares to its
    /// previous tick's value to fire one-tick output edges. Returns 0 for
    /// analyzer kinds that don't produce a trigger output at index 0.
    pub fn onset_count(&self) -> u64 {
        match &self.state {
            AnalyzerState::Beat(b) => b.onset_count(),
            AnalyzerState::PeakLevel(_) => 0,
            AnalyzerState::Envelope(_) => 0,
        }
    }

    /// True if output index 0 is a trigger that should be edge-detected from
    /// `onset_count`. False for purely continuous analyzers.
    pub fn first_output_is_trigger(&self) -> bool {
        matches!(self.state, AnalyzerState::Beat(_))
    }

    pub fn set_param(&self, index: usize, value: ParamValue) {
        match &self.state {
            AnalyzerState::Beat(b) => b.set_param(index, value),
            AnalyzerState::PeakLevel(p) => p.set_param(index, value),
            AnalyzerState::Envelope(e) => e.set_param(index, value),
        }
    }

    pub fn current_params(&self) -> Vec<ParamDef> {
        match &self.state {
            AnalyzerState::Beat(b) => b.current_params(),
            AnalyzerState::PeakLevel(p) => p.current_params(),
            AnalyzerState::Envelope(e) => e.current_params(),
        }
    }
}

/// Spawn an analyzer of the given kind, subscribed to the audio chunk stream.
pub fn spawn_analyzer(
    kind: AnalyzerKind,
    rx: crossbeam_channel::Receiver<crate::audio::device::AudioChunk>,
    sample_rate: u32,
) -> AnalyzerInstance {
    match kind {
        AnalyzerKind::Beat => {
            let analyzer = beat::BeatAnalyzer::spawn(rx, sample_rate);
            AnalyzerInstance { kind, state: AnalyzerState::Beat(analyzer) }
        }
        AnalyzerKind::PeakLevel => {
            let analyzer = peak_level::PeakLevelAnalyzer::spawn(rx, sample_rate);
            AnalyzerInstance { kind, state: AnalyzerState::PeakLevel(analyzer) }
        }
        AnalyzerKind::Envelope => {
            let analyzer = envelope::EnvelopeAnalyzer::spawn(rx, sample_rate);
            AnalyzerInstance { kind, state: AnalyzerState::Envelope(analyzer) }
        }
    }
}

/// Holder used by the manager so each analyzer can be dropped & recreated when
/// the audio input is reopened (e.g. after a sample-rate change).
pub type SharedAudioChunkRx = Arc<crossbeam_channel::Receiver<crate::audio::device::AudioChunk>>;
