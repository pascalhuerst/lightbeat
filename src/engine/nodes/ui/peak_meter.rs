//! Display-only Peak Level Meter node — takes peak (input 0) and optional
//! RMS (input 1), both 0..1, and exposes nothing on the output side. The
//! widget renders a colored level bar with dB scale, peak hold and clip
//! indicator.

use serde::{Deserialize, Serialize};

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

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports() -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![
            PortDef::new("peak", PortType::Untyped),
            PortDef::new("rms", PortType::Untyped),
        ],
        vec![],
    )
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

/// The node's own settings, as saved and as the widget pushes them.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PeakMeterConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    orientation: Option<String>,
}

impl PeakMeterConfig {
    pub fn new(name: &str, orientation: PeakMeterOrientation) -> Self {
        Self { name: Some(name.to_string()), orientation: Some(orientation.as_str().to_string()) }
    }

    /// Malformed data loads as an empty update, which changes nothing.
    pub fn from_value(v: &serde_json::Value) -> Self {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
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
        let (inputs, _) = ports();
        Self {
            id,
            name: String::new(),
            inputs,
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
        Some(PeakMeterConfig::new(&self.name, self.orientation).to_value())
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        let cfg = PeakMeterConfig::from_value(data);
        if let Some(n) = cfg.name {
            self.name = n;
        }
        if let Some(s) = cfg.orientation {
            self.orientation = PeakMeterOrientation::from_str(&s);
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
