//! XY Pad: a draggable point inside a unit square, emitting four values
//! based on the point's position.
//!
//! Output layout follows a Z-order (reading left→right, top→bottom):
//! q1 (top-left), q2 (top-right), q3 (bottom-left), q4 (bottom-right).
//!
//! Two semantics, picked via `Mode`:
//!
//! - **Positions** (default): four sorted 0..1 values suitable for wiring
//!   into gradient-stop positions. At the pad's centre they are evenly
//!   distributed `[0, 0.333, 0.667, 1]`; as the knob moves, stops compress
//!   toward the opposite edge of the pulled corner. `q1` is always at 0,
//!   `q4` always at 1 — the interior two (`q2`, `q3`) slide between them
//!   based on the bilinear weight of each preceding corner.
//!
//! - **Mix**: classic bilinear corner weights summing to 1:
//!   `q1 = (1-x)(1-y)`, `q2 = x(1-y)`, `q3 = (1-x)y`, `q4 = xy`. Use for
//!   four-way mixer / crossfader scenarios.

use crate::engine::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XyPadMode {
    Positions,
    Mix,
}

impl XyPadMode {
    pub fn as_str(self) -> &'static str {
        match self { XyPadMode::Positions => "positions", XyPadMode::Mix => "mix" }
    }
    pub fn from_str(s: &str) -> Self {
        match s { "mix" => XyPadMode::Mix, _ => XyPadMode::Positions }
    }
    pub fn to_index(self) -> usize {
        match self { XyPadMode::Positions => 0, XyPadMode::Mix => 1 }
    }
    pub fn from_index(i: usize) -> Self {
        match i { 1 => XyPadMode::Mix, _ => XyPadMode::Positions }
    }
}

/// Display snapshot consumed by the widget each frame.
pub struct XyPadDisplay {
    pub name: String,
    pub mode: XyPadMode,
    pub x: f32,
    pub y: f32,
}

pub struct XyPadProcessNode {
    id: NodeId,
    name: String,
    mode: XyPadMode,
    /// Current point position in the unit square (both clamped 0..=1).
    x: f32,
    y: f32,
    output_values: [f32; 4],
    outputs: Vec<PortDef>,
}

impl XyPadProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            name: String::new(),
            mode: XyPadMode::Positions,
            x: 0.5,
            y: 0.5,
            // Default outputs at centre, positions mode: evenly spread.
            output_values: [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0],
            outputs: vec![
                PortDef::new("q1", PortType::Untyped),
                PortDef::new("q2", PortType::Untyped),
                PortDef::new("q3", PortType::Untyped),
                PortDef::new("q4", PortType::Untyped),
            ],
        }
    }
}

impl ProcessNode for XyPadProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "XY Pad" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn process(&mut self) {
        let x = self.x.clamp(0.0, 1.0);
        let y = self.y.clamp(0.0, 1.0);
        // Bilinear corner weights (always sum to 1).
        let w = [
            (1.0 - x) * (1.0 - y),
            x * (1.0 - y),
            (1.0 - x) * y,
            x * y,
        ];
        match self.mode {
            XyPadMode::Mix => {
                self.output_values = w;
            }
            XyPadMode::Positions => {
                // Cumulative sum of the first three weights, normalised so
                // the centre (w = [0.25; 4]) maps to [0, 0.333, 0.667, 1].
                // q1 is always anchored at 0 and q4 at 1.
                let denom = (w[0] + w[1] + w[2]).max(1e-6);
                self.output_values[0] = 0.0;
                self.output_values[1] = (w[0] / denom).clamp(0.0, 1.0);
                self.output_values[2] = ((w[0] + w[1]) / denom).clamp(0.0, 1.0);
                self.output_values[3] = 1.0;
            }
        }
    }

    fn read_output(&self, channel: usize) -> f32 {
        self.output_values.get(channel).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "name": self.name,
            "mode": self.mode.as_str(),
            "x": self.x,
            "y": self.y,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
            self.name = n.to_string();
        }
        if let Some(s) = data.get("mode").and_then(|v| v.as_str()) {
            self.mode = XyPadMode::from_str(s);
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
            ParamDef::Choice {
                name: "Mode".into(),
                value: self.mode.to_index(),
                options: vec!["Positions".into(), "Mix".into()],
            },
            ParamDef::Float { name: "X".into(), value: self.x, min: 0.0, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Float { name: "Y".into(), value: self.y, min: 0.0, max: 1.0, step: 0.01, unit: "" },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.mode = XyPadMode::from_index(value.as_usize()),
            1 => self.x = value.as_f32().clamp(0.0, 1.0),
            2 => self.y = value.as_f32().clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(XyPadDisplay {
            name: self.name.clone(),
            mode: self.mode,
            x: self.x,
            y: self.y,
        }));
    }
}
