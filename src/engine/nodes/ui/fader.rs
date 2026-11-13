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
}

/// The order of the "Mouse override" dropdown; saved settings go by position.
pub const MOUSE_OVERRIDE_ORDER: [MouseOverrideMode; 4] = [
    MouseOverrideMode::No,
    MouseOverrideMode::ClearOnReset,
    MouseOverrideMode::PickupDecrease,
    MouseOverrideMode::PickupIncrease,
];

pub struct FaderDisplay {
    pub name: String,
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
    name: String,
    orientation: FaderOrientation,
    /// Local mouse-driven value when inputs are disabled.
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
            name: String::new(),
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

    /// The one place the input port appears or goes away, for both a changed
    /// param and a file saved before the setting was a param.
    fn set_inputs_enabled(&mut self, enabled: bool) {
        if enabled == self.inputs_enabled { return; }
        self.inputs_enabled = enabled;
        self.rebuild_ports();
        if !enabled {
            self.override_value = None;
            self.input_value = 0.0;
            self.prev_input_value = 0.0;
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
        if self.inputs_enabled && self.mouse_override.allows_override()
            && let Some(ov) = self.override_value
                && self.mouse_override.should_clear(self.prev_input_value, self.input_value, ov) {
                    self.override_value = None;
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

    fn non_promotable_params(&self) -> &'static [&'static str] {
        &["Input enabled"]
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Choice {
                name: "Orientation".into(),
                value: if self.orientation == FaderOrientation::Horizontal { 1 } else { 0 },
                options: vec!["Vertical".into(), "Horizontal".into()],
            },
            ParamDef::Bool { name: "Input enabled".into(), value: self.inputs_enabled },
            ParamDef::Choice {
                name: "Mouse override".into(),
                value: MOUSE_OVERRIDE_ORDER.iter().position(|m| *m == self.mouse_override).unwrap_or(0),
                options: MOUSE_OVERRIDE_ORDER.iter().map(|m| m.label().to_string()).collect(),
            },
            ParamDef::Bool { name: "Bipolar".into(), value: self.bipolar },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => {
                self.orientation = if value.as_usize() == 1 {
                    FaderOrientation::Horizontal
                } else {
                    FaderOrientation::Vertical
                };
            }
            1 => self.set_inputs_enabled(value.as_usize() != 0),
            2 => {
                self.mouse_override = MOUSE_OVERRIDE_ORDER
                    .get(value.as_usize())
                    .copied()
                    .unwrap_or(MouseOverrideMode::No);
            }
            3 => self.bipolar = value.as_usize() != 0,
            _ => {}
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "name": self.name,
            "mouse_value": self.mouse_value,
            "override_value": self.override_value,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
            self.name = n.to_string();
        }
        // Settings that were once kept here and are params now; only files
        // saved before that carry them.
        if let Some(s) = data.get("orientation").and_then(|v| v.as_str()) {
            self.orientation = FaderOrientation::from_str(s);
        }
        if let Some(v) = data.get("mouse_value").and_then(|v| v.as_f64()) {
            self.mouse_value = (v as f32).clamp(0.0, 1.0);
        }
        if let Some(b) = data.get("inputs_enabled").and_then(|v| v.as_bool()) {
            self.set_inputs_enabled(b);
        }
        if let Some(s) = data.get("mouse_override").and_then(|v| v.as_str()) {
            self.mouse_override = MouseOverrideMode::from_str(s);
        }
        if let Some(b) = data.get("bipolar").and_then(|v| v.as_bool()) {
            self.bipolar = b;
        }
        // Override value: null/missing -> None; number -> Some(v).
        if let Some(ov) = data.get("override_value") {
            self.override_value = ov.as_f64().map(|f| (f as f32).clamp(0.0, 1.0));
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(FaderDisplay {
            name: self.name.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_round_trip() {
        let mut a = FaderProcessNode::new(NodeId(1));
        a.load_data(&serde_json::json!({ "name": "Master", "mouse_value": 0.25, "override_value": 0.75 }));
        a.set_param(0, ParamValue::Choice(1));
        a.set_param(1, ParamValue::Bool(true));
        a.set_param(2, ParamValue::Choice(3));
        a.set_param(3, ParamValue::Bool(true));
        let saved = a.save_data().unwrap();
        assert_eq!(saved, serde_json::json!({ "name": "Master", "mouse_value": 0.25, "override_value": 0.75 }));
        // The settings travel as params, in the order the node lists them.
        let params = a.params();
        let mut b = FaderProcessNode::new(NodeId(2));
        b.load_data(&saved);
        for (i, p) in params.iter().enumerate() { b.set_param(i, p.as_value()); }
        assert_eq!(b.save_data().unwrap(), saved);
        assert_eq!(b.params().iter().map(|p| format!("{:?}", p.as_value())).collect::<Vec<_>>(),
                   params.iter().map(|p| format!("{:?}", p.as_value())).collect::<Vec<_>>());
        assert_eq!(b.inputs().len(), 1);
    }

    #[test]
    fn a_fader_saved_with_its_settings_in_the_config_comes_back_with_them() {
        let mut f = FaderProcessNode::new(NodeId(1));
        f.load_data(&serde_json::json!({
            "name": "Master",
            "orientation": "horizontal",
            "mouse_value": 0.25,
            "inputs_enabled": true,
            "mouse_override": "pickup_increase",
            "bipolar": true,
            "override_value": 0.75,
        }));
        let p = f.params();
        assert!(matches!(&p[0], ParamDef::Choice { value: 1, .. }));
        assert!(matches!(&p[1], ParamDef::Bool { value: true, .. }));
        assert!(matches!(&p[2], ParamDef::Choice { value: 3, .. }));
        assert!(matches!(&p[3], ParamDef::Bool { value: true, .. }));
        assert_eq!(f.inputs().len(), 1);
        // Written back out, the old keys are gone.
        let out = f.save_data().unwrap();
        assert!(out.get("orientation").is_none() && out.get("bipolar").is_none());
        assert_eq!(out["override_value"], serde_json::json!(0.75));
    }
}
