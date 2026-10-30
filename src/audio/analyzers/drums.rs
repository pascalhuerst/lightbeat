//! Kick, snare and hat triggers. Built on the beat tracker's front end, which
//! already measures how sharply each frequency band starts: the kick is the
//! lowest band, the hat the highest, and the snare/clap channel is a hit that
//! is loud in the mid and high bands together.
//!
//! Each voice fires when its onset strength clears a threshold that follows
//! the recent average, then waits out a short refractory time. A `level` per
//! voice jumps to 1 on a hit and fades, for driving things that should swell
//! rather than flash.

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::audio::analyzers::{AnalyzerHandle, AnalyzerProc, handle_for};
use crate::audio::tracker::frontend::{Frontend, HOP};
use crate::engine::types::{ParamDef, ParamValue};

/// Frontend channels used for the three voices.
const CHANNELS: [usize; 3] = [0, 4, 3];
/// Quiet time after a hit, in seconds.
const REFRACTORY_S: [f32; 3] = [0.12, 0.10, 0.045];
/// The threshold is `BASE + RATIO * recent mean`, divided by sensitivity.
/// The kick band also carries bass notes and pads, so it asks for more.
const BASE: [f32; 3] = [4.0, 3.8, 2.5];
const RATIO: f32 = 1.5;
/// How long the recent mean looks back.
const MEAN_S: f32 = 1.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrumParams {
    pub sensitivity: f32,
    pub release_ms: f32,
}

impl Default for DrumParams {
    fn default() -> Self {
        Self { sensitivity: 1.0, release_ms: 150.0 }
    }
}

fn describe(p: &DrumParams) -> Vec<ParamDef> {
    vec![
        ParamDef::Float { name: "sensitivity".into(), value: p.sensitivity, min: 0.3, max: 3.0, step: 0.05, unit: "" },
        ParamDef::Float { name: "release_ms".into(), value: p.release_ms, min: 10.0, max: 1000.0, step: 5.0, unit: "ms" },
    ]
}

fn apply(p: &mut DrumParams, index: usize, value: ParamValue) {
    let v = value.as_f32();
    match index {
        0 => p.sensitivity = v.max(0.1),
        1 => p.release_ms = v.max(1.0),
        _ => {}
    }
}

#[derive(Default)]
struct Voice {
    history: VecDeque<f32>,
    sum: f32,
    quiet_frames: u32,
    count: u64,
    level: f32,
}

impl Voice {
    /// Feed one frame's onset strength; returns true on a hit.
    fn feed(&mut self, strength: f32, base: f32, sensitivity: f32, refractory: u32, window: usize) -> bool {
        let mean = if self.history.is_empty() { 0.0 } else { self.sum / self.history.len() as f32 };
        let threshold = (base + RATIO * mean) / sensitivity;
        self.history.push_back(strength);
        self.sum += strength;
        if self.history.len() > window {
            self.sum -= self.history.pop_front().unwrap_or(0.0);
        }
        self.quiet_frames = self.quiet_frames.saturating_sub(1);
        if strength > threshold && self.quiet_frames == 0 {
            self.quiet_frames = refractory;
            self.count += 1;
            self.level = 1.0;
            return true;
        }
        false
    }
}

pub struct DrumsProc {
    params: Arc<RwLock<DrumParams>>,
    sample_rate: u32,
    frontend: Frontend,
    voices: [Voice; 3],
    refractory: [u32; 3],
    window: usize,
}

impl AnalyzerProc for DrumsProc {
    fn num_outputs(&self) -> usize { 6 }

    fn step(&mut self, samples: &[f32]) {
        let p = *self.params.read();
        let (voices, refractory, window) = (&mut self.voices, self.refractory, self.window);
        self.frontend.push(samples, |frame| {
            for (i, v) in voices.iter_mut().enumerate() {
                v.feed(frame.channels[CHANNELS[i]], BASE[i], p.sensitivity, refractory[i], window);
            }
        });
        let chunk_ms = samples.len() as f32 / self.sample_rate.max(1) as f32 * 1000.0;
        let fall = (-chunk_ms / p.release_ms).exp();
        for v in &mut self.voices {
            v.level *= fall;
        }
    }

