//! Onset detection (Glover/Lazzarini/Timoney 2011, EURASIP JASP 2011:68).
//!
//! Seven ODFs behind a common trait plus an adaptive peak picker. The
//! per-algorithm sub-modules (`frame`, `odf`, `peak`, `burg`, `sinusoidal`)
//! are the streaming reference implementation; this file wraps them as an
//! `AnalyzerProc` so they plug into the same worker thread as the other
//! analyzers.
//!
//! Outputs: `onset` (Logic trigger, edge-detected from the onset counter)
//! and `odf` (Untyped, the raw detection-function value — useful for
//! visualising what the algorithm is reacting to).

mod burg;
mod frame;
mod odf;
mod peak;
mod sinusoidal;

use std::sync::Arc;

use parking_lot::RwLock;

use self::frame::FrameBuffer;
use self::odf::{
    ComplexDomainLpOdf, ComplexDomainOdf, EnergyLpOdf, EnergyOdf, Odf as OdfTrait,
    PeakAmplitudeDifferenceOdf, SpectralDifferenceLpOdf, SpectralDifferenceOdf,
};
use self::peak::{PeakPicker, PeakPickerConfig};

use crate::audio::analyzers::{AnalyzerHandle, AnalyzerKind, AnalyzerProc};
use crate::engine::types::{ParamDef, ParamValue};

/// Paper defaults (§5.2). Buffer = hop size = 512 samples, frame = 2048.
/// Latency of the peak picker is ~1 buffer (= 512 samples); PAD adds nothing
/// in this streaming approximation.
const BUFFER_SIZE: usize = 512;
const FRAME_SIZE: usize = 2048;
const LP_ORDER: usize = 5;
const PAD_MAX_PEAKS: usize = 20;
const PAD_MATCH_TOL_BINS: f64 = 2.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnsetMethod {
    /// Frame-to-frame energy delta. Cheapest, OK for monophonic signals.
    Energy,
    /// Magnitude-spectrum change per FFT bin.
    SpectralDifference,
    /// Magnitude + phase prediction error.
    ComplexDomain,
    /// Energy + Burg linear prediction.
    EnergyLp,
    /// Spectral difference + Burg LP per bin (paper's highest-accuracy pick).
    SpectralDifferenceLp,
    /// Complex-phasor distance + Burg LP per bin.
    ComplexDomainLp,
    /// Peak amplitude difference (sinusoidal modelling).
    PeakAmplitudeDifference,
}

