//! Onset + BPM detector. Ported from `beat_detection_tests/src/detectors/neo_beat_detection.rs`.
//!
//! Outputs (in order): `onset` (Logic, edge-detected from a u64 counter) and
//! `bpm` (Untyped, latest BPM estimate).

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use crossbeam_channel::Receiver;
use num_complex::Complex32;
use parking_lot::RwLock;
use realfft::{RealFftPlanner, RealToComplex};
use rustfft::{Fft as RFft, FftPlanner as RFftPlanner};

use crate::audio::backend::AudioChunk;
use crate::engine::types::{ParamDef, ParamValue};

const ANALYSIS_WINDOW: usize = 1024;
const HOP_SIZE: usize = 512;

const FB_BANDS_PER_OCTAVE: usize = 24;
const FB_F_MIN_HZ: f32 = 30.0;
const FB_F_MAX_HZ: f32 = 17_000.0;
const SF_MAX_FREQ_RADIUS: usize = 1;
const SF_TIME_LAG: usize = 3;
const LOG_MAG_C: f32 = 1000.0;
const BELLO_MEDIAN_WINDOW_S: f64 = 0.10;
const CGD_WINDOW_S: f64 = 2.0;
const CGD_UPDATE_HZ: f64 = 10.0;
const CGD_RADIUS: f32 = 1.05;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeoOdfMethod { SuperFlux, ComplexDomain, RectifiedComplex, SpectralFlux }
impl NeoOdfMethod {
    pub const ALL: [Self; 4] = [Self::SuperFlux, Self::ComplexDomain, Self::RectifiedComplex, Self::SpectralFlux];
    pub fn label(&self) -> &'static str { match self {
        Self::SuperFlux => "SuperFlux", Self::ComplexDomain => "Complex domain",
        Self::RectifiedComplex => "Rectified complex", Self::SpectralFlux => "Spectral flux",
    }}
    pub fn to_index(&self) -> usize { Self::ALL.iter().position(|m| m == self).unwrap_or(0) }
    pub fn from_index(i: usize) -> Self { Self::ALL.get(i).copied().unwrap_or(Self::SuperFlux) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeoPicker { Bello, Vpd }
impl NeoPicker {
    pub const ALL: [Self; 2] = [Self::Bello, Self::Vpd];
    pub fn label(&self) -> &'static str { match self {
        Self::Bello => "Bello (median)", Self::Vpd => "Valley–peak distance",
    }}
    pub fn to_index(&self) -> usize { Self::ALL.iter().position(|m| m == self).unwrap_or(0) }
    pub fn from_index(i: usize) -> Self { Self::ALL.get(i).copied().unwrap_or(Self::Bello) }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeatParams {
    pub odf: NeoOdfMethod,
    pub picker: NeoPicker,
    pub cgd_smooth: bool,
    pub silence_db: f32,
    pub threshold: f32,
    pub min_ioi_ms: f32,
}

impl Default for BeatParams {
    fn default() -> Self {
        Self {
            odf: NeoOdfMethod::SuperFlux,
            picker: NeoPicker::Bello,
            cgd_smooth: false,
            silence_db: -40.0,
            threshold: 0.3,
            min_ioi_ms: 50.0,
        }
    }
}

/// Engine-side `ParamDef` definitions for beat analyzer params.
pub fn beat_params() -> Vec<ParamDef> {
    let d = BeatParams::default();
    params_for(&d)
}

fn params_for(p: &BeatParams) -> Vec<ParamDef> {
    vec![
        ParamDef::Choice {
            name: "odf".into(),
            value: p.odf.to_index(),
            options: NeoOdfMethod::ALL.iter().map(|m| m.label().to_string()).collect(),
        },
        ParamDef::Choice {
            name: "picker".into(),
            value: p.picker.to_index(),
            options: NeoPicker::ALL.iter().map(|m| m.label().to_string()).collect(),
        },
        ParamDef::Bool { name: "cgd_smooth".into(), value: p.cgd_smooth },
        ParamDef::Float { name: "silence_db".into(), value: p.silence_db, min: -80.0, max: 0.0, step: 1.0, unit: "dB" },
        ParamDef::Float { name: "threshold".into(), value: p.threshold, min: 0.0, max: 1.0, step: 0.01, unit: "" },
        ParamDef::Float { name: "min_ioi_ms".into(), value: p.min_ioi_ms, min: 1.0, max: 500.0, step: 1.0, unit: "ms" },
    ]
}

