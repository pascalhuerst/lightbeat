//! Gradient Stops: a UI control that emits four 0..1 positions for the
//! Palette → Gradient node. Optional `palette` input drives a live preview
//! in the widget (colours for each handle + interpolated strip background).

use crate::engine::types::*;

pub struct GradientStopsDisplay {
    pub name: String,
    pub positions: [f32; 4],
    /// Mirrored palette input values (12 floats = 4 × RGB). Displayed by the
    /// widget to colour the handles + gradient-preview strip.
    pub palette: [f32; 12],
}

pub struct GradientStopsProcessNode {
    id: NodeId,
    name: String,
    positions: [f32; 4],
    palette: [f32; 12],
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl GradientStopsProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            name: String::new(),
            // Even distribution by default.
            positions: [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0],
            palette: [0.0; 12],
            inputs: vec![PortDef::new("palette", PortType::Palette)],
            outputs: vec![
                PortDef::new("pos1", PortType::Untyped),
                PortDef::new("pos2", PortType::Untyped),
                PortDef::new("pos3", PortType::Untyped),
                PortDef::new("pos4", PortType::Untyped),
            ],
        }
    }
}

impl ProcessNode for GradientStopsProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Gradient Stops" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, channel: usize, value: f32) {
        if channel < 12 { self.palette[channel] = value; }
    }

    fn read_input(&self, channel: usize) -> f32 {
        if channel < 12 { self.palette[channel] } else { 0.0 }
    }

    fn process(&mut self) {
        // Outputs are just the stored positions; nothing to compute.
    }

    fn read_output(&self, channel: usize) -> f32 {
        self.positions.get(channel).copied().unwrap_or(0.0).clamp(0.0, 1.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "name": self.name,
            "positions": self.positions,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
            self.name = n.to_string();
        }
        if let Some(arr) = data.get("positions").and_then(|v| v.as_array()) {
            for (i, v) in arr.iter().take(4).enumerate() {
                if let Some(f) = v.as_f64() {
                    self.positions[i] = (f as f32).clamp(0.0, 1.0);
                }
            }
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float { name: "pos1".into(), value: self.positions[0], min: 0.0, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Float { name: "pos2".into(), value: self.positions[1], min: 0.0, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Float { name: "pos3".into(), value: self.positions[2], min: 0.0, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Float { name: "pos4".into(), value: self.positions[3], min: 0.0, max: 1.0, step: 0.01, unit: "" },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index < 4 {
            self.positions[index] = value.as_f32().clamp(0.0, 1.0);
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(GradientStopsDisplay {
            name: self.name.clone(),
            positions: self.positions,
            palette: self.palette,
        }));
    }
}
