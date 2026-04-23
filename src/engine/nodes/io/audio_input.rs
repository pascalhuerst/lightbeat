use crate::audio::analyzers::AnalyzerKind;
use crate::audio::manager::SharedAudioInputs;
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
#[derive(Default, Clone, Copy)]
struct AnalyzerCache {
    last_onset_count: u64,
}

pub struct AudioInputProcessNode {
    id: NodeId,
    /// Bound audio input id (0 = none selected).
    input_id: u32,
    /// Port layout — rebuilt when the bound input's analyzer set changes.
    outputs: Vec<PortDef>,
    output_values: Vec<f32>,
    /// Analyzer kinds currently reflected in the port layout.
    cached_kinds: Vec<AnalyzerKind>,
    /// Per-analyzer onset-count cache (same order as `cached_kinds`).
    caches: Vec<AnalyzerCache>,
    display_outputs: Vec<(String, PortType, f32)>,
    display_name: String,
    display_params: Vec<ParamDef>,
    display_analyzer_results: Vec<(AnalyzerKind, Vec<f32>)>,
    audio: SharedAudioInputs,
}

impl AudioInputProcessNode {
    pub fn new(id: NodeId, audio: SharedAudioInputs) -> Self {
        Self {
            id,
            input_id: 0,
            outputs: Vec::new(),
            output_values: Vec::new(),
            cached_kinds: Vec::new(),
            caches: Vec::new(),
            display_outputs: Vec::new(),
            display_name: String::new(),
            display_params: Vec::new(),
            display_analyzer_results: Vec::new(),
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
        let state = self.audio.lock().unwrap();
        let input = state.iter().find(|c| c.id == self.input_id);
        let Some(input) = input else {
            for v in &mut self.output_values {
                *v = 0.0;
            }
            self.display_outputs.clear();
            self.display_name.clear();
            self.display_params.clear();
            self.display_analyzer_results.clear();
            self.outputs.clear();
            self.cached_kinds.clear();
            self.caches.clear();
            return;
        };

        // Port layout follows the persistent `analyzer_kinds` — the config,
        // not the live handles. This means disabling the audio input (which
        // clears the live handles/frame) leaves the port list intact, so
        // wires in the graph survive a disable → enable toggle.
        let kinds: Vec<AnalyzerKind> = input.analyzer_kinds.clone();
        if kinds != self.cached_kinds {
            let mut expected: Vec<PortDef> = Vec::new();
            for (i, k) in kinds.iter().enumerate() {
                let prefix = format!("a{}.", i);
                for p in k.outputs() {
                    expected.push(PortDef::new(format!("{}{}", prefix, p.name), p.port_type));
                }
            }
            self.outputs = expected;
            self.output_values = vec![0.0; self.outputs.len()];
            self.caches = vec![AnalyzerCache::default(); kinds.len()];
            self.cached_kinds = kinds.clone();
        }

        // Read the latest phase-aligned analyzer frame.
        let Some(frame_arc) = input.analyzer_frame.clone() else {
            for v in &mut self.output_values { *v = 0.0; }
            // Reset onset caches so a future re-enable (worker restart with
            // onset_counter back at 0) doesn't edge-detect against our
            // pre-disable cache value and fire a spurious pulse.
            for c in self.caches.iter_mut() { *c = AnalyzerCache::default(); }
            self.display_name = input.name.clone();
            self.display_params = input.analyzer_param_defs();
            self.display_outputs = self.outputs.iter().enumerate()
                .map(|(i, p)| (p.name.clone(), p.port_type, self.output_values[i]))
                .collect();
            self.display_analyzer_results.clear();
            return;
        };
        // Drop the state lock before touching the frame mutex so the audio
        // worker and the engine tick can't deadlock each other.
        drop(state);
        let frame = frame_arc.lock().clone();

        // Slot analyzer values into the output buffer. For analyzers whose
        // first output is a trigger (Beat, AudioBeat), edge-detect from the
        // monotonic onset counter emitted by the worker.
        let mut slot = 0;
        for (ai, k) in self.cached_kinds.iter().enumerate() {
            let n = k.outputs().len();
            let is_trigger = matches!(k, AnalyzerKind::Beat | AnalyzerKind::Onset);
            let onset_pulse = if is_trigger {
                let cur = frame.onset_counts.get(ai).copied().unwrap_or(0);
                let prev = self.caches[ai].last_onset_count;
                self.caches[ai].last_onset_count = cur;
                if cur != prev && prev != 0 { 1.0 } else { 0.0 }
            } else { 0.0 };
            for j in 0..n {
                let v = frame.values.get(slot + j).copied().unwrap_or(0.0);
                let out = if is_trigger && j == 0 { onset_pulse } else { v };
                if let Some(s) = self.output_values.get_mut(slot + j) {
                    *s = out;
                }
            }
            slot += n;
        }

        // Snapshot for display (re-lock briefly to read name + params).
        let state = self.audio.lock().unwrap();
        let input = state.iter().find(|c| c.id == self.input_id);
        if let Some(input) = input {
            self.display_name = input.name.clone();
            self.display_params = input.analyzer_param_defs();
        }
        drop(state);

        self.display_outputs = self
            .outputs
            .iter()
            .enumerate()
            .map(|(i, p)| (p.name.clone(), p.port_type, self.output_values[i]))
            .collect();
        self.display_analyzer_results.clear();
        let mut o = 0;
        for k in &self.cached_kinds {
            let n = k.outputs().len();
            let vals: Vec<f32> = (o..o + n)
                .map(|i| self.output_values.get(i).copied().unwrap_or(0.0))
                .collect();
            self.display_analyzer_results.push((*k, vals));
            o += n;
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        self.output_values.get(pi).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        // Persist analyzer params by name (e.g. "a0.threshold") so the saved
        // values survive analyzer add/remove that shifts global indices.
        let mut params = serde_json::Map::new();
        for def in &self.display_params {
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

        // Apply saved param values by name. Looks up the analyzer params on
        // the bound audio input so we don't depend on `self.display_params`
        // (which may not be populated yet — process() hasn't run).
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
        self.display_params.clone()
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        let state = self.audio.lock().unwrap();
        if let Some(input) = state.iter().find(|c| c.id == self.input_id) {
            input.set_param(index, value);
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(AudioInputDisplay {
            input_id: self.input_id,
            input_name: self.display_name.clone(),
            outputs: self.display_outputs.clone(),
            analyzer_results: self.display_analyzer_results.clone(),
        }));
    }
}