pub type SharedBeatParams = Arc<RwLock<BeatParams>>;

/// Live outputs published by the worker.
pub struct BeatOutputs {
    pub onset_count: AtomicU64,
    pub bpm: parking_lot::Mutex<f32>,
}

pub struct BeatAnalyzer {
    pub params: SharedBeatParams,
    pub outputs: Arc<BeatOutputs>,
    _join: Option<thread::JoinHandle<()>>,
}

impl BeatAnalyzer {
    pub fn spawn(rx: Receiver<AudioChunk>, sample_rate: u32) -> Self {
        let params = Arc::new(RwLock::new(BeatParams::default()));
        let outputs = Arc::new(BeatOutputs {
            onset_count: AtomicU64::new(0),
            bpm: parking_lot::Mutex::new(0.0),
        });
        let p_clone = params.clone();
        let o_clone = outputs.clone();
        let join = thread::Builder::new()
            .name("lightbeat-beat-analyzer".into())
            .spawn(move || run(rx, sample_rate, p_clone, o_clone))
            .expect("spawn beat analyzer thread");
        Self { params, outputs, _join: Some(join) }
    }

    pub fn read_outputs(&self) -> Vec<f32> {
        // onset (index 0) is computed by the engine via edge detection on the
        // shared onset_count; we publish 0.0 here as a placeholder.
        let bpm = *self.outputs.bpm.lock();
        vec![0.0, bpm]
    }

    pub fn onset_count(&self) -> u64 {
        self.outputs.onset_count.load(Ordering::Relaxed)
    }

    pub fn current_params(&self) -> Vec<ParamDef> {
        let p = *self.params.read();
        params_for(&p)
    }

    pub fn set_param(&self, index: usize, value: ParamValue) {
        let mut p = self.params.write();
        match index {
            0 => p.odf = NeoOdfMethod::from_index(value.as_i64() as usize),
            1 => p.picker = NeoPicker::from_index(value.as_i64() as usize),
            2 => p.cgd_smooth = matches!(value, ParamValue::Bool(true)),
            3 => p.silence_db = value.as_f32(),
            4 => p.threshold = value.as_f32(),
            5 => p.min_ioi_ms = value.as_f32(),
            _ => {}
        }
    }
}

// ---------- worker -----------------------------------------------------------

fn run(rx: Receiver<AudioChunk>, sample_rate: u32, params: SharedBeatParams, outputs: Arc<BeatOutputs>) {
    let mut stft = Stft::new(ANALYSIS_WINDOW, HOP_SIZE);
    let odf_rate = sample_rate as f64 / HOP_SIZE as f64;
    let num_bins = ANALYSIS_WINDOW / 2 + 1;
    let filterbank = LogFilterbank::new(num_bins, sample_rate, FB_F_MIN_HZ, FB_F_MAX_HZ, FB_BANDS_PER_OCTAVE);

    let mut current = *params.read();
    let mut odf = Odf::new(current.odf, filterbank.num_bands(), num_bins);
    let mut picker = Picker::new(current.picker, odf_rate, &current);
    let mut tempo = TempoTracker::new(odf_rate);
    let mut cgd = if current.cgd_smooth && cgd_applicable(current.odf) {
        Some(CgdSmoother::new(odf_rate))
    } else { None };

    while let Ok(chunk) = rx.recv() {
        let p = *params.read();
        if p != current {
            if p.odf != current.odf {
                odf = Odf::new(p.odf, filterbank.num_bands(), num_bins);
                tempo = TempoTracker::new(odf_rate);
            }
            if p.picker != current.picker {
                picker = Picker::new(p.picker, odf_rate, &p);
            } else {
                picker.apply_params(odf_rate, &p);
            }
            let want_cgd = p.cgd_smooth && cgd_applicable(p.odf);
            if want_cgd != cgd.is_some() {
                cgd = if want_cgd { Some(CgdSmoother::new(odf_rate)) } else { None };
            }
            current = p;
        }

        stft.push(&chunk.mono_f32, |_frame_idx, spectrum, frame_rms| {
            let log_bands = filterbank.apply_log(spectrum);
            let flux = odf.step(spectrum, &log_bands);
            let smoothed = match cgd.as_mut() {
                Some(s) => s.push_and_peek(flux),
                None => flux,
            };
            tempo.push(smoothed);
            let frame_db = 20.0 * frame_rms.max(1e-9).log10();
            let gated = frame_db < current.silence_db;
            if let Some(_lookback) = picker.step(smoothed, gated) {
                outputs.onset_count.fetch_add(1, Ordering::Relaxed);
            }
            if let Some(bpm) = tempo.estimate() {
                *outputs.bpm.lock() = bpm as f32;
            }
        });
    }
}

