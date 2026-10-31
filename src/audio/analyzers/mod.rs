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

pub mod beat_clock;
pub mod brightness;
pub mod buildup;
pub mod drums;
pub mod envelope;
pub mod fft;
pub mod key;
pub mod loudness;
pub mod peak_level;
pub mod presence;
pub mod spectrum;
pub mod worker;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::engine::types::{ParamDef, ParamValue, PortDef, PortType};

pub use worker::{AnalyzerWorker, SharedAnalyzerFrame};

/// Persistent identifier for an analyzer type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalyzerKind {
    /// Tempo, beat and bar position as phases; see `beat_clock`.
    BeatClock,
    PeakLevel,
    Envelope,
    /// Level in eight octave bands; see `spectrum`.
    Spectrum,
    /// Kick, snare and hat triggers; see `drums`.
    Drums,
    /// Where the energy sits between dark and bright; see `brightness`.
    Brightness,
    /// Breakdowns, build-ups and drops; see `buildup`.
    Buildup,
    /// Whether anything is playing; see `presence`.
    Presence,
    /// Perceived loudness (LUFS); see `loudness`.
    Loudness,
    /// Musical key; see `key`.
    Key,
}

impl AnalyzerKind {
    pub const ALL: [Self; 10] = [
        Self::BeatClock,
        Self::PeakLevel,
        Self::Envelope,
        Self::Spectrum,
        Self::Drums,
        Self::Brightness,
        Self::Buildup,
        Self::Presence,
        Self::Loudness,
        Self::Key,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            AnalyzerKind::BeatClock => "Beat clock (tempo, beat, bar)",
            AnalyzerKind::PeakLevel => "Peak Level",
            AnalyzerKind::Envelope => "Envelope",
            AnalyzerKind::Spectrum => "Spectrum (8 octave bands)",
            AnalyzerKind::Drums => "Drums (kick, snare, hat)",
            AnalyzerKind::Brightness => "Brightness",
            AnalyzerKind::Buildup => "Build-up and drop",
            AnalyzerKind::Presence => "Signal present",
            AnalyzerKind::Loudness => "Loudness (LUFS)",
            AnalyzerKind::Key => "Key",
        }
    }

    /// A few letters for labelling the node's output ports.
    pub fn short_label(&self) -> &'static str {
        match self {
            AnalyzerKind::BeatClock => "Clock",
            AnalyzerKind::PeakLevel => "Peak",
            AnalyzerKind::Envelope => "Env",
            AnalyzerKind::Spectrum => "Spectrum",
            AnalyzerKind::Drums => "Drums",
            AnalyzerKind::Brightness => "Bright",
            AnalyzerKind::Buildup => "Build-up",
            AnalyzerKind::Presence => "Signal",
            AnalyzerKind::Loudness => "Loud",
            AnalyzerKind::Key => "Key",
        }
    }

    /// Which of `outputs()` are triggers, driven by `trigger_counts` instead
    /// of by the output values.
    pub fn trigger_outputs(&self) -> &'static [usize] {
        match self {
            AnalyzerKind::BeatClock => &[0],
            AnalyzerKind::Drums => &[0, 1, 2],
            AnalyzerKind::Buildup => &[2],
            AnalyzerKind::Presence => &[1, 2],
            _ => &[],
        }
    }

    /// Output port layout for a given analyzer kind. Used both for engine
    /// port declaration and for display slicing in the Audio Input node.
    pub fn outputs(&self) -> Vec<PortDef> {
        match self {
            AnalyzerKind::BeatClock => vec![
                PortDef::new("beat", PortType::Logic),
                PortDef::new("beat phase", PortType::Phase),
                PortDef::new("bar phase", PortType::Phase),
                PortDef::new("16th phase", PortType::Phase),
                PortDef::new("step", PortType::Untyped),
                PortDef::new("bpm", PortType::Untyped),
                PortDef::new("confidence", PortType::Untyped),
            ],
            AnalyzerKind::PeakLevel => vec![
                PortDef::new("peak", PortType::Untyped),
                PortDef::new("rms", PortType::Untyped),
            ],
            AnalyzerKind::Envelope => vec![PortDef::new("envelope", PortType::Untyped)],
            AnalyzerKind::Drums => vec![
                PortDef::new("kick", PortType::Logic),
                PortDef::new("snare", PortType::Logic),
                PortDef::new("hat", PortType::Logic),
                PortDef::new("kick level", PortType::Untyped),
                PortDef::new("snare level", PortType::Untyped),
                PortDef::new("hat level", PortType::Untyped),
            ],
            AnalyzerKind::Brightness => vec![PortDef::new("brightness", PortType::Untyped)],
            AnalyzerKind::Buildup => vec![
                PortDef::new("tension", PortType::Untyped),
                PortDef::new("breakdown", PortType::Logic),
                PortDef::new("drop", PortType::Logic),
            ],
            AnalyzerKind::Presence => vec![
                PortDef::new("signal", PortType::Logic),
                PortDef::new("started", PortType::Logic),
                PortDef::new("stopped", PortType::Logic),
            ],
            AnalyzerKind::Loudness => vec![
                PortDef::new("momentary", PortType::Untyped),
                PortDef::new("short term", PortType::Untyped),
            ],
            AnalyzerKind::Key => vec![
                PortDef::new("key", PortType::Untyped),
                PortDef::new("major", PortType::Logic),
                PortDef::new("confidence", PortType::Untyped),
            ],
            AnalyzerKind::Spectrum => spectrum::BAND_CENTRES
                .iter()
                .map(|&c| {
                    let name = if c >= 1000.0 { format!("{} kHz", c / 1000.0) } else { format!("{c} Hz") };
                    PortDef::new(name, PortType::Untyped)
                })
                .collect(),
        }
    }
}

