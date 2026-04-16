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
    pub state: bool,
}

pub struct ButtonProcessNode {
    id: NodeId,
    label: String,
    mode: ButtonMode,
    /// For toggle mode: persistent state.
    state: bool,
    /// Monotonic click counter from widget — engine fires when it changes.
    last_click_id: u64,
    /// For trigger mode: set by handle_click; consumed (cleared) on next process().
    trigger_pending: bool,
    /// Current Logic output (1.0 high, 0.0 low).
    output: f32,
    outputs: Vec<PortDef>,
}

impl ButtonProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            label: "Button".to_string(),
            mode: ButtonMode::Trigger,
            state: false,
            last_click_id: 0,
            trigger_pending: false,
            output: 0.0,
            outputs: vec![PortDef::new("out", PortType::Logic)],
        }
    }

    fn handle_click(&mut self) {
        match self.mode {
            ButtonMode::Trigger => { self.trigger_pending = true; }
            ButtonMode::Toggle => { self.state = !self.state; }
        }
    }
}

impl ProcessNode for ButtonProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Button" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn process(&mut self) {
        match self.mode {
            ButtonMode::Trigger => {
                // Trigger fires for exactly one tick after a click.
                self.output = if self.trigger_pending { 1.0 } else { 0.0 };
                self.trigger_pending = false;
            }
            ButtonMode::Toggle => {
                self.output = if self.state { 1.0 } else { 0.0 };
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
        // Click event (incrementing click_id from widget).
        if let Some(click_id) = data.get("click_id").and_then(|v| v.as_u64()) {
            if click_id != self.last_click_id {
                self.last_click_id = click_id;
                self.handle_click();
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(ButtonDisplay {
            label: self.label.clone(),
            mode: self.mode,
            state: self.state,
        }));
    }
}