fn cgd_applicable(odf: NeoOdfMethod) -> bool { !matches!(odf, NeoOdfMethod::SuperFlux) }

// ---------- STFT ------------------------------------------------------------

struct Stft {
    fft_size: usize,
    hop_size: usize,
    window: Vec<f32>,
    buffer: Vec<f32>,
    plan: Arc<dyn RealToComplex<f32>>,
    scratch: Vec<Complex32>,
    spectrum: Vec<Complex32>,
    frame_in: Vec<f32>,
    frame_idx: u64,
}

impl Stft {
    fn new(fft_size: usize, hop_size: usize) -> Self {
        let window = hann_window(fft_size);
        let mut planner = RealFftPlanner::<f32>::new();
        let plan = planner.plan_fft_forward(fft_size);
        let scratch = plan.make_scratch_vec();
        let spectrum = plan.make_output_vec();
        Self {
            fft_size, hop_size, window,
            buffer: Vec::with_capacity(fft_size * 4),
            plan, scratch, spectrum,
            frame_in: vec![0.0; fft_size],
            frame_idx: 0,
        }
    }
    fn push(&mut self, samples: &[f32], mut on_frame: impl FnMut(u64, &[Complex32], f32)) {
        self.buffer.extend_from_slice(samples);
        while self.buffer.len() >= self.fft_size {
            let mut sqsum = 0.0f32;
            for i in 0..self.fft_size {
                let s = self.buffer[i];
                sqsum += s * s;
                self.frame_in[i] = s * self.window[i];
            }
            let rms = (sqsum / self.fft_size as f32).sqrt();
            self.plan
                .process_with_scratch(&mut self.frame_in, &mut self.spectrum, &mut self.scratch)
                .expect("realfft process");
            on_frame(self.frame_idx, &self.spectrum, rms);
            self.frame_idx += 1;
            self.buffer.drain(..self.hop_size);
        }
    }
}

fn hann_window(n: usize) -> Vec<f32> {
    (0..n).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (n as f32 - 1.0)).cos()).collect()
}

// ---------- log-frequency triangular filterbank ------------------------------

struct LogBand { start_bin: usize, weights: Vec<f32> }
struct LogFilterbank { bands: Vec<LogBand> }

impl LogFilterbank {
    fn new(num_bins: usize, sample_rate: u32, f_min: f32, f_max: f32, bands_per_octave: usize) -> Self {
        let fft_size = (num_bins - 1) * 2;
        let bin_hz = sample_rate as f32 / fft_size as f32;
        let f_max = f_max.min((sample_rate as f32) * 0.5 - bin_hz);
        let num_octaves = (f_max / f_min).log2().max(0.0);
        let num_bands = (num_octaves * bands_per_octave as f32).ceil() as usize;
        let step = 2.0f32.powf(1.0 / bands_per_octave as f32);
        let mut bands = Vec::with_capacity(num_bands);
        for i in 0..num_bands {
            let center = f_min * step.powi(i as i32);
            let low = center / step;
            let high = (center * step).min(f_max);
            let start_bin = (low / bin_hz).floor().max(1.0) as usize;
            let end_bin = ((high / bin_hz).ceil() as usize + 1).min(num_bins);
            if end_bin <= start_bin + 1 {
                bands.push(LogBand { start_bin, weights: vec![1.0] });
                continue;
            }
            let mut weights = Vec::with_capacity(end_bin - start_bin);
            for k in start_bin..end_bin {
                let f = k as f32 * bin_hz;
                let w = if f <= center {
                    (f - low) / (center - low).max(1e-6)
                } else {
                    (high - f) / (high - center).max(1e-6)
                };
                weights.push(w.clamp(0.0, 1.0));
            }
            bands.push(LogBand { start_bin, weights });
        }
        Self { bands }
    }
    fn num_bands(&self) -> usize { self.bands.len() }
    fn apply_log(&self, spectrum: &[Complex32]) -> Vec<f32> {
        self.bands.iter().map(|band| {
            let mut s = 0.0f32;
            for (i, &w) in band.weights.iter().enumerate() {
                let idx = band.start_bin + i;
                if idx < spectrum.len() {
                    s += w * spectrum[idx].norm();
                }
            }
            (1.0 + LOG_MAG_C * s).ln()
        }).collect()
    }
}

