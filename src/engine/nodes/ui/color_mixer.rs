//! Color Mixer: three sliders that mix a color in RGB, HSV or CMY and output
//! it on a Color port.

use crate::color::{ColorConvert, Hsv, Rgb};
use serde::{Deserialize, Serialize};

use crate::engine::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerMode {
    Rgb,
    Hsv,
    Cmy,
}

pub const MIXER_MODE_NAMES: &[&str] = &["RGB", "HSV", "CMY"];

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports() -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![],
        vec![PortDef::new("color", PortType::Color)],
    )
}

impl MixerMode {
    pub fn to_index(self) -> usize {
        match self {
            MixerMode::Rgb => 0,
            MixerMode::Hsv => 1,
            MixerMode::Cmy => 2,
        }
    }

    pub fn from_index(i: usize) -> Self {
        match i {
            1 => MixerMode::Hsv,
            2 => MixerMode::Cmy,
            _ => MixerMode::Rgb,
        }
    }

    pub fn channel_names(self) -> [&'static str; 3] {
        match self {
            MixerMode::Rgb => ["R", "G", "B"],
            MixerMode::Hsv => ["H", "S", "V"],
            MixerMode::Cmy => ["C", "M", "Y"],
        }
    }

    /// The color three slider values in this mode stand for.
    pub fn to_rgb(self, v: [f32; 3]) -> Rgb {
        let v = v.map(|x| x.clamp(0.0, 1.0));
        match self {
            MixerMode::Rgb => Rgb::new(v[0], v[1], v[2]),
            MixerMode::Hsv => Hsv::new(v[0], v[1], v[2]).to_rgb(),
            MixerMode::Cmy => Rgb::new(1.0 - v[0], 1.0 - v[1], 1.0 - v[2]),
        }
    }

    /// Slider values in this mode that give `c`, so a mode switch keeps the color.
    pub fn from_rgb(self, c: Rgb) -> [f32; 3] {
        match self {
            MixerMode::Rgb => [c.r, c.g, c.b],
            MixerMode::Hsv => {
                let h = c.to_hsv();
                [h.h, h.s, h.v]
            }
            MixerMode::Cmy => [1.0 - c.r, 1.0 - c.g, 1.0 - c.b],
        }
    }
}

/// Display snapshot consumed by the widget each frame.
/// The node's settings, as saved and as the widget pushes them.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MixerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    values: Option<Vec<f32>>,
}

impl MixerConfig {
    pub fn new(name: &str, mode: MixerMode, values: [f32; 3]) -> Self {
        Self { name: Some(name.to_string()), mode: Some(mode.to_index()), values: Some(values.to_vec()) }
    }

    /// Malformed data loads as an empty update, which changes nothing.
    pub fn from_value(v: &serde_json::Value) -> Self {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

pub struct ColorMixerDisplay {
    pub name: String,
    pub mode: MixerMode,
    pub values: [f32; 3],
}

pub struct ColorMixerProcessNode {
    id: NodeId,
    name: String,
    mode: MixerMode,
    values: [f32; 3],
    out: [f32; 3],
    outputs: Vec<PortDef>,
}

impl ColorMixerProcessNode {
    pub fn new(id: NodeId) -> Self {
        let (_, outputs) = ports();
        Self {
            id,
            name: String::new(),
            mode: MixerMode::Rgb,
            values: [1.0; 3],
            out: [1.0; 3],
            outputs,
        }
    }
}

impl ProcessNode for ColorMixerProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color Mixer" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn process(&mut self) {
        let c = self.mode.to_rgb(self.values);
        self.out = [c.r, c.g, c.b];
    }

    fn read_output(&self, channel: usize) -> f32 {
        self.out.get(channel).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(MixerConfig::new(&self.name, self.mode, self.values).to_value())
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        let cfg = MixerConfig::from_value(data);
        if let Some(n) = cfg.name {
            self.name = n;
        }
        if let Some(m) = cfg.mode {
            self.mode = MixerMode::from_index(m);
        }
        if let Some(vs) = cfg.values {
            for (slot, x) in self.values.iter_mut().zip(vs) {
                *slot = x.clamp(0.0, 1.0);
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(ColorMixerDisplay {
            name: self.name.clone(),
            mode: self.mode,
            values: self.values,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_mode_makes_the_color_its_sliders_describe() {
        let mut n = ColorMixerProcessNode::new(NodeId(1));
        n.load_data(&serde_json::json!({ "mode": 2, "values": [1.0, 0.0, 0.0] }));
        n.process();
        // Full cyan is no red, all green and blue.
        assert_eq!([n.read_output(0), n.read_output(1), n.read_output(2)], [0.0, 1.0, 1.0]);
        n.load_data(&serde_json::json!({ "mode": 1, "values": [0.0, 1.0, 1.0] }));
        n.process();
        assert_eq!([n.read_output(0), n.read_output(1), n.read_output(2)], [1.0, 0.0, 0.0]);
    }

    #[test]
    fn the_hand_written_save_shape_loads_and_saves_unchanged() {
        let saved = serde_json::json!({ "name": "Wash", "mode": 1, "values": [0.25, 0.5, 0.75] });
        let mut n = ColorMixerProcessNode::new(NodeId(1));
        n.load_data(&saved);
        assert_eq!(n.name, "Wash");
        assert_eq!(n.mode, MixerMode::Hsv);
        assert_eq!(n.values, [0.25, 0.5, 0.75]);
        assert_eq!(n.save_data(), Some(saved));
    }

    #[test]
    fn switching_mode_keeps_the_color() {
        let c = Rgb::new(0.2, 0.6, 0.9);
        for from in [MixerMode::Rgb, MixerMode::Hsv, MixerMode::Cmy] {
            for to in [MixerMode::Rgb, MixerMode::Hsv, MixerMode::Cmy] {
                let back = to.to_rgb(to.from_rgb(from.to_rgb(from.from_rgb(c))));
                assert!((back.r - c.r).abs() < 1e-4 && (back.g - c.g).abs() < 1e-4 && (back.b - c.b).abs() < 1e-4);
            }
        }
    }
}
