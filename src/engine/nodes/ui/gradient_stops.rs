//! Gradient Stops: a UI control that emits four 0..1 positions for the
//! Palette → Gradient node. Optional `palette` input drives a live preview
//! in the widget (colours for each handle + interpolated strip background).

use crate::engine::param_spec::FloatParam;
use crate::engine::types::*;

const POSITIONS: [FloatParam; 4] = [
    FloatParam::new("pos1", 0.0, 1.0, 0.01, "").clamped(),
    FloatParam::new("pos2", 0.0, 1.0, 0.01, "").clamped(),
    FloatParam::new("pos3", 0.0, 1.0, 0.01, "").clamped(),
    FloatParam::new("pos4", 0.0, 1.0, 0.01, "").clamped(),
];

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

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports() -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![PortDef::new("palette", PortType::Palette)],
        vec![
            PortDef::new("pos1", PortType::Untyped),
            PortDef::new("pos2", PortType::Untyped),
            PortDef::new("pos3", PortType::Untyped),
            PortDef::new("pos4", PortType::Untyped),
        ],
    )
}

impl GradientStopsProcessNode {
    pub fn new(id: NodeId) -> Self {
        let (inputs, outputs) = ports();
        Self {
            id,
            name: String::new(),
            // Even distribution by default.
            positions: [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0],
            palette: [0.0; 12],
            inputs,
            outputs,
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
        Some(serde_json::json!({ "name": self.name }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
            self.name = n.to_string();
        }
    }

    fn non_promotable_params(&self) -> &'static [&'static str] {
        &["pos1", "pos2", "pos3", "pos4"]
    }

    fn params(&self) -> Vec<ParamDef> {
        POSITIONS.iter().zip(self.positions).map(|(spec, v)| spec.def(v)).collect()
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index < 4 {
            self.positions[index] = POSITIONS[index].apply(&value);
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
