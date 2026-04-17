use crate::engine::nodes::ui::common::MouseOverrideMode;
use crate::engine::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonMode {
    Trigger,
    Toggle,
}

impl ButtonMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "toggle" => ButtonMode::Toggle,
            _ => ButtonMode::Trigger,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            ButtonMode::Trigger => "trigger",
            ButtonMode::Toggle => "toggle",
        }
    }
}

pub struct ButtonDisplay {
    pub label: String,
    pub mode: ButtonMode,
    /// Toggle state (only meaningful in Toggle mode).
    pub state: bool,
    pub input_value: f32,
    pub inputs_enabled: bool,
    pub override_enabled: bool,
    pub override_active: bool,
    pub reset_mode: MouseOverrideMode,
}

pub struct ButtonProcessNode {
    id: NodeId,
    label: String,
    mode: ButtonMode,
    /// Persistent toggle state (Toggle mode, no input).
    state: bool,
    /// Override state (Toggle + inputs + override): Some = held; None = follow input.
    override_state: Option<bool>,
    /// Monotonic click counter from widget — engine fires trigger when it changes.
    last_click_id: u64,
    /// For trigger mode: set by handle_click; consumed (cleared) on next process().
    trigger_pending: bool,
    /// Current Logic output (1.0 high, 0.0 low).
    output: f32,

    /// Input signal (Logic).
    input_value: f32,
    prev_input_value: f32,

    inputs_enabled: bool,
    override_enabled: bool,
    reset_mode: MouseOverrideMode,

    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl ButtonProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            label: "Button".to_string(),
            mode: ButtonMode::Trigger,
            state: false,
            override_state: None,
            last_click_id: 0,
            trigger_pending: false,
            output: 0.0,
            input_value: 0.0,
            prev_input_value: 0.0,
            inputs_enabled: false,
            override_enabled: false,
            reset_mode: MouseOverrideMode::ClearOnReset,
            inputs: Vec::new(),
            outputs: vec![PortDef::new("out", PortType::Logic)],
        }
    }

    fn rebuild_ports(&mut self) {
        self.inputs = if self.inputs_enabled {
            vec![PortDef::new("in", PortType::Logic)]
        } else {
            Vec::new()
        };
    }

    fn handle_click(&mut self) {
        match self.mode {
            ButtonMode::Trigger => { self.trigger_pending = true; }
            ButtonMode::Toggle => {
                if self.inputs_enabled && self.override_enabled {
                    // Toggle override state, starting from the input-derived state.
                    let cur = self.override_state.unwrap_or(self.input_value >= 0.5);
                    self.override_state = Some(!cur);
                } else {
                    self.state = !self.state;
                }
            }
        }
    }
}

impl ProcessNode for ButtonProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Button" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if port_index == 0 { self.input_value = value; }
    }
    fn read_input(&self, port_index: usize) -> f32 {
        if port_index == 0 { self.input_value } else { 0.0 }
    }

    fn process(&mut self) {
        let prev_input = self.prev_input_value;

        // Pass-through reset for override (Toggle mode only).
        if self.mode == ButtonMode::Toggle && self.inputs_enabled && self.override_enabled {
            if let Some(ov_bool) = self.override_state {
                let ov = if ov_bool { 1.0 } else { 0.0 };
                if self.reset_mode.should_clear(prev_input, self.input_value, ov) {
                    self.override_state = None;
                }
            }
        }
        self.prev_input_value = self.input_value;

        match self.mode {
            ButtonMode::Trigger => {
                let mut fire = self.trigger_pending;
                // Input rising edge also fires a trigger.
                if self.inputs_enabled && self.input_value >= 0.5 && prev_input < 0.5 {
                    fire = true;
                }
                self.output = if fire { 1.0 } else { 0.0 };
                self.trigger_pending = false;
            }
            ButtonMode::Toggle => {
                let on = if self.inputs_enabled {
                    self.override_state.unwrap_or(self.input_value >= 0.5)
                } else {
                    self.state
                };
                self.output = if on { 1.0 } else { 0.0 };
            }
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.output } else { 0.0 }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "label": self.label,
            "mode": self.mode.as_str(),
            "state": self.state,
            "inputs_enabled": self.inputs_enabled,
            "override_enabled": self.override_enabled,
            "reset_mode": self.reset_mode.as_str(),
            "override_state": self.override_state,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(label) = data.get("label").and_then(|v| v.as_str()) {
            self.label = label.to_string();
        }
        if let Some(mode_str) = data.get("mode").and_then(|v| v.as_str()) {
            let new_mode = ButtonMode::from_str(mode_str);
            if new_mode != self.mode {
                self.mode = new_mode;
                self.trigger_pending = false;
                self.output = 0.0;
            }
        }
        if let Some(state) = data.get("state").and_then(|v| v.as_bool()) {
            self.state = state;
        }
        let mut ports_dirty = false;
        if let Some(b) = data.get("inputs_enabled").and_then(|v| v.as_bool()) {
            if b != self.inputs_enabled { ports_dirty = true; }
            self.inputs_enabled = b;
        }
        if let Some(b) = data.get("override_enabled").and_then(|v| v.as_bool()) {
            self.override_enabled = b;
        }
        if let Some(s) = data.get("reset_mode").and_then(|v| v.as_str()) {
            self.reset_mode = MouseOverrideMode::from_str(s);
        }
        if let Some(ov) = data.get("override_state") {
            self.override_state = ov.as_bool();
        }
        if ports_dirty {
            self.rebuild_ports();
            if !self.inputs_enabled {
                self.override_state = None;
                self.input_value = 0.0;
                self.prev_input_value = 0.0;
            }
        }
        // Click event (incrementing click_id from widget).
        if let Some(click_id) = data.get("click_id").and_then(|v| v.as_u64()) {
            if click_id != self.last_click_id {
                self.last_click_id = click_id;
                self.handle_click();
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        let on = match self.mode {
            ButtonMode::Toggle => {
                if self.inputs_enabled {
                    self.override_state.unwrap_or(self.input_value >= 0.5)
                } else {
                    self.state
                }
            }
            ButtonMode::Trigger => false,
        };
        shared.display = Some(Box::new(ButtonDisplay {
            label: self.label.clone(),
            mode: self.mode,
            state: on,
            input_value: self.input_value,
            inputs_enabled: self.inputs_enabled,
            override_enabled: self.override_enabled,
            override_active: self.override_state.is_some(),
            reset_mode: self.reset_mode,
        }));
    }
}
