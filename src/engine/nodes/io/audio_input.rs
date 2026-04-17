use crate::audio::analyzers::{AnalyzerInstance, AnalyzerKind};
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
    /// Mirror of the analyzer params for the inspector.
    pub params: Vec<ParamDef>,
    /// (kind, output values for this analyzer). Lets the widget render
    /// per-kind mini visualisations (e.g. a level bar for PeakLevel).
    pub analyzer_results: Vec<(AnalyzerKind, Vec<f32>)>,
}

#[derive(Default, Clone, Copy)]
struct AnalyzerCache {
    last_onset_count: u64,
}

pub struct AudioInputProcessNode {
    id: NodeId,
    /// Bound audio input id (0 = none selected).
    input_id: u32,
    outputs: Vec<PortDef>,
    output_values: Vec<f32>,
    /// Per-analyzer cached state for edge detection on onset counters.
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
            id, input_id: 0,
            outputs: Vec::new(), output_values: Vec::new(),
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
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Audio Input" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn process(&mut self) {
        let state = self.audio.lock().unwrap();
        let input = state.iter().find(|c| c.id == self.input_id);
        let Some(input) = input else {
            for v in &mut self.output_values { *v = 0.0; }
            self.display_outputs.clear();
            self.display_name.clear();
            self.display_params.clear();
            self.display_analyzer_results.clear();
            self.outputs.clear();
            return;
        };

        // Build expected port layout from analyzers.
        let mut expected: Vec<PortDef> = Vec::new();
        for (i, a) in input.analyzers.iter().enumerate() {
            let prefix = format!("a{}.", i);
            for p in crate::audio::analyzers::AnalyzerInstance::outputs_for_kind(a.kind) {
                expected.push(PortDef::new(format!("{}{}", prefix, p.name), p.port_type));
            }
        }
        let layout_changed = expected.len() != self.outputs.len()
            || expected.iter().zip(self.outputs.iter())
                .any(|(a, b)| a.name != b.name || a.port_type != b.port_type);
        if layout_changed {
            self.outputs = expected;
            self.output_values = vec![0.0; self.outputs.len()];
            self.caches = vec![AnalyzerCache::default(); input.analyzers.len()];
        }
        if self.caches.len() != input.analyzers.len() {
            self.caches = vec![AnalyzerCache::default(); input.analyzers.len()];
        }

        // Read each analyzer's outputs and slot them into output_values.
        // For analyzers whose first output is a trigger (Beat), edge-detect
        // from the onset counter. Continuous-only analyzers (PeakLevel) pass
        // their values through unchanged.
        let mut idx = 0;
        for (ai, a) in input.analyzers.iter().enumerate() {
            let raw = a.read_outputs();
            let trigger_at_zero = a.first_output_is_trigger();
            let onset_pulse = if trigger_at_zero {
                let cur_onset = a.onset_count();
                let prev_onset = self.caches[ai].last_onset_count;
                self.caches[ai].last_onset_count = cur_onset;
                if cur_onset != prev_onset && prev_onset != 0 { 1.0 } else { 0.0 }
            } else {
                0.0
            };

            for (j, mut v) in raw.into_iter().enumerate() {
                if trigger_at_zero && j == 0 { v = onset_pulse; }
                if let Some(slot) = self.output_values.get_mut(idx) {
                    *slot = v;
                }
                idx += 1;
            }
        }

        // Snapshot for display.
        self.display_name = input.name.clone();
        self.display_outputs = self.outputs.iter().enumerate()
            .map(|(i, p)| (p.name.clone(), p.port_type, self.output_values[i]))
            .collect();
        self.display_params = input.analyzer_param_defs();

        // Per-analyzer kind + values, for kind-specific mini visualisations.
        // Slice output_values back into per-analyzer chunks.
        self.display_analyzer_results.clear();
        let mut o = 0;
        for a in &input.analyzers {
            let n = AnalyzerInstance::outputs_for_kind(a.kind).len();
            let vals: Vec<f32> = (o..o + n).map(|i| self.output_values.get(i).copied().unwrap_or(0.0)).collect();
            self.display_analyzer_results.push((a.kind, vals));
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
        }

        // Apply saved param values by name. Looks up the analyzer params on
        // the bound audio input so we don't depend on `self.display_params`
        // (which may not be populated yet — process() hasn't run).
        if let Some(saved) = data.get("params").and_then(|v| v.as_object()) {
            let state = self.audio.lock().unwrap();
            if let Some(input) = state.iter().find(|c| c.id == self.input_id) {
                let defs = input.analyzer_param_defs();
                for (name, value) in saved {
                    if let Some(idx) = defs.iter().position(|d| d.name() == name) {
                        if let Some(pv) = json_to_param_value(value, &defs[idx]) {
                            input.set_param(idx, pv);
                        }
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
            params: self.display_params.clone(),
            analyzer_results: self.display_analyzer_results.clone(),
        }));
        // Also surface params via current_params so the standard inspector
        // renders them. Engine main loop overwrites `current_params` from
        // `node.params()` already, so this is redundant — but mirror once
        // in case ordering ever changes.
    }
}
