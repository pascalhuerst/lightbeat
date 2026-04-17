use crate::engine::nodes::ui::common::MouseOverrideMode;
use crate::engine::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaderOrientation {
    Vertical,
    Horizontal,
}

impl FaderOrientation {
    pub fn from_str(s: &str) -> Self {
        match s {
            "horizontal" => FaderOrientation::Horizontal,
            _ => FaderOrientation::Vertical,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            FaderOrientation::Vertical => "vertical",
            FaderOrientation::Horizontal => "horizontal",
        }
    }
}

pub struct FaderDisplay {
    pub orientation: FaderOrientation,
    /// Currently-emitted value.
    pub output: f32,
    /// Input signal value (only meaningful when inputs_enabled).
    pub input: f32,
    pub inputs_enabled: bool,
    pub mouse_override: MouseOverrideMode,
    pub override_active: bool,
    pub override_value: f32,
    pub bipolar: bool,
}

pub struct FaderProcessNode {
    id: NodeId,
    orientation: FaderOrientation,
    /// Local mouse-driven value when inputs are disabled (legacy behavior).
    mouse_value: f32,
    /// Incoming input signal value.
    input_value: f32,
    prev_input_value: f32,
    /// When Some, user has overridden the input-driven value.
    override_value: Option<f32>,
    /// Computed output each tick.
    output: f32,

    inputs_enabled: bool,
    mouse_override: MouseOverrideMode,
    bipolar: bool,

    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl FaderProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            orientation: FaderOrientation::Vertical,
            mouse_value: 0.0,
            input_value: 0.0,
            prev_input_value: 0.0,
            override_value: None,
            output: 0.0,
            inputs_enabled: false,
            mouse_override: MouseOverrideMode::No,
            bipolar: false,
            inputs: Vec::new(),
            outputs: vec![PortDef::new("out", PortType::Untyped)],
        }
    }

    fn rebuild_ports(&mut self) {
        self.inputs = if self.inputs_enabled {
            vec![PortDef::new("in", PortType::Untyped)]
        } else {
            Vec::new()
        };
    }
}

impl ProcessNode for FaderProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Fader" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if port_index == 0 { self.input_value = value; }
    }

    fn read_input(&self, port_index: usize) -> f32 {
        if port_index == 0 { self.input_value } else { 0.0 }
    }

    fn process(&mut self) {
        // Pickup-style reset: if override active and input crosses it, clear.
        if self.inputs_enabled && self.mouse_override.allows_override() {
            if let Some(ov) = self.override_value {
                if self.mouse_override.should_clear(self.prev_input_value, self.input_value, ov) {
                    self.override_value = None;
                }
            }
        }
        self.prev_input_value = self.input_value;

        self.output = if !self.inputs_enabled {
            self.mouse_value
        } else if let Some(ov) = self.override_value {
            ov
        } else {
            self.input_value
        };
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.output } else { 0.0 }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "orientation": self.orientation.as_str(),
            "mouse_value": self.mouse_value,
            "inputs_enabled": self.inputs_enabled,
            "mouse_override": self.mouse_override.as_str(),
            "bipolar": self.bipolar,
            "override_value": self.override_value,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(s) = data.get("orientation").and_then(|v| v.as_str()) {
            self.orientation = FaderOrientation::from_str(s);
        }
        if let Some(v) = data.get("mouse_value").and_then(|v| v.as_f64()) {
            self.mouse_value = (v as f32).clamp(0.0, 1.0);
        }
        // Legacy single-field save ("value") still respected.
        if let Some(v) = data.get("value").and_then(|v| v.as_f64()) {
            self.mouse_value = (v as f32).clamp(0.0, 1.0);
        }
        let mut ports_dirty = false;
        if let Some(b) = data.get("inputs_enabled").and_then(|v| v.as_bool()) {
            if b != self.inputs_enabled { ports_dirty = true; }
            self.inputs_enabled = b;
        }
        if let Some(s) = data.get("mouse_override").and_then(|v| v.as_str()) {
            self.mouse_override = MouseOverrideMode::from_str(s);
        } else {
            // Backward-compat: collapse old (override_enabled + reset_mode) → new enum.
            let override_enabled = data.get("override_enabled").and_then(|v| v.as_bool()).unwrap_or(false);
            let reset_mode = data.get("reset_mode").and_then(|v| v.as_str()).unwrap_or("on_reset");
            self.mouse_override = if !override_enabled {
                MouseOverrideMode::No
            } else {
                match reset_mode {
                    "pass_from_below" => MouseOverrideMode::PickupIncrease,
                    "pass_from_above" => MouseOverrideMode::PickupDecrease,
                    _ => MouseOverrideMode::ClearOnReset,
                }
            };
        }
        if let Some(b) = data.get("bipolar").and_then(|v| v.as_bool()) {
            self.bipolar = b;
        }
        // Override value: null/missing -> None; number -> Some(v).
        if let Some(ov) = data.get("override_value") {
            self.override_value = ov.as_f64().map(|f| (f as f32).clamp(0.0, 1.0));
        }
        if ports_dirty {
            self.rebuild_ports();
            if !self.inputs_enabled {
                self.override_value = None;
                self.input_value = 0.0;
                self.prev_input_value = 0.0;
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(FaderDisplay {
            orientation: self.orientation,
            output: self.output,
            input: self.input_value,
            inputs_enabled: self.inputs_enabled,
            mouse_override: self.mouse_override,
            override_active: self.override_value.is_some(),
            override_value: self.override_value.unwrap_or(0.0),
            bipolar: self.bipolar,
        }));
    }
}
