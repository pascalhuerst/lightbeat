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
    pub value: f32,
}

pub struct FaderProcessNode {
    id: NodeId,
    orientation: FaderOrientation,
    value: f32,
    outputs: Vec<PortDef>,
}

impl FaderProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            orientation: FaderOrientation::Vertical,
            value: 0.0,
            outputs: vec![PortDef::new("out", PortType::Untyped)],
        }
    }
}

impl ProcessNode for FaderProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Fader" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn process(&mut self) {}

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.value } else { 0.0 }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "orientation": self.orientation.as_str(),
            "value": self.value,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(s) = data.get("orientation").and_then(|v| v.as_str()) {
            self.orientation = FaderOrientation::from_str(s);
        }
        if let Some(v) = data.get("value").and_then(|v| v.as_f64()) {
            self.value = (v as f32).clamp(0.0, 1.0);
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(FaderDisplay {
            orientation: self.orientation,
            value: self.value,
        }));
    }
}