/// What to call analyzer `index` of `kinds` on the node's ports: its short
/// name, numbered when the same kind is added more than once.
pub fn port_prefix(kinds: &[AnalyzerKind], index: usize) -> String {
    let kind = kinds[index];
    let same = kinds.iter().filter(|k| **k == kind).count();
    if same > 1 {
        let nth = kinds[..=index].iter().filter(|k| **k == kind).count();
        format!("{} {nth}: ", kind.short_label())
    } else {
        format!("{}: ", kind.short_label())
    }
}

/// Persistent per-analyzer config (currently just kind; per-kind params are
/// edited live on the audio input node and not part of the saved setup).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzerConfig {
    pub kind: AnalyzerKind,
}

impl AnalyzerConfig {
    pub fn new(kind: AnalyzerKind) -> Self {
        Self { kind }
    }
}

/// Worker-side trait: only called from the analyzer worker thread. Each
/// analyzer implementation owns its DSP state here.
pub trait AnalyzerProc: Send {
    fn num_outputs(&self) -> usize;
    /// How many audio samples *behind* the latest chunk's end does the
    /// current output represent? Fast analyzers (envelope, peak meter)
    /// return 0; onset detectors return their picker lookback.
    fn output_latency_samples(&self) -> u32 {
        0
    }
    /// Process one chunk of mono f32 audio. Implementations update their
    /// internal state so that `outputs()` / `trigger_counts()` reflect the
    /// post-chunk result.
    fn step(&mut self, samples: &[f32]);
    /// Current analyzer output values, in declared port order.
    fn outputs(&self) -> Vec<f32>;
    /// Monotonic count of events so far for each trigger output, in the
    /// order of `AnalyzerKind::trigger_outputs`. Counting, rather than
    /// pulsing, means an event is never missed between two engine ticks.
    fn trigger_counts(&self) -> Vec<u64> {
        Vec::new()
    }
}

/// Thread-safe handle to an analyzer's parameters. Created alongside the
/// `AnalyzerProc`; lives on the manager / UI side. The closures capture the
/// analyzer's internal `Arc<RwLock<…>>` so the worker and handle share one
/// source of truth for params.
#[derive(Clone)]
pub struct AnalyzerHandle {
    get_params: Arc<dyn Fn() -> Vec<ParamDef> + Send + Sync>,
    set_param: Arc<dyn Fn(usize, ParamValue) + Send + Sync>,
}

impl AnalyzerHandle {
    pub fn new(
        get_params: Arc<dyn Fn() -> Vec<ParamDef> + Send + Sync>,
        set_param: Arc<dyn Fn(usize, ParamValue) + Send + Sync>,
    ) -> Self {
        Self {
            get_params,
            set_param,
        }
    }

    pub fn current_params(&self) -> Vec<ParamDef> {
        (self.get_params)()
    }
    pub fn set_param(&self, index: usize, value: ParamValue) {
        (self.set_param)(index, value)
    }
}

/// A handle onto parameters kept in a shared struct: `describe` lists them
/// for the UI and `apply` sets one by index.
pub fn handle_for<P: Send + Sync + 'static>(
    params: Arc<parking_lot::RwLock<P>>,
    describe: fn(&P) -> Vec<ParamDef>,
    apply: fn(&mut P, usize, ParamValue),
) -> AnalyzerHandle {
    let get = params.clone();
    AnalyzerHandle::new(
        Arc::new(move || describe(&get.read())),
        Arc::new(move |index, value| apply(&mut params.write(), index, value)),
    )
}

/// Factory: create the (handle, proc) pair for a given analyzer kind at the
/// given stream sample rate. Used by the worker when spawning analyzers.
pub fn create_analyzer(
    kind: AnalyzerKind,
    sample_rate: u32,
) -> (AnalyzerHandle, Box<dyn AnalyzerProc>) {
    match kind {
        AnalyzerKind::BeatClock => beat_clock::create(sample_rate),
        AnalyzerKind::PeakLevel => peak_level::create(sample_rate),
        AnalyzerKind::Envelope => envelope::create(sample_rate),
        AnalyzerKind::Spectrum => spectrum::create(sample_rate),
        AnalyzerKind::Drums => drums::create(sample_rate),
        AnalyzerKind::Brightness => brightness::create(sample_rate),
        AnalyzerKind::Buildup => buildup::create(sample_rate),
        AnalyzerKind::Presence => presence::create(sample_rate),
        AnalyzerKind::Loudness => loudness::create(sample_rate),
        AnalyzerKind::Key => key::create(sample_rate),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Saved projects wire to analyzer outputs by index, so the layout is a
    /// contract. Moving these ports means migrating the saved wires.
    #[test]
    fn beat_clock_port_layout_is_stable() {
        let names: Vec<String> = AnalyzerKind::BeatClock.outputs().into_iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            ["beat", "beat phase", "bar phase", "16th phase", "step", "bpm", "confidence"]
        );
        assert_eq!(create_analyzer(AnalyzerKind::BeatClock, 44_100).0.current_params().len(), 4);
    }

    #[test]
    fn every_analyzer_declares_as_many_outputs_as_it_produces() {
        for kind in AnalyzerKind::ALL {
            let (_handle, proc) = create_analyzer(kind, 44_100);
            assert_eq!(proc.num_outputs(), kind.outputs().len(), "{kind:?}");
            assert_eq!(proc.outputs().len(), kind.outputs().len(), "{kind:?}");
        }
    }
}