// ---------- onset detection functions ---------------------------------------

enum Odf {
    SuperFlux { history: VecDeque<Vec<f32>> },
    ComplexDomain {
        prev_mag: Vec<f32>,
        prev_phase: Vec<f32>,
        prev_prev_phase: Vec<f32>,
        rectified: bool,
        frames_seen: u32,
    },
    SpectralFlux { prev_mag: Vec<f32> },
}

impl Odf {
    fn new(method: NeoOdfMethod, _num_bands: usize, num_bins: usize) -> Self {
        match method {
            NeoOdfMethod::SuperFlux => Self::SuperFlux { history: VecDeque::with_capacity(SF_TIME_LAG + 1) },
            NeoOdfMethod::ComplexDomain => Self::ComplexDomain {
                prev_mag: vec![0.0; num_bins], prev_phase: vec![0.0; num_bins],
                prev_prev_phase: vec![0.0; num_bins], rectified: false, frames_seen: 0,
            },
            NeoOdfMethod::RectifiedComplex => Self::ComplexDomain {
                prev_mag: vec![0.0; num_bins], prev_phase: vec![0.0; num_bins],
                prev_prev_phase: vec![0.0; num_bins], rectified: true, frames_seen: 0,
            },
            NeoOdfMethod::SpectralFlux => Self::SpectralFlux { prev_mag: vec![0.0; num_bins] },
        }
    }
    fn step(&mut self, spectrum: &[Complex32], log_bands: &[f32]) -> f32 {
        match self {
            Self::SuperFlux { history } => {
                let n = log_bands.len();
                let r = SF_MAX_FREQ_RADIUS;
                let mut maxed = vec![f32::NEG_INFINITY; n];
                for k in 0..n {
                    let lo = k.saturating_sub(r);
                    let hi = (k + r + 1).min(n);
                    for &v in &log_bands[lo..hi] {
                        if v > maxed[k] { maxed[k] = v; }
                    }
                }
                let flux = if history.len() >= SF_TIME_LAG {
                    let lagged = &history[history.len() - SF_TIME_LAG];
                    let mut s = 0.0f32;
                    for k in 0..n {
                        let d = maxed[k] - lagged[k];
                        if d > 0.0 { s += d; }
                    }
                    s
                } else { 0.0 };
                if history.len() > SF_TIME_LAG { history.pop_front(); }
                history.push_back(maxed);
                flux
            }
            Self::ComplexDomain { prev_mag, prev_phase, prev_prev_phase, rectified, frames_seen } => {
                let mut sum = 0.0f32;
                for (k, &c) in spectrum.iter().enumerate() {
                    let mag = c.norm();
                    let phase = c.arg();
                    if *frames_seen >= 2 {
                        let phase_delta = wrap_pi(prev_phase[k] - prev_prev_phase[k]);
                        let predicted_phase = prev_phase[k] + phase_delta;
                        let predicted = Complex32::from_polar(prev_mag[k], predicted_phase);
                        let diff = (c - predicted).norm();
                        if !*rectified || mag >= prev_mag[k] {
                            sum += diff;
                        }
                    }
                    prev_prev_phase[k] = prev_phase[k];
                    prev_phase[k] = phase;
                    prev_mag[k] = mag;
                }
                if *frames_seen < 2 { *frames_seen += 1; }
                sum
            }
            Self::SpectralFlux { prev_mag } => {
                let mut flux = 0.0f32;
                for (k, &c) in spectrum.iter().enumerate() {
                    let m = c.norm();
                    let d = m - prev_mag[k];
                    if d > 0.0 { flux += d; }
                    prev_mag[k] = m;
                }
                flux
            }
        }
    }
}

