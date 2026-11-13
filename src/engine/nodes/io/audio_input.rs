use crate::audio::analyzers::AnalyzerKind;
use crate::audio::manager::{ConnectionStatus, SharedAudioInputs};
use crate::engine::nodes::io::status::{level, with_status_port};
use crate::engine::types::*;

/// Decode a JSON number/bool into the right `ParamValue` variant given the
/// expected `ParamDef` shape.
fn json_to_param_value(v: &serde_json::Value, def: &ParamDef) -> Option<ParamValue> {
    match def {
        ParamDef::Float { .. } => v.as_f64().map(|f| ParamValue::Float(f as f32)),
        ParamDef::Int { .. } => v.as_i64().map(ParamValue::Int),
        ParamDef::Bool { .. } => v.as_bool().map(ParamValue::Bool),
        ParamDef::Choice { .. } => v.as_u64().map(|n| ParamValue::Choice(n as usize)),
    }
}

pub struct AudioInputDisplay {
    pub input_id: u32,
    pub input_name: String,
    /// Per-output (name, port_type, current value).
    pub outputs: Vec<(String, PortType, f32)>,
    /// (kind, output values for this analyzer). Lets the widget render
    /// per-kind mini visualisations (e.g. a level bar for PeakLevel).
    pub analyzer_results: Vec<(AnalyzerKind, Vec<f32>)>,
}

/// Per-analyzer edge-detection cache for onset triggers.
#[derive(Default, Clone)]
struct AnalyzerCache {
    /// Trigger counters seen last tick; None until the first one, so a
    /// worker restarting at zero cannot read as a burst of events.
    last_counts: Option<Vec<u64>>,
}

pub struct AudioInputProcessNode {
    id: NodeId,
    /// Bound audio input id (0 = none selected).
    input_id: u32,
    /// Port layout — "connected" at 0, then the analyzer ports. Rebuilt when
    /// the bound input's analyzer set changes. Empty while no input is bound.
    outputs: Vec<PortDef>,
    output_values: Vec<f32>,
    /// Analyzer kinds currently reflected in the port layout.
    cached_kinds: Vec<AnalyzerKind>,
    /// Per-analyzer onset-count cache (same order as `cached_kinds`).
    caches: Vec<AnalyzerCache>,
    /// Output count of each cached kind; `AnalyzerKind::outputs` builds a
    /// Vec of named ports, too much to do per tick.
    widths: Vec<usize>,
    /// Whether the last tick had an analyzer frame; without one the
    /// per-analyzer results are not shown.
    has_frame: bool,
    audio: SharedAudioInputs,
}

impl AudioInputProcessNode {
    /// The bound input's analyzer settings, read when asked for (the UI sync
    /// and saving) instead of copied on every tick.
    fn analyzer_params(&self) -> Vec<ParamDef> {
        let state = self.audio.lock().unwrap();
        state.iter().find(|c| c.id == self.input_id).map(|c| c.analyzer_param_defs()).unwrap_or_default()
    }

    pub fn new(id: NodeId, audio: SharedAudioInputs) -> Self {
        Self {
            id,
            input_id: 0,
            outputs: Vec::new(),
            output_values: Vec::new(),
            cached_kinds: Vec::new(),
            caches: Vec::new(),
            widths: Vec::new(),
            has_frame: false,
            audio,
        }
    }
}

