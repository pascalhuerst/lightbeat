//! Onset + BPM detector. Aubio-style port from
//! `beat_detection_tests/src/detectors/beat_detection.rs`.

use std::sync::Arc;

use num_complex::Complex32;
use parking_lot::RwLock;
use realfft::{RealFftPlanner, RealToComplex};

use crate::audio::analyzers::{AnalyzerHandle, AnalyzerKind, AnalyzerProc};
use crate::engine::types::{ParamDef, ParamValue};

const ANALYSIS_WINDOW: usize = 1024;
const HOP_SIZE: usize = 512;

/// aubio-style picker waits `PICKER_WIN_POST` frames of lookahead before
/// confirming a candidate. That's our intrinsic output latency.
const PICKER_WIN_PRE: usize = 1;
const PICKER_WIN_POST: usize = 5;
const PICKER_WIN: usize = PICKER_WIN_PRE + 1 + PICKER_WIN_POST;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnsetMethod {
    Hfc,
    Energy,
    SpecFlux,
}

impl OnsetMethod {
    pub const ALL: [Self; 3] = [Self::Hfc, Self::Energy, Self::SpecFlux];
    pub fn label(&self) -> &'static str {
        match self {
            Self::Hfc => "HFC",
            Self::Energy => "Energy",
            Self::SpecFlux => "Spectral flux",
        }
    }
    pub fn to_index(&self) -> usize {
        Self::ALL.iter().position(|m| m == self).unwrap_or(0)
    }
    pub fn from_index(i: usize) -> Self {
        Self::ALL.get(i).copied().unwrap_or(Self::Hfc)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioBeatParams {
    pub method: OnsetMethod,
    pub silence_db: f32,
    pub threshold: f32,
    pub min_ioi_ms: f32,
}

impl Default for AudioBeatParams {
    fn default() -> Self {
        Self {
            method: OnsetMethod::Hfc,
            silence_db: -90.0,
            threshold: 0.1,
            min_ioi_ms: 50.0,
        }
    }
}

pub fn audio_beat_params() -> Vec<ParamDef> {
    let d = AudioBeatParams::default();
    params_for(&d)
}

fn params_for(p: &AudioBeatParams) -> Vec<ParamDef> {
    vec![
        ParamDef::Choice {
            name: "method".into(),
            value: p.method.to_index(),
            options: OnsetMethod::ALL.iter().map(|m| m.label().to_string()).collect(),
        },
        ParamDef::Float {
            name: "silence_db".into(),
            value: p.silence_db, min: -80.0, max: 0.0, step: 1.0, unit: "dB",
        },
        ParamDef::Float {
            name: "threshold".into(),
            value: p.threshold, min: 0.0, max: 1.0, step: 0.01, unit: "",
        },
        ParamDef::Float {
            name: "min_ioi_ms".into(),
            value: p.min_ioi_ms, min: 1.0, max: 500.0, step: 1.0, unit: "ms",
        },
    ]
}

pub type SharedAudioBeatParams = Arc<RwLock<AudioBeatParams>>;

fn apply_param(params: &SharedAudioBeatParams, index: usize, value: ParamValue) {
    let mut p = params.write();
    match index {
        0 => p.method = OnsetMethod::from_index(value.as_i64() as usize),
        1 => p.silence_db = value.as_f32(),
        2 => p.threshold = value.as_f32(),
        3 => p.min_ioi_ms = value.as_f32(),
        _ => {}
    }
}

pub struct AudioBeatProc {
    params: SharedAudioBeatParams,
    current: AudioBeatParams,
    stft: Stft,
    odf: Odf,
    picker: OnsetPicker,
    tempo: TempoTracker,
    onset_counter: u64,
    bpm: f32,
}

impl AnalyzerProc for AudioBeatProc {
    fn kind(&self) -> AnalyzerKind { AnalyzerKind::AudioBeat }
    fn num_outputs(&self) -> usize { 2 }
    fn first_output_is_trigger(&self) -> bool { true }
    fn output_latency_samples(&self) -> u32 { (PICKER_WIN_POST as u32) * HOP_SIZE as u32 }

    fn step(&mut self, samples: &[f32]) {
        let p = *self.params.read();
        if p != self.current {
            if p.method != self.current.method {
                let num_bins = ANALYSIS_WINDOW / 2 + 1;
                self.odf = Odf::new(p.method, num_bins);
                self.picker = OnsetPicker::new(self.stft.odf_rate);
                self.tempo = TempoTracker::new(self.stft.odf_rate);
            }
            self.picker.apply_params(&p, self.stft.odf_rate);
            self.current = p;
        }
        let current = self.current;
        let odf = &mut self.odf;
        let picker = &mut self.picker;
        let tempo = &mut self.tempo;
        let onset_counter = &mut self.onset_counter;
        let bpm_out = &mut self.bpm;
        self.stft.push(samples, |_frame_idx, magnitudes, frame_rms| {
            let flux = odf.step(magnitudes);
            tempo.push(flux);
            let frame_db = 20.0 * frame_rms.max(1e-9).log10();
            let gated = frame_db < current.silence_db;
            if picker.step(flux, gated).is_some() {
                *onset_counter += 1;
            }
            if let Some(bpm) = tempo.estimate() {
                *bpm_out = bpm as f32;
            }
        });
    }

    fn outputs(&self) -> Vec<f32> { vec![0.0, self.bpm] }
    fn onset_count(&self) -> u64 { self.onset_counter }
}

pub fn create(sample_rate: u32) -> (AnalyzerHandle, Box<dyn AnalyzerProc>) {
    let params = Arc::new(RwLock::new(AudioBeatParams::default()));
    let current = *params.read();

    let stft = Stft::new(ANALYSIS_WINDOW, HOP_SIZE, sample_rate);
    let num_bins = ANALYSIS_WINDOW / 2 + 1;
    let odf = Odf::new(current.method, num_bins);
    let mut picker = OnsetPicker::new(stft.odf_rate);
    picker.apply_params(&current, stft.odf_rate);
    let tempo = TempoTracker::new(stft.odf_rate);

    let p_get = params.clone();
    let get_params = Arc::new(move || params_for(&*p_get.read()))
        as Arc<dyn Fn() -> Vec<ParamDef> + Send + Sync>;
    let p_set = params.clone();
    let set_param = Arc::new(move |idx: usize, v: ParamValue| apply_param(&p_set, idx, v))
        as Arc<dyn Fn(usize, ParamValue) + Send + Sync>;

    let proc = Box::new(AudioBeatProc {
        params, current, stft, odf, picker, tempo,
        onset_counter: 0, bpm: 0.0,
    });
    (AnalyzerHandle::new(AnalyzerKind::AudioBeat, get_params, set_param), proc)
}

// ---------- STFT (magnitudes) -----------------------------------------------

struct Stft {
    fft_size: usize,
    hop_size: usize,
    window: Vec<f32>,
    buffer: Vec<f32>,
    plan: Arc<dyn RealToComplex<f32>>,
    scratch: Vec<Complex32>,
    spectrum: Vec<Complex32>,
    frame_in: Vec<f32>,
    magnitudes: Vec<f32>,
    frame_idx: u64,
    odf_rate: f64,
}

impl Stft {
    fn new(fft_size: usize, hop_size: usize, sample_rate: u32) -> Self {
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
            magnitudes: vec![0.0; fft_size / 2 + 1],
            frame_idx: 0,
            odf_rate: sample_rate as f64 / hop_size as f64,
        }
    }
    fn push(&mut self, samples: &[f32], mut on_frame: impl FnMut(u64, &[f32], f32)) {
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
            for (m, c) in self.magnitudes.iter_mut().zip(self.spectrum.iter()) {
                *m = c.norm();
            }
            on_frame(self.frame_idx, &self.magnitudes, rms);
            self.frame_idx += 1;
            self.buffer.drain(..self.hop_size);
        }
    }
}

