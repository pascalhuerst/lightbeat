//! Single worker thread per audio input.
//!
//! On each incoming audio chunk, runs every analyzer in sequence on the same
//! sample buffer, then gathers their outputs into one phase-coherent frame.
//! Per-analyzer latency FIFOs hold fast outputs back to match the slowest
//! analyzer's detection lookback — so at any given engine tick the onset,
//! envelope, and peak-level values all describe the same audio sample
//! window.

use std::collections::VecDeque;
use std::sync::Arc;
use std::thread;

use crossbeam_channel::Receiver;
use parking_lot::Mutex;

use crate::audio::analyzers::{AnalyzerHandle, AnalyzerKind, AnalyzerProc, create_analyzer};
use crate::audio::backend::AudioChunk;

/// Snapshot of all analyzer outputs at a common virtual sample index.
#[derive(Clone, Debug, Default)]
pub struct AnalyzerFrame {
    /// Concatenated continuous outputs of all analyzers, in analyzer order
    /// followed by port order within an analyzer.
    pub values: Vec<f32>,
    /// Per-analyzer onset counter (already delay-aligned). Length = number
    /// of analyzers. Analyzers without trigger outputs report 0.
    pub onset_counts: Vec<u64>,
    /// Virtual sample index this frame corresponds to (samples processed
    /// so far, minus `max_output_latency_samples`). Monotonic while the
    /// stream is open.
    pub sample_index: u64,
}

pub type SharedAnalyzerFrame = Arc<Mutex<AnalyzerFrame>>;

pub struct AnalyzerWorker {
    pub frame: SharedAnalyzerFrame,
    pub handles: Vec<AnalyzerHandle>,
    pub sample_rate: u32,
    _join: Option<thread::JoinHandle<()>>,
}

impl AnalyzerWorker {
    pub fn spawn(
        rx: Receiver<AudioChunk>,
        sample_rate: u32,
        kinds: &[AnalyzerKind],
    ) -> Self {
        let mut handles: Vec<AnalyzerHandle> = Vec::with_capacity(kinds.len());
        let mut procs: Vec<Box<dyn AnalyzerProc>> = Vec::with_capacity(kinds.len());
        for k in kinds {
            let (h, p) = create_analyzer(*k, sample_rate);
            handles.push(h);
            procs.push(p);
        }

        // Pre-compute initial frame layout so the engine can read something
        // sensible before the first audio chunk arrives.
        let total_outputs: usize = procs.iter().map(|p| p.num_outputs()).sum();
        let frame = Arc::new(Mutex::new(AnalyzerFrame {
            values: vec![0.0; total_outputs],
            onset_counts: vec![0; procs.len()],
            sample_index: 0,
        }));

        let frame_thread = frame.clone();
        let join = thread::Builder::new()
            .name("lightbeat-analyzer-worker".into())
            .spawn(move || run(rx, procs, frame_thread))
            .expect("spawn analyzer worker thread");

        Self { frame, handles, sample_rate, _join: Some(join) }
    }
}

/// Per-analyzer delay-aligned history.
struct Slot {
    proc: Box<dyn AnalyzerProc>,
    delay_samples: u64,
    /// Ring buffer of (sample-index-at-push, outputs, onset_count). The
    /// sample-index is the total samples processed at the end of the chunk
    /// whose step produced this entry.
    history: VecDeque<(u64, Vec<f32>, u64)>,
    /// Most recently emitted (delayed) values + counter, carried forward
    /// if no new entry is old enough yet.
    latest_values: Vec<f32>,
    latest_onset: u64,
    num_outputs: usize,
}

fn run(
    rx: Receiver<AudioChunk>,
    procs: Vec<Box<dyn AnalyzerProc>>,
    frame: SharedAnalyzerFrame,
) {
    // Worst-case analyzer latency — the "slowest" voice sets the virtual
    // clock; every other analyzer is delayed to match.
    let max_latency = procs
        .iter()
        .map(|p| p.output_latency_samples() as u64)
        .max()
        .unwrap_or(0);

    let mut slots: Vec<Slot> = procs
        .into_iter()
        .map(|p| {
            let lat = p.output_latency_samples() as u64;
            let delay_samples = max_latency.saturating_sub(lat);
            let num_outputs = p.num_outputs();
            Slot {
                proc: p,
                delay_samples,
                history: VecDeque::with_capacity(8),
                latest_values: vec![0.0; num_outputs],
                latest_onset: 0,
                num_outputs,
            }
        })
        .collect();

    let mut samples_processed: u64 = 0;

    while let Ok(chunk) = rx.recv() {
        samples_processed = samples_processed.saturating_add(chunk.mono_f32.len() as u64);
        // Target virtual sample index — everything older than this is "now"
        // for the purposes of this frame.
        let virtual_now = samples_processed.saturating_sub(max_latency);

        // Step each analyzer on the same chunk (lockstep).
        for slot in slots.iter_mut() {
            slot.proc.step(&chunk.mono_f32);
            let outs = slot.proc.outputs();
            let count = slot.proc.onset_count();
            slot.history.push_back((samples_processed, outs, count));
            // Emit target = samples_processed - delay_samples. Walk forward
            // while the next entry is still at or before emit_target, so the
            // front holds the freshest value old enough to emit.
            let emit_target = samples_processed.saturating_sub(slot.delay_samples);
            while slot.history.len() >= 2 {
                let second = slot.history[1].0;
                if second <= emit_target {
                    slot.history.pop_front();
                } else {
                    break;
                }
            }
            if let Some(front) = slot.history.front()
                && front.0 <= emit_target
            {
                slot.latest_values.clone_from(&front.1);
                slot.latest_onset = front.2;
            }
            // Cap history length defensively so a misbehaving proc can't
            // leak memory. 256 entries covers ~256 chunks of history.
            while slot.history.len() > 256 {
                slot.history.pop_front();
            }
        }

        // Concatenate emitted values + onset counts into the shared frame.
        let total_outputs: usize = slots.iter().map(|s| s.num_outputs).sum();
        let mut values: Vec<f32> = Vec::with_capacity(total_outputs);
        let mut onset_counts: Vec<u64> = Vec::with_capacity(slots.len());
        for slot in slots.iter() {
            values.extend_from_slice(&slot.latest_values);
            onset_counts.push(slot.latest_onset);
        }

        let mut fr = frame.lock();
        fr.values = values;
        fr.onset_counts = onset_counts;
        fr.sample_index = virtual_now;
    }
}
