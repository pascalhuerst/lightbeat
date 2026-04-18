use crate::engine::types::*;
use crate::input_controller::{InputBindingMode, SharedControllers};

/// Display state mirrored to the widget.
pub struct InputControllerDisplay {
    pub controller_id: u32,
    pub controller_name: String,
    /// Per-output (name, port_type, current value) — UI uses this to label
    /// outputs and show meters. Order matches ProcessNode outputs.
    pub outputs: Vec<(String, PortType, f32)>,
    /// Per-input (name, port_type) — for kinds that support graph → device
    /// feedback (BCF2000). Empty for input-only kinds. Order matches
    /// ProcessNode inputs.
    pub feedback_inputs: Vec<(String, PortType)>,
}

/// Per-input cached state for trigger detection.
#[derive(Default, Clone, Copy)]
struct InputCache {
    prev_value: f32,
}

pub struct InputControllerProcessNode {
    id: NodeId,
    /// Bound controller id (0 = none selected).
    controller_id: u32,
    /// Output port layout. Index 0 is always the "any change" trigger port,
    /// followed by one port per learned input.
    outputs: Vec<PortDef>,
    /// Input port layout — only populated for feedback-capable controllers
    /// (BCF2000). Order matches the learned inputs; writes are forwarded to
    /// `ControllerRuntime.out_values` which the MIDI feedback worker sends
    /// back to the device.
    inputs: Vec<PortDef>,
    /// Pending graph → device values, mirrored from `write_input` into
    /// shared `out_values` every tick.
    input_values: Vec<f32>,
    /// For each feedback input port, the index into `c.inputs` (i.e. into
    /// the controller's mapping list) it corresponds to. Mappings with
    /// `disable_feedback = true` are omitted, so this is shorter than
    /// `c.inputs` whenever the user has hidden some.
    feedback_input_mapping_idx: Vec<usize>,
    /// Output values for the learned inputs (parallel to inputs[]).
    output_values: Vec<f32>,
    /// Output values from the previous tick — used to detect changes.
    prev_output_values: Vec<f32>,
    /// True for one tick whenever any output value changed.
    any_changed: bool,
    /// Cached previous values for trigger-edge detection.
    cache: Vec<InputCache>,
    /// Snapshot of current display info (for `update_display`).
    /// Includes the "any change" entry as the first element.
    display_outputs: Vec<(String, PortType, f32)>,
    display_feedback_inputs: Vec<(String, PortType)>,
    display_name: String,
    /// Shared input controller state.
    controllers: SharedControllers,
    /// Whether the bound controller supports graph → device feedback.
    /// Updated each tick. Drives the presence of input ports.
    has_feedback: bool,
}

impl InputControllerProcessNode {
    pub fn new(id: NodeId, controllers: SharedControllers) -> Self {
        Self {
            id,
            controller_id: 0,
            outputs: vec![PortDef::new("any change", PortType::Logic)],
            inputs: Vec::new(),
            input_values: Vec::new(),
            feedback_input_mapping_idx: Vec::new(),
            output_values: Vec::new(),
            prev_output_values: Vec::new(),
            any_changed: false,
            cache: Vec::new(),
            display_outputs: Vec::new(),
            display_feedback_inputs: Vec::new(),
            display_name: String::new(),
            controllers,
            has_feedback: false,
        }
    }
}