impl ProcessNode for AudioInputProcessNode {
    fn node_id(&self) -> NodeId {
        self.id
    }
    fn type_name(&self) -> &'static str {
        "Audio Input"
    }
    fn inputs(&self) -> &[PortDef] {
        &[]
    }
    fn outputs(&self) -> &[PortDef] {
        &self.outputs
    }

    fn process(&mut self) {
        self.has_frame = false;
        let state = self.audio.lock().unwrap();
        let input = state.iter().find(|c| c.id == self.input_id);
        let Some(input) = input else {
            for v in &mut self.output_values {
                *v = 0.0;
            }
            self.outputs.clear();
            self.cached_kinds.clear();
            self.widths.clear();
            self.caches.clear();
            return;
        };

        // Port layout follows the persistent `analyzer_kinds` — the config,
        // not the live handles. This means disabling the audio input (which
        // clears the live handles/frame) leaves the port list intact, so
        // wires in the graph survive a disable → enable toggle.
        // True only while a stream is actually open; an input that is
        // disabled or whose device is missing reports false.
        let connected = input.status == ConnectionStatus::Connected;
        if input.analyzer_kinds != self.cached_kinds || self.outputs.is_empty() {
            let kinds = &input.analyzer_kinds;
            let mut expected: Vec<PortDef> = Vec::new();
            for (i, k) in kinds.iter().enumerate() {
                let prefix = crate::audio::analyzers::port_prefix(kinds, i);
                for p in k.outputs() {
                    expected.push(PortDef::new(format!("{}{}", prefix, p.name), p.port_type));
                }
            }
            self.outputs = with_status_port(expected);
            self.output_values = vec![0.0; self.outputs.len()];
            self.caches = vec![AnalyzerCache::default(); kinds.len()];
            self.widths = kinds.iter().map(|k| k.outputs().len()).collect();
            self.cached_kinds = kinds.clone();
        }

        self.output_values[0] = level(connected);

        // Read the latest phase-aligned analyzer frame.
        let Some(frame_arc) = input.analyzer_frame.clone() else {
            for v in self.output_values.iter_mut().skip(1) { *v = 0.0; }
            // Reset onset caches so a future re-enable (worker restart with
            // onset_counter back at 0) doesn't edge-detect against our
            // pre-disable cache value and fire a spurious pulse.
            for c in self.caches.iter_mut() { *c = AnalyzerCache::default(); }
            return;
        };
        // Drop the state lock before touching the frame mutex so the audio
        // worker and the engine tick can't deadlock each other.
        drop(state);
        let frame = frame_arc.lock();
        self.has_frame = true;

        // Slot analyzer values into the output buffer. Trigger outputs are
        // edge-detected from the monotonic counters emitted by the worker.
        // `slot` indexes our outputs, which start with the status port; the
        // worker's frame has no such port, so it is walked separately.
        let mut slot = 1;
        let mut frame_slot = 0;
        for (ai, k) in self.cached_kinds.iter().enumerate() {
            let n = self.widths[ai];
            let triggers = k.trigger_outputs();
            let counts: &[u64] = frame.onset_counts.get(ai).map(|v| v.as_slice()).unwrap_or(&[]);
            let cache = &mut self.caches[ai];
            for j in 0..n {
                let out = match triggers.iter().position(|&t| t == j) {
                    Some(ti) => {
                        let cur = counts.get(ti).copied().unwrap_or(0);
                        match &cache.last_counts {
                            Some(p) if p.get(ti).copied().unwrap_or(0) != cur => 1.0,
                            _ => 0.0,
                        }
                    }
                    None => frame.values.get(frame_slot + j).copied().unwrap_or(0.0),
                };
                if let Some(s) = self.output_values.get_mut(slot + j) {
                    *s = out;
                }
            }
            match &mut cache.last_counts {
                Some(last) => {
                    last.clear();
                    last.extend_from_slice(counts);
                }
                None => cache.last_counts = Some(counts.to_vec()),
            }
            slot += n;
            frame_slot += n;
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        self.output_values.get(pi).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        // Persist analyzer params by name (e.g. "a0.threshold") so the saved
        // values survive analyzer add/remove that shifts global indices.
        let mut params = serde_json::Map::new();
        for def in &self.analyzer_params() {
            let value = match def {
                ParamDef::Float { value, .. } => serde_json::json!(value),
                ParamDef::Int { value, .. } => serde_json::json!(value),
                ParamDef::Bool { value, .. } => serde_json::json!(value),
                ParamDef::Choice { value, .. } => serde_json::json!(value),
            };
            params.insert(def.name().to_string(), value);
        }
        Some(serde_json::json!({
            "input_id": self.input_id,
            "params": params,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(id) = data.get("input_id").and_then(|v| v.as_u64()) {
            self.input_id = id as u32;
            self.outputs.clear();
            self.output_values.clear();
            self.caches.clear();
            self.cached_kinds.clear();
        }

        // Apply saved param values by name, looked up on the bound audio
        // input.
        if let Some(saved) = data.get("params").and_then(|v| v.as_object()) {
            let state = self.audio.lock().unwrap();
            if let Some(input) = state.iter().find(|c| c.id == self.input_id) {
                let defs = input.analyzer_param_defs();
                for (name, value) in saved {
                    if let Some(idx) = defs.iter().position(|d| d.name() == name)
                        && let Some(pv) = json_to_param_value(value, &defs[idx]) {
                            input.set_param(idx, pv);
                        }
                }
            }
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        self.analyzer_params()
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        let state = self.audio.lock().unwrap();
        if let Some(input) = state.iter().find(|c| c.id == self.input_id) {
            input.set_param(index, value);
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        let name = {
            let state = self.audio.lock().unwrap();
            state.iter().find(|c| c.id == self.input_id).map(|c| c.name.clone()).unwrap_or_default()
        };
        let outputs = self.outputs.iter().enumerate()
            .map(|(i, p)| (p.name.clone(), p.port_type, self.output_values.get(i).copied().unwrap_or(0.0)))
            .collect();
        let mut analyzer_results = Vec::new();
        if self.has_frame {
            let mut o = 1;
            for (k, n) in self.cached_kinds.iter().zip(&self.widths) {
                let vals = (o..o + n).map(|i| self.output_values.get(i).copied().unwrap_or(0.0)).collect();
                analyzer_results.push((*k, vals));
                o += n;
            }
        }
        shared.display = Some(Box::new(AudioInputDisplay {
            input_id: self.input_id,
            input_name: name,
            outputs,
            analyzer_results,
        }));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::audio::analyzers::worker::AnalyzerFrame;
    use crate::audio::manager::{AudioInputRuntime, ConnectionStatus};

    fn input_with(kinds: &[AnalyzerKind], values: Vec<f32>, counts: Vec<Vec<u64>>) -> SharedAudioInputs {
        let frame = Arc::new(parking_lot::Mutex::new(AnalyzerFrame {
            values,
            onset_counts: counts,
            sample_index: 0,
        }));
        Arc::new(Mutex::new(vec![AudioInputRuntime {
            id: 7,
            name: "in".into(),
            enabled: true,
            backend: Default::default(),
            device_name: String::new(),
            sample_rate: None,
            buffer_size_frames: None,
            analyzer_kinds: kinds.to_vec(),
            analyzer_handles: Vec::new(),
            analyzer_frame: Some(frame),
            status: ConnectionStatus::Connected,
            actual_sample_rate: 44_100,
            actual_buffer_frames: 512,
            bound_to: None,
        }]))
    }

    fn node(audio: SharedAudioInputs) -> AudioInputProcessNode {
        let mut n = AudioInputProcessNode::new(NodeId(1), audio);
        n.load_data(&serde_json::json!({ "input_id": 7 }));
        n.process();
        n
    }

    #[test]
    fn each_analyzer_output_reads_its_own_value_after_the_status_port() {
        // Beat clock: beat, beat phase, bar phase, 16th phase, step, bpm, confidence.
        let clock = vec![0.0, 0.25, 0.5, 0.75, 3.0, 128.0, 0.9];
        let mut values = clock.clone();
        values.extend([0.8, 0.3, 0.6]); // peak, rms, envelope
        let n = node(input_with(
            &[AnalyzerKind::BeatClock, AnalyzerKind::PeakLevel, AnalyzerKind::Envelope],
            values,
            vec![vec![0], vec![], vec![]],
        ));
        let out: Vec<f32> = (0..n.outputs().len()).map(|i| n.read_output(i)).collect();
        assert_eq!(out[0], 1.0, "connected");
        assert_eq!(out[6], 128.0, "bpm is in beats per minute, not the next value over");
        assert_eq!(out[7], 0.9, "confidence");
        assert_eq!(&out[8..11], &[0.8, 0.3, 0.6]);
    }

    #[test]
    fn the_display_lists_outputs_and_per_analyzer_results() {
        let n = node(input_with(
            &[AnalyzerKind::PeakLevel, AnalyzerKind::Envelope],
            vec![0.8, 0.3, 0.6],
            vec![vec![], vec![]],
        ));
        let mut shared = NodeSharedState::new(0, n.outputs().len());
        n.update_display(&mut shared);
        let d = shared.display.as_ref().unwrap().downcast_ref::<AudioInputDisplay>().unwrap();
        assert_eq!(d.input_name, "in");
        assert_eq!(d.outputs.len(), n.outputs().len());
        assert_eq!(d.outputs[0].2, 1.0);
        assert_eq!(d.analyzer_results.len(), 2);
        assert_eq!(d.analyzer_results[0].0, AnalyzerKind::PeakLevel);
        assert_eq!(d.analyzer_results[0].1[0], 0.8);
    }

    #[test]
    fn a_trigger_output_pulses_when_its_counter_moves() {
        let audio = input_with(&[AnalyzerKind::Drums], vec![0.0; 6], vec![vec![1, 0, 0]]);
        let mut n = node(audio.clone());
        assert_eq!(n.read_output(1), 0.0, "the first sight of the counters is not an event");
        let frame = audio.lock().unwrap()[0].analyzer_frame.clone().unwrap();
        frame.lock().onset_counts = vec![vec![2, 0, 0]];
        n.process();
        assert_eq!((n.read_output(1), n.read_output(2)), (1.0, 0.0), "kick pulses, snare does not");
        n.process();
        assert_eq!(n.read_output(1), 0.0);
    }
}
