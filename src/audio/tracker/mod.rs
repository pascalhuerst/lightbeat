//! Real-time tempo and beat tracking for music played from a turntable.
//!
//! Three stages, each usable alone:
//! - `frontend`: audio to per-band onset strength.
//! - `tempo`: onset strength to a smoothly following tempo.
//! - `phase`: a free-running beat clock kept in step with the music.
//!
//! Nothing here touches the app; the audio analyzers and the offline test
//! bench both drive `BeatTracker` the same way, by pushing audio.

pub mod bar;
pub mod frontend;
pub mod novelty;
pub mod phase;
pub mod tempo;

use frontend::Frontend;
use phase::PhaseTracker;
use tempo::TempoTracker;

#[derive(Clone, Copy, Debug, Default)]
pub struct TrackerState {
    pub tempo_bpm: f64,
    /// Beats since the start, as of the end of the audio pushed so far.
    pub beat_phase: f64,
    /// Position in the bar, in beats from the bar start (0..4).
    pub bar_position: f64,
    /// How clearly the bar start stands out from the alternatives, 0..1.
    pub bar_confidence: f64,
    /// How sure the tempo estimate is, 0..1.
    pub tempo_confidence: f64,
    /// How clear the beat is in the recent past: 1 is nothing, higher is a clear beat.
    pub beat_sharpness: f64,
}

pub struct BeatTracker {
    sample_rate: u32,
    frontend: Frontend,
    tempo: TempoTracker,
    phase: PhaseTracker,
    samples_pushed: u64,
    /// Sample count at the end of the newest analysed frame.
    last_frame_end: u64,
}

impl BeatTracker {
    pub fn new(sample_rate: u32) -> Self {
        Self::with_range(sample_rate, tempo::DEFAULT_MIN_BPM, tempo::DEFAULT_MAX_BPM)
    }

    /// `min_bpm..max_bpm` is the range the tempo is searched in. Keep it
    /// narrower than an octave and the half/double ambiguity is settled by
    /// the range alone: 87 and 174 BPM are the same drum and bass.
    pub fn with_range(sample_rate: u32, min_bpm: f64, max_bpm: f64) -> Self {
        Self {
            sample_rate,
            frontend: Frontend::new(sample_rate),
            tempo: TempoTracker::new(Frontend::rate(sample_rate), min_bpm, max_bpm),
            phase: PhaseTracker::new(sample_rate),
            samples_pushed: 0,
            last_frame_end: 0,
        }
    }

    pub fn push(&mut self, samples: &[f32]) {
        self.samples_pushed += samples.len() as u64;
        let Self { frontend, tempo, phase, .. } = self;
        frontend.push(samples, |frame| {
            tempo.push(frame.odf);
            phase.step(&frame, tempo.tempo());
        });
        self.last_frame_end = newest_frame_end(self.samples_pushed);
    }

    /// Scores of the sixteen bar-start hypotheses and the chosen one.
    pub fn bar_scores(&self) -> ([f64; bar::STEPS], usize) {
        self.phase.bar_scores()
    }

    /// The clock's own phase, before the bar start is applied.
    pub fn lock_phase(&self) -> f64 {
        self.phase.lock_phase()
    }

    pub fn state(&self) -> TrackerState {
        let tempo = self.phase.tempo();
        // The clock was last advanced at the end of the newest frame; carry
        // it forward over the audio that has arrived since.
        let pending = self.samples_pushed.saturating_sub(self.last_frame_end) as f64;
        let ahead = tempo / 60.0 * pending / self.sample_rate as f64;
        TrackerState {
            tempo_bpm: tempo,
            beat_phase: self.phase.phase() + ahead,
            bar_position: (self.phase.phase() + ahead).rem_euclid(4.0),
            bar_confidence: self.phase.bar_confidence(),
            tempo_confidence: self.tempo.confidence(),
            beat_sharpness: self.phase.sharpness(),
        }
    }
}

/// Frames start at multiples of the hop and are `FFT` samples long, so the
/// newest one ends at the largest `k * HOP + FFT` not beyond `pushed`.
fn newest_frame_end(pushed: u64) -> u64 {
    if pushed < frontend::FFT as u64 {
        return 0;
    }
    let k = (pushed - frontend::FFT as u64) / frontend::HOP as u64;
    k * frontend::HOP as u64 + frontend::FFT as u64
}
