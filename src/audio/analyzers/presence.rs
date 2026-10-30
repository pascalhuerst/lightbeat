//! Whether anything is playing: `signal` is high while the input is above a
//! threshold (and for a hold time afterwards, so a quiet passage does not
//! flicker it), with a trigger when it starts and when it stops.

use std::sync::Arc;

use parking_lot::RwLock;

use crate::audio::analyzers::{AnalyzerHandle, AnalyzerProc, handle_for};
use crate::engine::types::{ParamDef, ParamValue};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresenceParams {
    pub threshold_db: f32,
    pub hold_s: f32,
}

impl Default for PresenceParams {
    fn default() -> Self {
        Self { threshold_db: -50.0, hold_s: 2.0 }
    }
}

fn describe(p: &PresenceParams) -> Vec<ParamDef> {
    vec![
        ParamDef::Float { name: "threshold_db".into(), value: p.threshold_db, min: -90.0, max: -10.0, step: 1.0, unit: "dB" },
        ParamDef::Float { name: "hold_s".into(), value: p.hold_s, min: 0.0, max: 30.0, step: 0.1, unit: "s" },
    ]
}

fn apply(p: &mut PresenceParams, index: usize, value: ParamValue) {
    match index {
        0 => p.threshold_db = value.as_f32(),
        1 => p.hold_s = value.as_f32().max(0.0),
        _ => {}
    }
}

pub struct PresenceProc {
    params: Arc<RwLock<PresenceParams>>,
    sample_rate: u32,
    now: u64,
    last_loud: u64,
    signal: bool,
    started: u64,
    stopped: u64,
}

impl AnalyzerProc for PresenceProc {
    fn num_outputs(&self) -> usize { 3 }

    fn step(&mut self, samples: &[f32]) {
        let p = *self.params.read();
        self.now += samples.len() as u64;
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt();
        let db = 20.0 * rms.max(1e-9).log10();
        if db > p.threshold_db {
            self.last_loud = self.now;
            if !self.signal {
                self.signal = true;
                self.started += 1;
            }
        } else if self.signal && (self.now - self.last_loud) as f32 > p.hold_s * self.sample_rate as f32 {
            self.signal = false;
            self.stopped += 1;
        }
    }

    fn outputs(&self) -> Vec<f32> { vec![f32::from(u8::from(self.signal)), 0.0, 0.0] }
    fn trigger_counts(&self) -> Vec<u64> { vec![self.started, self.stopped] }
}

pub fn create(sample_rate: u32) -> (AnalyzerHandle, Box<dyn AnalyzerProc>) {
    let params = Arc::new(RwLock::new(PresenceParams::default()));
    let handle = handle_for(params.clone(), describe, apply);
    let proc = PresenceProc { params, sample_rate, now: 0, last_loud: 0, signal: false, started: 0, stopped: 0 };
    (handle, Box::new(proc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_starts_with_sound_and_stops_after_the_hold_time() {
        let (_h, mut p) = create(44_100);
        let tone: Vec<f32> = (0..512).map(|i| 0.3 * (i as f32 * 0.1).sin()).collect();
        for _ in 0..10 {
            p.step(&[0.0; 512]);
        }
        assert_eq!(p.outputs()[0], 0.0);
        for _ in 0..10 {
            p.step(&tone);
        }
        assert_eq!(p.outputs()[0], 1.0);
        assert_eq!(p.trigger_counts(), vec![1, 0]);
        // Silence shorter than the 2 s hold keeps it up.
        for _ in 0..100 {
            p.step(&[0.0; 512]);
        }
        assert_eq!(p.outputs()[0], 1.0);
        for _ in 0..100 {
            p.step(&[0.0; 512]);
        }
        assert_eq!(p.outputs()[0], 0.0);
        assert_eq!(p.trigger_counts(), vec![1, 1]);
    }
}