impl OnsetMethod {
    pub const ALL: [Self; 7] = [
        Self::Energy,
        Self::SpectralDifference,
        Self::ComplexDomain,
        Self::EnergyLp,
        Self::SpectralDifferenceLp,
        Self::ComplexDomainLp,
        Self::PeakAmplitudeDifference,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            Self::Energy => "Energy",
            Self::SpectralDifference => "Spectral difference",
            Self::ComplexDomain => "Complex domain",
            Self::EnergyLp => "Energy + LP",
            Self::SpectralDifferenceLp => "Spectral diff + LP",
            Self::ComplexDomainLp => "Complex + LP",
            Self::PeakAmplitudeDifference => "Peak amp diff",
        }
    }
    pub fn to_index(&self) -> usize {
        Self::ALL.iter().position(|m| m == self).unwrap_or(0)
    }
    pub fn from_index(i: usize) -> Self {
        Self::ALL.get(i).copied().unwrap_or(Self::SpectralDifferenceLp)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OnsetParams {
    pub method: OnsetMethod,
    /// Median weight λ in the adaptive threshold.
    pub lambda: f32,
    /// Mean weight α in the adaptive threshold.
    pub alpha: f32,
    /// Minimum inter-onset interval (ms). Onsets closer than this are
    /// suppressed — our addition on top of the paper's picker.
    pub min_ioi_ms: f32,
}

impl Default for OnsetParams {
    fn default() -> Self {
        // Tuned for 4-on-the-floor electronic dance music at 110–145 BPM
        // (= 414–545 ms between kicks): EnergyLp reacts to amplitude
        // transients and is largely blind to pad/hi-hat spectral movement;
        // 300 ms min-IOI swallows hi-hats and 16th-note fills while letting
        // every quarter-note kick through.
        Self {
            method: OnsetMethod::EnergyLp,
            lambda: 1.0,
            alpha: 2.0,
            min_ioi_ms: 300.0,
        }
    }
}

pub fn onset_params() -> Vec<ParamDef> {
    params_for(&OnsetParams::default())
}

fn params_for(p: &OnsetParams) -> Vec<ParamDef> {
    vec![
        ParamDef::Choice {
            name: "method".into(),
            value: p.method.to_index(),
            options: OnsetMethod::ALL.iter().map(|m| m.label().to_string()).collect(),
        },
        ParamDef::Float {
            name: "lambda".into(),
            value: p.lambda, min: 0.0, max: 4.0, step: 0.05, unit: "",
        },
        ParamDef::Float {
            name: "alpha".into(),
            value: p.alpha, min: 0.0, max: 4.0, step: 0.05, unit: "",
        },
        ParamDef::Float {
            name: "min_ioi_ms".into(),
            value: p.min_ioi_ms, min: 1.0, max: 500.0, step: 1.0, unit: "ms",
        },
    ]
}

pub type SharedOnsetParams = Arc<RwLock<OnsetParams>>;

fn apply_param(params: &SharedOnsetParams, index: usize, value: ParamValue) {
    let mut p = params.write();
    match index {
        0 => p.method = OnsetMethod::from_index(value.as_i64() as usize),
        1 => p.lambda = value.as_f32().max(0.0),
        2 => p.alpha = value.as_f32().max(0.0),
        3 => p.min_ioi_ms = value.as_f32().max(1.0),
        _ => {}
    }
}

fn make_picker(p: &OnsetParams) -> PeakPicker {
    PeakPicker::with_config(PeakPickerConfig {
        lambda: p.lambda as f64,
        alpha: p.alpha as f64,
        ..PeakPickerConfig::default()
    })
}

fn build_odf(method: OnsetMethod) -> Box<dyn OdfTrait + Send> {
    match method {
        OnsetMethod::Energy => Box::new(EnergyOdf::new(FRAME_SIZE)),
        OnsetMethod::SpectralDifference => Box::new(SpectralDifferenceOdf::new(FRAME_SIZE)),
        OnsetMethod::ComplexDomain => Box::new(ComplexDomainOdf::new(FRAME_SIZE)),
        OnsetMethod::EnergyLp => Box::new(EnergyLpOdf::new(FRAME_SIZE, LP_ORDER)),
        OnsetMethod::SpectralDifferenceLp => Box::new(SpectralDifferenceLpOdf::new(FRAME_SIZE, LP_ORDER)),
        OnsetMethod::ComplexDomainLp => Box::new(ComplexDomainLpOdf::new(FRAME_SIZE, LP_ORDER)),
        OnsetMethod::PeakAmplitudeDifference => Box::new(PeakAmplitudeDifferenceOdf::new(
            FRAME_SIZE, PAD_MAX_PEAKS, PAD_MATCH_TOL_BINS,
        )),
    }
}

pub struct OnsetProc {
    params: SharedOnsetParams,
    current: OnsetParams,
    sample_rate: u32,
    /// Sliding 2048-sample frame; rebuilt when method changes.
    frames: FrameBuffer,
    /// Active detection function; rebuilt when method changes.
    odf: Box<dyn OdfTrait + Send>,
    picker: PeakPicker,
    /// Accumulator for audio samples until we have a full buffer_size chunk
    /// to hand to `FrameBuffer`. Avoids allocating on every step.
    accumulator: Vec<f32>,
    /// Monotonic counter of picker steps (what we pass as `index` to the
    /// picker). Used both for the picker's internal coordinate and to
    /// enforce min_ioi_ms.
    picker_index: u64,
    /// Picker-index of the last onset emitted (so we can suppress close
    /// successors).
    last_onset_picker_index: Option<u64>,
    /// Latest raw ODF value (exposed as output for debugging/visualisation).
    latest_odf: f32,
    /// Monotonic onset counter (drives the engine's trigger edge detection).
    onset_counter: u64,
}

impl AnalyzerProc for OnsetProc {
    fn kind(&self) -> AnalyzerKind { AnalyzerKind::Onset }
    fn num_outputs(&self) -> usize { 2 }
    fn first_output_is_trigger(&self) -> bool { true }
    /// Peak picker has 1-buffer lookback; all ODFs (including the streaming
    /// PAD approximation) share this latency in our impl.
    fn output_latency_samples(&self) -> u32 { BUFFER_SIZE as u32 }

    fn step(&mut self, samples: &[f32]) {
        // Rebuild the ODF when the user switches method. Threshold weight
        // changes (lambda/alpha) also require a picker rebuild because
        // PeakPickerConfig is set at construction; we preserve nothing from
        // the old picker, but its short median history fills back up in a
        // few frames.
        let p = *self.params.read();
        if p.method != self.current.method {
            self.current.method = p.method;
            self.odf = build_odf(p.method);
            self.frames = FrameBuffer::new(BUFFER_SIZE, FRAME_SIZE);
            self.picker = make_picker(&p);
            self.picker_index = 0;
            self.last_onset_picker_index = None;
        }
        if p.lambda != self.current.lambda || p.alpha != self.current.alpha {
            self.picker = make_picker(&p);
        }
        self.current = p;
        let min_ioi_frames = ((p.min_ioi_ms.max(1.0) as f64) / 1000.0
            * (self.sample_rate as f64 / BUFFER_SIZE as f64)) as u64;

        // Accumulate and drain in BUFFER_SIZE chunks.
        self.accumulator.extend_from_slice(samples);
        while self.accumulator.len() >= BUFFER_SIZE {
            let chunk: Vec<f32> = self.accumulator.drain(..BUFFER_SIZE).collect();
            let Some(frame) = self.frames.push(&chunk) else { continue; };
            let v = self.odf.process(frame);
            self.latest_odf = v as f32;
            let idx = self.picker_index;
            self.picker_index += 1;
            if let Some(onset_idx) = self.picker.push(v, idx) {
                let suppress = self.last_onset_picker_index
                    .is_some_and(|last| onset_idx.saturating_sub(last) < min_ioi_frames);
                if !suppress {
                    self.last_onset_picker_index = Some(onset_idx);
                    self.onset_counter += 1;
                }
            }
        }
    }

    fn outputs(&self) -> Vec<f32> {
        // Index 0 is the trigger slot; the engine edge-detects from
        // `onset_count()`. Index 1 carries the live ODF value so a Scope
        // can show the detection function alongside the onset markers.
        vec![0.0, self.latest_odf]
    }

    fn onset_count(&self) -> u64 { self.onset_counter }
}

pub fn create(sample_rate: u32) -> (AnalyzerHandle, Box<dyn AnalyzerProc>) {
    let params = Arc::new(RwLock::new(OnsetParams::default()));
    let current = *params.read();

    let proc = Box::new(OnsetProc {
        params: params.clone(),
        current,
        sample_rate,
        frames: FrameBuffer::new(BUFFER_SIZE, FRAME_SIZE),
        odf: build_odf(current.method),
        picker: make_picker(&current),
        accumulator: Vec::with_capacity(BUFFER_SIZE * 2),
        picker_index: 0,
        last_onset_picker_index: None,
        latest_odf: 0.0,
        onset_counter: 0,
    });

    let p_get = params.clone();
    let get_params = Arc::new(move || params_for(&*p_get.read()))
        as Arc<dyn Fn() -> Vec<ParamDef> + Send + Sync>;
    let p_set = params;
    let set_param = Arc::new(move |idx: usize, v: ParamValue| apply_param(&p_set, idx, v))
        as Arc<dyn Fn(usize, ParamValue) + Send + Sync>;
    (AnalyzerHandle::new(AnalyzerKind::Onset, get_params, set_param), proc)
}
