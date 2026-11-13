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

/// The order of the "Reset" dropdown; saved settings go by position.
const RESET_ORDER: [MouseOverrideMode; 3] = [
    MouseOverrideMode::ClearOnReset,
    MouseOverrideMode::PickupIncrease,
    MouseOverrideMode::PickupDecrease,
];

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

    fn set_mode(&mut self, mode: ButtonMode) {
        if mode != self.mode {
            self.mode = mode;
            self.trigger_pending = false;
            self.output = 0.0;
        }
    }

    /// The one place the input port appears or goes away, for both a changed
    /// param and a file saved before the setting was a param.
    fn set_inputs_enabled(&mut self, enabled: bool) {
        if enabled == self.inputs_enabled { return; }
        self.inputs_enabled = enabled;
        self.rebuild_ports();
        if !enabled {
            self.override_state = None;
            self.input_value = 0.0;
            self.prev_input_value = 0.0;
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
        if self.mode == ButtonMode::Toggle && self.inputs_enabled && self.override_enabled
            && let Some(ov_bool) = self.override_state {
                let ov = if ov_bool { 1.0 } else { 0.0 };
                if self.reset_mode.should_clear(prev_input, self.input_value, ov) {
                    self.override_state = None;
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

    fn non_promotable_params(&self) -> &'static [&'static str] {
        &["Input enabled"]
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Choice {
                // Not "Mode": a setting of that name counts as reshaping the
                // node, and the button's mode can be driven by a wire.
                name: "Button mode".into(),
                value: if self.mode == ButtonMode::Toggle { 1 } else { 0 },
                options: vec!["Trigger".into(), "Toggle".into()],
            },
            ParamDef::Bool { name: "Input enabled".into(), value: self.inputs_enabled },
            ParamDef::Bool { name: "Allow mouse override".into(), value: self.override_enabled },
            ParamDef::Choice {
                name: "Reset".into(),
                value: RESET_ORDER.iter().position(|m| *m == self.reset_mode).unwrap_or(0),
                options: RESET_ORDER.iter().map(|m| m.label().to_string()).collect(),
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.set_mode(if value.as_usize() == 1 { ButtonMode::Toggle } else { ButtonMode::Trigger }),
            1 => self.set_inputs_enabled(value.as_usize() != 0),
            2 => self.override_enabled = value.as_usize() != 0,
            3 => self.reset_mode = RESET_ORDER.get(value.as_usize()).copied().unwrap_or(MouseOverrideMode::ClearOnReset),
            _ => {}
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "label": self.label,
            "state": self.state,
            "override_state": self.override_state,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(label) = data.get("label").and_then(|v| v.as_str()) {
            self.label = label.to_string();
        }
        // Settings that were once kept here and are params now; only files
        // saved before that carry them.
        if let Some(mode_str) = data.get("mode").and_then(|v| v.as_str()) {
            self.set_mode(ButtonMode::from_str(mode_str));
        }
        if let Some(state) = data.get("state").and_then(|v| v.as_bool()) {
            self.state = state;
        }
        if let Some(b) = data.get("inputs_enabled").and_then(|v| v.as_bool()) {
            self.set_inputs_enabled(b);
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
        // Click event (incrementing click_id from widget).
        if let Some(click_id) = data.get("click_id").and_then(|v| v.as_u64())
            && click_id != self.last_click_id {
                self.last_click_id = click_id;
                self.handle_click();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn values(b: &ButtonProcessNode) -> Vec<String> {
        b.params().iter().map(|p| format!("{:?}", p.as_value())).collect()
    }

    #[test]
    fn a_button_saved_with_its_settings_in_the_config_comes_back_with_them() {
        let mut b = ButtonProcessNode::new(NodeId(1));
        b.load_data(&serde_json::json!({
            "label": "Go",
            "mode": "toggle",
            "state": true,
            "inputs_enabled": true,
            "override_enabled": true,
            "reset_mode": "pickup_decrease",
            "override_state": null,
        }));
        let p = b.params();
        assert!(matches!(&p[0], ParamDef::Choice { value: 1, .. }));
        assert!(matches!(&p[1], ParamDef::Bool { value: true, .. }));
        assert!(matches!(&p[2], ParamDef::Bool { value: true, .. }));
        assert!(matches!(&p[3], ParamDef::Choice { value: 2, .. }));
        assert_eq!(b.inputs().len(), 1);
        let out = b.save_data().unwrap();
        assert_eq!(out, serde_json::json!({ "label": "Go", "state": true, "override_state": null }));
    }

    #[test]
    fn the_new_form_round_trips_through_params() {
        let mut a = ButtonProcessNode::new(NodeId(1));
        a.set_param(0, ParamValue::Choice(1));
        a.set_param(1, ParamValue::Bool(true));
        a.set_param(2, ParamValue::Bool(true));
        a.set_param(3, ParamValue::Choice(1));
        a.load_data(&serde_json::json!({ "label": "Go", "state": true }));
        let saved = a.save_data().unwrap();
        let mut b = ButtonProcessNode::new(NodeId(2));
        b.load_data(&saved);
        for (i, p) in a.params().iter().enumerate() { b.set_param(i, p.as_value()); }
        assert_eq!(values(&a), values(&b));
        assert_eq!(b.save_data().unwrap(), saved);
        assert_eq!(b.inputs().len(), 1);
    }
}
