use crate::engine::types::*;
use crate::input_controller::{InputBindingMode, SharedControllers};

/// Display state mirrored to the widget.
pub struct InputControllerDisplay {
    pub controller_id: u32,
    pub controller_name: String,
    /// Per-output (name, port_type, current value) — UI uses this to label
    /// outputs and show meters. Order matches ProcessNode outputs.
    pub outputs: Vec<(String, PortType, f32)>,
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
    /// Resolved per-output info, refreshed each tick from shared state.
    outputs: Vec<PortDef>,
    /// Output values, one per port.
    output_values: Vec<f32>,
    /// Cached previous values for trigger-edge detection.
    cache: Vec<InputCache>,
    /// Snapshot of current display info (for `update_display`).
    display_outputs: Vec<(String, PortType, f32)>,
    display_name: String,
    /// Shared input controller state.
    controllers: SharedControllers,
}

impl InputControllerProcessNode {
    pub fn new(id: NodeId, controllers: SharedControllers) -> Self {
        Self {
            id,
            controller_id: 0,
            outputs: Vec::new(),
            output_values: Vec::new(),
            cache: Vec::new(),
            display_outputs: Vec::new(),
            display_name: String::new(),
            controllers,
        }
    }
}

impl ProcessNode for InputControllerProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Input Controller" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn process(&mut self) {
        let state = self.controllers.lock().unwrap();
        let c = state.iter().find(|c| c.id == self.controller_id);
        let Some(c) = c else {
            // No bound controller — clear outputs.
            for v in &mut self.output_values { *v = 0.0; }
            self.display_outputs.clear();
            self.display_name.clear();
            return;
        };

        // Rebuild port layout if the input set changed.
        let n = c.inputs.len();
        if self.outputs.len() != n
            || self.outputs.iter().zip(c.inputs.iter())
                .any(|(p, i)| p.name != i.name || p.port_type != port_type_for(&i.source))
        {
            self.outputs = c.inputs.iter().map(|i| {
                PortDef::new(i.name.clone(), port_type_for(&i.source))
            }).collect();
            self.output_values = vec![0.0; n];
            self.cache = vec![InputCache::default(); n];
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

        self.display_name = c.name.clone();
        self.display_outputs = c.inputs.iter().enumerate().map(|(i, input)| {
            (input.name.clone(), port_type_for(&input.source), self.output_values[i])
        }).collect();
    }

    fn read_output(&self, pi: usize) -> f32 {
        self.output_values.get(pi).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "controller_id": self.controller_id,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(id) = data.get("controller_id").and_then(|v| v.as_u64()) {
            self.controller_id = id as u32;
            // Force port rebuild on next process tick by clearing.
            self.outputs.clear();
            self.output_values.clear();
            self.cache.clear();
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(InputControllerDisplay {
            controller_id: self.controller_id,
            controller_name: self.display_name.clone(),
            outputs: self.display_outputs.clone(),
        }));
    }
}

fn port_type_for(source: &crate::input_controller::InputSource) -> PortType {
    if source.is_binary() { PortType::Logic } else { PortType::Untyped }
}