    fn outputs(&self) -> Vec<f32> {
        let mut out = vec![0.0; 3];
        out.extend(self.voices.iter().map(|v| v.level));
        out
    }

    fn trigger_counts(&self) -> Vec<u64> {
        self.voices.iter().map(|v| v.count).collect()
    }
}

pub fn create(sample_rate: u32) -> (AnalyzerHandle, Box<dyn AnalyzerProc>) {
    let params = Arc::new(RwLock::new(DrumParams::default()));
    let handle = handle_for(params.clone(), describe, apply);
    let frames_per_s = sample_rate as f32 / HOP as f32;
    let proc = DrumsProc {
        params,
        sample_rate,
        frontend: Frontend::new(sample_rate),
        voices: Default::default(),
        refractory: REFRACTORY_S.map(|s| (s * frames_per_s) as u32),
        window: (MEAN_S * frames_per_s) as usize,
    };
    (handle, Box::new(proc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(name: &str, seconds: usize) -> (Vec<u64>, crate::beat_bench::scenarios::Generated) {
        let g = crate::beat_bench::generated(name);
        let (_h, mut p) = create(g.truth.sample_rate);
        let n = g.truth.sample_rate as usize * seconds;
        for chunk in g.audio[..n.min(g.audio.len())].chunks(512) {
            p.step(chunk);
        }
        (p.trigger_counts(), g)
    }

    #[test]
    fn a_four_on_the_floor_house_track_gives_one_kick_per_beat() {
        let (counts, g) = run("house_124", 60);
        let beats = g.truth.beats.iter().filter(|&&t| t < 60.0).count() as f32;
        let kicks = counts[0] as f32;
        assert!((0.85..=1.15).contains(&(kicks / beats)), "{kicks} kicks for {beats} beats ({counts:?})");
        // Claps on 2 and 4, hats on the off-beats: some, and not on every frame.
        assert!(counts[1] as f32 > beats * 0.25 && (counts[1] as f32) < beats * 1.5, "snare {counts:?}");
        assert!(counts[2] as f32 > beats * 0.5 && (counts[2] as f32) < beats * 6.0, "hat {counts:?}");
    }

    #[test]
    fn no_kicks_are_found_where_the_track_has_none() {
        let g = crate::beat_bench::generated("trance_138_breakdown");
        let sr = g.truth.sample_rate as usize;
        let (_h, mut p) = create(g.truth.sample_rate);
        let &(from, to) = g.truth.no_kick.first().expect("a no-kick stretch");
        let (from, to) = ((from * sr as f64) as usize + sr, (to * sr as f64) as usize);
        for chunk in g.audio[..from.min(g.audio.len())].chunks(512) {
            p.step(chunk);
        }
        let before = p.trigger_counts()[0];
        for chunk in g.audio[from..to.min(g.audio.len())].chunks(512) {
            p.step(chunk);
        }
        let during = p.trigger_counts()[0] - before;
        // Sustained bass notes and pads still start in the kick's band, so
        // some false hits remain; there are far fewer than with a kick drum.
        let seconds = (to - from) as f32 / sr as f32;
        let with_kick = seconds * 124.0 / 60.0;
        assert!((during as f32) < with_kick * 0.5, "{during} kicks in {seconds} s without any");
    }

    #[test]
    fn the_level_jumps_on_a_hit_and_fades() {
        let mut v = Voice::default();
        assert!(v.feed(9.0, 3.0, 1.0, 10, 100));
        assert_eq!(v.level, 1.0);
        assert!(!v.feed(9.0, 3.0, 1.0, 10, 100), "inside the refractory time");
    }
}
