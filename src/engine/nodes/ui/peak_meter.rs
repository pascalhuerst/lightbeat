//! Display-only Peak Level Meter node — takes peak (input 0) and optional
//! RMS (input 1), both 0..1, and exposes nothing on the output side. The
//! widget renders a colored level bar with dB scale, peak hold and clip
//! indicator.

use crate::engine::types::*;

pub struct PeakMeterDisplay {
    pub name: String,
    pub peak: f32,
    pub rms: f32,
    pub orientation: PeakMeterOrientation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeakMeterOrientation {
    Vertical,
    Horizontal,
}

impl PeakMeterOrientation {
    pub fn from_str(s: &str) -> Self {
        match s {
            "horizontal" => PeakMeterOrientation::Horizontal,
            _ => PeakMeterOrientation::Vertical,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            PeakMeterOrientation::Vertical => "vertical",
            PeakMeterOrientation::Horizontal => "horizontal",
        }
    }
}

pub struct PeakMeterProcessNode {
    id: NodeId,
    name: String,
    inputs: Vec<PortDef>,
    peak: f32,
    rms: f32,
    orientation: PeakMeterOrientation,
}

impl PeakMeterProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            name: String::new(),
            inputs: vec![
                PortDef::new("peak", PortType::Untyped),
                PortDef::new("rms", PortType::Untyped),
            ],
            peak: 0.0,
            rms: 0.0,
            orientation: PeakMeterOrientation::Vertical,
        }
    }
}

impl ProcessNode for PeakMeterProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Peak Level Meter" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, pi: usize, v: f32) {
        // No clamp — let the widget see >= 1.0 to drive the clip indicator.
        match pi {
            0 => self.peak = v.max(0.0),
            1 => self.rms = v.max(0.0),
            _ => {}
        }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi { 0 => self.peak, 1 => self.rms, _ => 0.0 }
    }
    fn process(&mut self) {}

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "name": self.name,
            "orientation": self.orientation.as_str(),
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
            self.name = n.to_string();
        }
        if let Some(s) = data.get("orientation").and_then(|v| v.as_str()) {
            self.orientation = PeakMeterOrientation::from_str(s);
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(PeakMeterDisplay {
            name: self.name.clone(),
            peak: self.peak,
            rms: self.rms,
            orientation: self.orientation,
        }));
    }
}