impl ProcessNode for InputControllerProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Input Controller" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if let Some(slot) = self.input_values.get_mut(port_index) {
            *slot = value;
        }
    }

    fn process(&mut self) {
        let mut state = self.controllers.lock().unwrap();
        let c = state.iter_mut().find(|c| c.id == self.controller_id);
        let Some(c) = c else {
            for v in &mut self.output_values { *v = 0.0; }
            self.any_changed = false;
            self.prev_output_values.clear();
            self.display_outputs.clear();
            self.display_feedback_inputs.clear();
            self.display_name.clear();
            if self.outputs.len() != 1 || self.outputs[0].name != "any change" {
                self.outputs = vec![PortDef::new("any change", PortType::Logic)];
            }
            self.inputs.clear();
            self.input_values.clear();
            self.feedback_input_mapping_idx.clear();
            self.has_feedback = false;
            return;
        };

        self.has_feedback = c.kind.has_feedback();

        // Rebuild output port layout if the input set changed.
        let n = c.inputs.len();
        let expected_out_len = n + 1;
        let out_layout_changed = self.outputs.len() != expected_out_len
            || self.outputs.iter().skip(1).zip(c.inputs.iter())
                .any(|(p, i)| p.name != i.name || p.port_type != port_type_for(&i.source));
        if out_layout_changed {
            self.outputs = std::iter::once(PortDef::new("any change", PortType::Logic))
                .chain(c.inputs.iter().map(|i| {
                    PortDef::new(i.name.clone(), port_type_for(&i.source))
                }))
                .collect();
            self.output_values = vec![0.0; n];
            self.prev_output_values = vec![0.0; n];
            self.cache = vec![InputCache::default(); n];
        }

        // Feedback inputs: only expose a port per mapping where feedback is
        // NOT disabled; skip mappings the user has silenced.
        let desired_mapping: Vec<usize> = if self.has_feedback {
            c.inputs.iter().enumerate()
                .filter(|(_, i)| !i.disable_feedback)
                .map(|(idx, _)| idx)
                .collect()
        } else {
            Vec::new()
        };
        let in_layout_changed = desired_mapping != self.feedback_input_mapping_idx
            || self.inputs.len() != desired_mapping.len()
            || self.inputs.iter().zip(desired_mapping.iter())
                .any(|(p, &idx)| {
                    let i = &c.inputs[idx];
                    p.name != i.name || p.port_type != port_type_for(&i.source)
                });
        if in_layout_changed {
            self.inputs = desired_mapping.iter().map(|&idx| {
                let i = &c.inputs[idx];
                PortDef::new(i.name.clone(), port_type_for(&i.source))
            }).collect();
            self.input_values = vec![0.0; desired_mapping.len()];
            self.feedback_input_mapping_idx = desired_mapping;
        }

        // Forward pending graph → device values into shared out_values so
        // the MIDI feedback worker thread picks them up. Only for mappings
        // that have a feedback port wired, and only when the debug panel
        // hasn't taken manual control. Mappings with `disable_feedback` are
        // skipped entirely — the worker won't re-emit for them either.
        if self.has_feedback && !c.debug_feedback_override {
            for (port_idx, &mapping_idx) in self.feedback_input_mapping_idx.iter().enumerate() {
                if let (Some(v), Some(slot)) = (
                    self.input_values.get(port_idx),
                    c.out_values.get_mut(mapping_idx),
                ) {
                    *slot = *v;
                }
            }
        }

        // Compute outputs by mode.
        for (idx, input) in c.inputs.iter().enumerate() {
            let raw = c.values.get(idx).copied().unwrap_or(0.0);
            let prev = self.cache[idx].prev_value;
            let out = match input.mode {
                InputBindingMode::Value => raw,
                InputBindingMode::TriggerOnPress => {
                    if raw > 0.5 && prev <= 0.5 { 1.0 } else { 0.0 }
                }
                InputBindingMode::TriggerOnRelease => {
                    if raw <= 0.5 && prev > 0.5 { 1.0 } else { 0.0 }
                }
            };
            self.output_values[idx] = out;
            self.cache[idx].prev_value = raw;
        }

        self.any_changed = self.output_values.iter()
            .zip(self.prev_output_values.iter())
            .any(|(cur, prev)| cur != prev);
        self.prev_output_values.copy_from_slice(&self.output_values);

        self.display_name = c.name.clone();
        self.display_outputs = std::iter::once((
            "any change".to_string(),
            PortType::Logic,
            if self.any_changed { 1.0 } else { 0.0 },
        )).chain(c.inputs.iter().enumerate().map(|(i, input)| {
            (input.name.clone(), port_type_for(&input.source), self.output_values[i])
        })).collect();
        self.display_feedback_inputs = self.feedback_input_mapping_idx.iter().map(|&idx| {
            let i = &c.inputs[idx];
            (i.name.clone(), port_type_for(&i.source))
        }).collect();
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 {
            if self.any_changed { 1.0 } else { 0.0 }
        } else {
            self.output_values.get(pi - 1).copied().unwrap_or(0.0)
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "controller_id": self.controller_id,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(id) = data.get("controller_id").and_then(|v| v.as_u64()) {
            self.controller_id = id as u32;
            self.outputs.clear();
            self.inputs.clear();
            self.input_values.clear();
            self.output_values.clear();
            self.cache.clear();
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(InputControllerDisplay {
            controller_id: self.controller_id,
            controller_name: self.display_name.clone(),
            outputs: self.display_outputs.clone(),
            feedback_inputs: self.display_feedback_inputs.clone(),
        }));
    }
}

fn port_type_for(source: &crate::input_controller::InputSource) -> PortType {
    if source.is_binary() { PortType::Logic } else { PortType::Untyped }
}
