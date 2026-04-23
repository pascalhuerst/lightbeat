//! XY Pad: a draggable point inside a unit square, outputting its x and y
//! coordinates (each clamped 0..=1).

use crate::engine::types::*;

/// Display snapshot consumed by the widget each frame.
pub struct XyPadDisplay {
    pub name: String,
    pub x: f32,
    pub y: f32,
}

pub struct XyPadProcessNode {
    id: NodeId,
    name: String,
    /// Current point position in the unit square (both clamped 0..=1).
    x: f32,
    y: f32,
    outputs: Vec<PortDef>,
}

impl XyPadProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            name: String::new(),
            x: 0.5,
            y: 0.5,
            outputs: vec![
                PortDef::new("x", PortType::Untyped),
                PortDef::new("y", PortType::Untyped),
            ],
        }
    }
}

impl ProcessNode for XyPadProcessNode {
    fn node_id(&self) -> NodeId {
        self.id
    }
    fn type_name(&self) -> &'static str {
        "XY Pad"
    }
    fn inputs(&self) -> &[PortDef] {
        &[]
    }
    fn outputs(&self) -> &[PortDef] {
        &self.outputs
    }

    fn process(&mut self) {
        // Nothing to compute — outputs are the stored coordinates.
    }

    fn read_output(&self, channel: usize) -> f32 {
        match channel {
            0 => self.x.clamp(0.0, 1.0),
            1 => self.y.clamp(0.0, 1.0),
            _ => 0.0,
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "name": self.name,
            "x": self.x,
            "y": self.y,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
            self.name = n.to_string();
        }
        if let Some(x) = data.get("x").and_then(|v| v.as_f64()) {
            self.x = (x as f32).clamp(0.0, 1.0);
        }
        if let Some(y) = data.get("y").and_then(|v| v.as_f64()) {
            self.y = (y as f32).clamp(0.0, 1.0);
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float {
                name: "X".into(),
                value: self.x,
                min: 0.0,
                max: 1.0,
                step: 0.01,
                unit: "",
            },
            ParamDef::Float {
                name: "Y".into(),
                value: self.y,
                min: 0.0,
                max: 1.0,
                step: 0.01,
                unit: "",
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.x = value.as_f32().clamp(0.0, 1.0),
            1 => self.y = value.as_f32().clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(XyPadDisplay {
            name: self.name.clone(),
            x: self.x,
            y: self.y,
        }));
    }
}