fn hann_window(n: usize) -> Vec<f32> {
    (0..n).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (n as f32 - 1.0)).cos()).collect()
}

// ---------- onset detection functions ---------------------------------------

enum Odf {
    Hfc,
    Energy,
    SpecFlux { prev: Vec<f32> },
}

impl Odf {
    fn new(method: OnsetMethod, num_bins: usize) -> Self {
        match method {
            OnsetMethod::Hfc => Self::Hfc,
            OnsetMethod::Energy => Self::Energy,
            OnsetMethod::SpecFlux => Self::SpecFlux { prev: vec![0.0; num_bins] },
        }
    }
    fn step(&mut self, mags: &[f32]) -> f32 {
        match self {
            Self::Hfc => mags.iter().enumerate().map(|(k, &m)| (k + 1) as f32 * m).sum(),
            Self::Energy => mags.iter().map(|&m| m * m).sum(),
            Self::SpecFlux { prev } => {
                let mut flux = 0.0;
                for (m, p) in mags.iter().zip(prev.iter()) {
                    let d = m - p;
                    if d > 0.0 { flux += d; }
                }
                prev.copy_from_slice(mags);
                flux
            }
        }
    }
}

// ---------- onset picker -----------------------------------------------------

struct OnsetPicker {
    buf: [f32; PICKER_WIN],
    filled: usize,
    frames_seen: u64,
    last_onset_frame: Option<u64>,
    min_ioi_frames: u64,
    threshold: f32,
}

impl OnsetPicker {
    fn new(odf_rate: f64) -> Self {
        Self {
            buf: [0.0; PICKER_WIN],
            filled: 0,
            frames_seen: 0,
            last_onset_frame: None,
            min_ioi_frames: (odf_rate * 0.050).max(1.0) as u64,
            threshold: 0.1,
        }
    }
    fn apply_params(&mut self, p: &AudioBeatParams, odf_rate: f64) {
        self.threshold = p.threshold.clamp(0.0, 1.0);
        self.min_ioi_frames = (odf_rate * (p.min_ioi_ms.max(1.0) as f64) / 1000.0) as u64;
    }
    fn step(&mut self, odf: f32, gated: bool) -> Option<u64> {
        for i in 0..PICKER_WIN - 1 {
            self.buf[i] = self.buf[i + 1];
        }
        self.buf[PICKER_WIN - 1] = odf;
        if self.filled < PICKER_WIN { self.filled += 1; }
        self.frames_seen += 1;

        if gated || self.filled < PICKER_WIN { return None; }

        let c = PICKER_WIN_PRE;
        let cand = self.buf[c];
        let prev = self.buf[c - 1];
        let next = self.buf[c + 1];

        let mean = self.buf.iter().sum::<f32>() / PICKER_WIN as f32;
        let mut sorted = self.buf;
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = sorted[PICKER_WIN / 2];

        let thresholded = cand - median - mean * self.threshold;
        if thresholded <= 0.0 { return None; }
        if !(cand > prev && cand >= next) { return None; }

        let candidate_frame = self.frames_seen.saturating_sub(PICKER_WIN_POST as u64);
        if let Some(last) = self.last_onset_frame
            && candidate_frame.saturating_sub(last) < self.min_ioi_frames { return None; }
        self.last_onset_frame = Some(candidate_frame);
        Some(1)
    }
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

fn next_power_of_two(n: usize) -> usize {
    let mut p = 1usize;
    while p < n.max(1) { p <<= 1; }
    p
}