fn wrap_pi(mut x: f32) -> f32 {
    while x > std::f32::consts::PI { x -= std::f32::consts::TAU; }
    while x < -std::f32::consts::PI { x += std::f32::consts::TAU; }
    x
}

// ---------- peak pickers ----------------------------------------------------

enum Picker { Bello(BelloPicker), Vpd(VpdPicker) }

impl Picker {
    fn new(kind: NeoPicker, odf_rate: f64, params: &BeatParams) -> Self {
        match kind {
            NeoPicker::Bello => {
                let mut p = BelloPicker::new(odf_rate);
                p.apply_params(odf_rate, params);
                Self::Bello(p)
            }
            NeoPicker::Vpd => {
                let mut p = VpdPicker::new(odf_rate);
                p.apply_params(odf_rate, params);
                Self::Vpd(p)
            }
        }
    }
    fn apply_params(&mut self, odf_rate: f64, params: &BeatParams) {
        match self {
            Self::Bello(p) => p.apply_params(odf_rate, params),
            Self::Vpd(p) => p.apply_params(odf_rate, params),
        }
    }
    fn step(&mut self, flux: f32, gated: bool) -> Option<u64> {
        match self {
            Self::Bello(p) => p.step(flux, gated),
            Self::Vpd(p) => p.step(flux, gated),
        }
    }
}

struct BelloPicker {
    history: VecDeque<f32>,
    window_frames: usize,
    lambda: f32, delta: f32,
    min_ioi_frames: u64,
    last_onset_frame: Option<u64>,
    frames_seen: u64, f_m2: f32, f_m1: f32,
}

impl BelloPicker {
    fn new(odf_rate: f64) -> Self {
        let window = (odf_rate * BELLO_MEDIAN_WINDOW_S).max(4.0) as usize;
        Self {
            history: VecDeque::with_capacity(window), window_frames: window,
            lambda: 1.0, delta: 1e-4, min_ioi_frames: 1, last_onset_frame: None,
            frames_seen: 0, f_m2: 0.0, f_m1: 0.0,
        }
    }
    fn apply_params(&mut self, odf_rate: f64, params: &BeatParams) {
        self.lambda = (params.threshold.clamp(0.0, 1.0) * 3.0).max(0.05);
        self.min_ioi_frames = (odf_rate * (params.min_ioi_ms.max(1.0) as f64) / 1000.0) as u64;
    }
    fn step(&mut self, flux: f32, gated: bool) -> Option<u64> {
        let (f_m2, f_m1, f_0) = (self.f_m2, self.f_m1, flux);
        self.f_m2 = self.f_m1; self.f_m1 = flux; self.frames_seen += 1;
        self.history.push_back(flux);
        while self.history.len() > self.window_frames { self.history.pop_front(); }
        if gated || self.frames_seen < 3 || self.history.len() < 3 { return None; }
        let median = median_f32(&self.history);
        let threshold = self.lambda * median + self.delta;
        let is_local_max = f_m1 > f_m2 && f_m1 >= f_0 && f_m1 > threshold;
        if !is_local_max { return None; }
        let candidate_frame = self.frames_seen - 2;
        if let Some(last) = self.last_onset_frame
            && candidate_frame.saturating_sub(last) < self.min_ioi_frames { return None; }
        self.last_onset_frame = Some(candidate_frame);
        Some(1)
    }
}

fn median_f32(v: &VecDeque<f32>) -> f32 {
    let mut tmp: Vec<f32> = v.iter().copied().collect();
    tmp.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    tmp[tmp.len() / 2]
}

struct VpdPicker {
    mu: f32, min_ioi_frames: u64, last_onset_frame: Option<u64>,
    frames_seen: u64, f_m2: f32, f_m1: f32,
    last_valley_val: Option<f32>, rolling_max_dvp: f32, decay: f32, baseline: f32,
}

