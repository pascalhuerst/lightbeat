//! Beat clock for light shows: tempo, beat and bar position of the music.
//!
//! Wraps `audio::tracker::BeatTracker`. Unlike the `Beat` analyzer, which
//! fires on every onset, this is a clock that keeps running through breaks
//! and can be divided: the outputs are phases (0..1 sawtooth) at beat, bar
//! and sixteenth level, plus the sixteenth index within the bar.

use std::sync::Arc;

use parking_lot::RwLock;

use crate::audio::analyzers::{AnalyzerHandle, AnalyzerProc};
use crate::audio::tracker::{BeatTracker, tempo};
use crate::engine::types::{ParamDef, ParamValue};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockParams {
    pub min_bpm: f32,
    pub max_bpm: f32,
    /// Moves where the bar starts, in sixteenth notes. For when the tracker
    /// is a beat out and the bar start has to be set by ear.
    pub bar_shift: i64,
    /// Runs the clock this far ahead, to make up for latency between the
    /// sound and the lights.
    pub lead_ms: f32,
}

impl Default for ClockParams {
    fn default() -> Self {
        Self {
            min_bpm: tempo::DEFAULT_MIN_BPM as f32,
            max_bpm: tempo::DEFAULT_MAX_BPM as f32,
            bar_shift: 0,
            lead_ms: 0.0,
        }
    }
}

fn params_for(p: &ClockParams) -> Vec<ParamDef> {
    vec![
        ParamDef::Float { name: "min_bpm".into(), value: p.min_bpm, min: 40.0, max: 240.0, step: 1.0, unit: "BPM" },
        ParamDef::Float { name: "max_bpm".into(), value: p.max_bpm, min: 60.0, max: 300.0, step: 1.0, unit: "BPM" },
        ParamDef::Int { name: "bar_shift".into(), value: p.bar_shift, min: -16, max: 16 },
        ParamDef::Float { name: "lead_ms".into(), value: p.lead_ms, min: -250.0, max: 250.0, step: 1.0, unit: "ms" },
    ]
}

type SharedParams = Arc<RwLock<ClockParams>>;

fn apply_param(params: &SharedParams, index: usize, value: ParamValue) {
    let mut p = params.write();
    match index {
        0 => p.min_bpm = value.as_f32(),
        1 => p.max_bpm = value.as_f32(),
        2 => p.bar_shift = value.as_i64().clamp(-16, 16),
        3 => p.lead_ms = value.as_f32(),
        _ => {}
    }
}

pub struct BeatClockProc {
    params: SharedParams,
    current: ClockParams,
    sample_rate: u32,
    tracker: BeatTracker,
    beats_counted: u64,
    last_beat: Option<i64>,
    out: [f32; 7],
}

impl BeatClockProc {
    fn new_tracker(sample_rate: u32, p: &ClockParams) -> BeatTracker {
        // A range narrower than an octave is what settles half/double tempo,
        // so keep it at least a sliver wide and ordered.
        let lo = p.min_bpm.min(p.max_bpm - 10.0).max(30.0) as f64;
        let hi = p.max_bpm.max(p.min_bpm + 10.0) as f64;
        BeatTracker::with_range(sample_rate, lo, hi)
    }
}

impl AnalyzerProc for BeatClockProc {
    fn num_outputs(&self) -> usize { 7 }

    fn step(&mut self, samples: &[f32]) {
        let p = *self.params.read();
        if (p.min_bpm, p.max_bpm) != (self.current.min_bpm, self.current.max_bpm) {
            self.tracker = Self::new_tracker(self.sample_rate, &p);
            self.last_beat = None;
        }
        self.current = p;

        self.tracker.push(samples);
        let s = self.tracker.state();
        if s.tempo_bpm <= 0.0 {
            self.out = [0.0; 7];
            return;
        }

        let lead_beats = p.lead_ms as f64 / 1000.0 * s.tempo_bpm / 60.0;
        let beats = s.beat_phase + lead_beats + p.bar_shift as f64 / 4.0;
        let beat_index = beats.floor() as i64;
        match self.last_beat {
            Some(last) if beat_index == last + 1 => self.beats_counted += 1,
            // A jump (re-lock, bar shift) is not a beat.
            _ => {}
        }
        self.last_beat = Some(beat_index);

        let bar = beats.rem_euclid(4.0);
        let confidence = s.tempo_confidence.min(1.0) * ((s.beat_sharpness - 1.0) / 2.0).clamp(0.0, 1.0);
        self.out = [
            0.0, // trigger slot, driven by `onset_count`
            beats.rem_euclid(1.0) as f32,
            (bar / 4.0) as f32,
            (bar * 4.0).rem_euclid(1.0) as f32,
            (bar * 4.0).floor() as f32,
            s.tempo_bpm as f32,
            confidence as f32,
        ];
    }

    fn outputs(&self) -> Vec<f32> { self.out.to_vec() }

    fn onset_count(&self) -> u64 { self.beats_counted }
}

pub fn create(sample_rate: u32) -> (AnalyzerHandle, Box<dyn AnalyzerProc>) {
    let params = Arc::new(RwLock::new(ClockParams::default()));
    let current = *params.read();

    let p_get = params.clone();
    let get_params = Arc::new(move || params_for(&*p_get.read()))
        as Arc<dyn Fn() -> Vec<ParamDef> + Send + Sync>;
    let p_set = params.clone();
    let set_param = Arc::new(move |idx: usize, v: ParamValue| apply_param(&p_set, idx, v))
        as Arc<dyn Fn(usize, ParamValue) + Send + Sync>;

    let proc = Box::new(BeatClockProc {
        tracker: BeatClockProc::new_tracker(sample_rate, &current),
        params,
        current,
        sample_rate,
        beats_counted: 0,
        last_beat: None,
        out: [0.0; 7],
    });
    (AnalyzerHandle::new(get_params, set_param), proc)
}