impl VpdPicker {
    fn new(_odf_rate: f64) -> Self {
        Self {
            mu: 0.85, min_ioi_frames: 1, last_onset_frame: None,
            frames_seen: 0, f_m2: 0.0, f_m1: 0.0,
            last_valley_val: None, rolling_max_dvp: 0.0,
            decay: 0.9995, baseline: 1e-4,
        }
    }
    fn apply_params(&mut self, odf_rate: f64, params: &BeatParams) {
        self.mu = (0.5 + params.threshold.clamp(0.0, 1.0) * 0.49).min(0.99);
        self.min_ioi_frames = (odf_rate * (params.min_ioi_ms.max(1.0) as f64) / 1000.0) as u64;
    }
    fn step(&mut self, flux: f32, gated: bool) -> Option<u64> {
        let (f_m2, f_m1, f_0) = (self.f_m2, self.f_m1, flux);
        self.f_m2 = self.f_m1; self.f_m1 = flux; self.frames_seen += 1;
        self.rolling_max_dvp *= self.decay;
        if gated || self.frames_seen < 3 { return None; }
        if f_m1 < f_m2 && f_m1 < f_0 { self.last_valley_val = Some(f_m1); }
        if f_m1 > f_m2 && f_m1 >= f_0
            && let Some(valley_val) = self.last_valley_val {
                let dvp = f_m1 - valley_val;
                if dvp > self.rolling_max_dvp { self.rolling_max_dvp = dvp; }
                let threshold = (self.mu * self.rolling_max_dvp).max(self.baseline);
                if dvp > threshold {
                    let candidate_frame = self.frames_seen - 2;
                    if let Some(last) = self.last_onset_frame
                        && candidate_frame.saturating_sub(last) < self.min_ioi_frames { return None; }
                    self.last_onset_frame = Some(candidate_frame);
                    return Some(1);
                }
            }
        None
    }
}

// ---------- chirp group-delay smoothing -------------------------------------

struct CgdSmoother {
    window: VecDeque<f32>,
    target_len: usize,
    update_every: usize,
    frames_since_update: usize,
    fft_size: usize,
    forward: Arc<dyn RFft<f32>>,
    inverse: Arc<dyn RFft<f32>>,
    smoothed_tail: VecDeque<f32>,
}

impl CgdSmoother {
    fn new(odf_rate: f64) -> Self {
        let target_len = (odf_rate * CGD_WINDOW_S).max(32.0) as usize;
        let fft_size = next_power_of_two(target_len * 2);
        let mut planner = RFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(fft_size);
        let inverse = planner.plan_fft_inverse(fft_size);
        Self {
            window: VecDeque::with_capacity(target_len), target_len,
            update_every: (odf_rate / CGD_UPDATE_HZ).max(1.0) as usize,
            frames_since_update: 0, fft_size, forward, inverse,
            smoothed_tail: VecDeque::new(),
        }
    }
    fn push_and_peek(&mut self, flux: f32) -> f32 {
        self.window.push_back(flux);
        while self.window.len() > self.target_len { self.window.pop_front(); }
        self.frames_since_update += 1;
        if self.frames_since_update >= self.update_every
            && self.window.len() >= self.target_len / 2
        {
            self.frames_since_update = 0;
            self.recompute();
        }
        if let Some(v) = self.smoothed_tail.pop_front() { v } else { flux }
    }
    fn recompute(&mut self) {
        let oss: Vec<f32> = self.window.iter().copied().collect();
        let n = oss.len();
        let fft_n = self.fft_size;
        let mut x: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); fft_n];
        for k in 0..n { x[k] = Complex32::new(oss[k], 0.0); }
        for k in 1..n {
            let mirror = fft_n - k;
            if mirror < fft_n { x[mirror] = Complex32::new(oss[k], 0.0); }
        }
        let mut time = x;
        self.inverse.process(&mut time);
        let norm = 1.0 / fft_n as f32;
        for v in time.iter_mut() { *v *= norm; }
        let r_inv = 1.0 / CGD_RADIUS;
        let mut damped: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); fft_n];
        let mut scale = r_inv;
        for i in 1..fft_n {
            damped[i - 1] = time[i] * scale;
            scale *= r_inv;
            if scale < 1e-30 { break; }
        }
        self.forward.process(&mut damped);
        let mut phase = vec![0.0f32; fft_n];
        for (k, c) in damped.iter().enumerate() { phase[k] = c.arg(); }
        unwrap_phase(&mut phase);
        let mut tau = vec![0.0f32; n];
        for k in 0..n {
            if k == 0 {
                tau[k] = -(phase[1] - phase[0]);
            } else if k + 1 < fft_n {
                tau[k] = -(phase[k + 1] - phase[k - 1]) * 0.5;
            } else {
                tau[k] = -(phase[k] - phase[k - 1]);
            }
        }
        let fresh = (self.update_every + 2).min(n);
        let start = n.saturating_sub(fresh);
        self.smoothed_tail.clear();
        self.smoothed_tail.extend(tau[start..].iter().copied());
    }
}

fn unwrap_phase(phase: &mut [f32]) {
    let two_pi = std::f32::consts::TAU;
    for i in 1..phase.len() {
        let mut d = phase[i] - phase[i - 1];
        while d > std::f32::consts::PI { d -= two_pi; }
        while d < -std::f32::consts::PI { d += two_pi; }
        phase[i] = phase[i - 1] + d;
    }
}

fn next_power_of_two(n: usize) -> usize {
    let mut p = 1usize;
    while p < n.max(1) { p <<= 1; }
    p
}

// ---------- tempo tracker ---------------------------------------------------

struct TempoTracker {
    odf_rate: f64,
    laglen: usize,
    step: usize,
    dfframe: Vec<f32>,
    frames_since_estimate: usize,
    rwv: Vec<f32>,
    last_bpm: Option<f64>,
}

impl TempoTracker {
    fn new(odf_rate: f64) -> Self {
        let winlen = next_power_of_two((odf_rate * 5.8) as usize).max(4);
        let laglen = winlen / 4;
        let step = winlen / 4;
        let rayparam = (odf_rate / 2.0) as f32;
        let r2 = rayparam * rayparam;
        let rwv: Vec<f32> = (0..laglen)
            .map(|i| {
                let k = (i + 1) as f32;
                (k / r2) * (-(k * k) / (2.0 * r2)).exp()
            })
            .collect();
        Self {
            odf_rate, laglen, step,
            dfframe: vec![0.0; winlen],
            frames_since_estimate: 0,
            rwv, last_bpm: None,
        }
    }
    fn push(&mut self, flux: f32) {
        self.dfframe.rotate_left(1);
        if let Some(last) = self.dfframe.last_mut() { *last = flux; }
        self.frames_since_estimate += 1;
    }
    fn estimate(&mut self) -> Option<f64> {
        if self.frames_since_estimate < self.step { return None; }
        self.frames_since_estimate = 0;
        let df = &self.dfframe;
        let n = df.len();
        let mut acf = vec![0.0f32; n];
        for i in 0..n {
            let mut sum = 0.0f32;
            for j in i..n { sum += df[j - i] * df[j]; }
            acf[i] = sum / (n - i) as f32;
        }
        let numelem = 4usize;
        let mut acfout = vec![0.0f32; self.laglen];
        for i in 1..self.laglen.saturating_sub(1) {
            for a in 1..=numelem {
                let inv = 1.0 / (2.0 * a as f32 - 1.0);
                for b in 1..(2 * a) {
                    let idx = i * a + b - 1;
                    if idx < n { acfout[i] += acf[idx] * inv; }
                }
            }
        }
        for (v, w) in acfout.iter_mut().zip(self.rwv.iter()) { *v *= *w; }
        let mut maxidx = 0usize;
        let mut maxval = f32::NEG_INFINITY;
        for (i, &v) in acfout.iter().enumerate().take(self.laglen - 1).skip(1) {
            if v > maxval { maxval = v; maxidx = i; }
        }
        if maxidx == 0 { return None; }
        let y0 = acfout[maxidx - 1] as f64;
        let y1 = acfout[maxidx] as f64;
        let y2 = acfout[maxidx + 1] as f64;
        let denom = y0 - 2.0 * y1 + y2;
        let delta = if denom < -1e-12 { (0.5 * (y0 - y2) / denom).clamp(-1.0, 1.0) } else { 0.0 };
        let period = maxidx as f64 + delta;
        if period <= 0.5 { return None; }
        let bpm = 60.0 * self.odf_rate / period;
        self.last_bpm = Some(bpm);
        Some(bpm)
    }
}
