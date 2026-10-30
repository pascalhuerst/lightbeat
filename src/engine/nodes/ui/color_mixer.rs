//! Color Mixer: three sliders that mix a color in RGB, HSV or CMY and output
//! it on a Color port.

use crate::color::{ColorConvert, Hsv, Rgb};
use crate::engine::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerMode {
    Rgb,
    Hsv,
    Cmy,
}

pub const MIXER_MODE_NAMES: &[&str] = &["RGB", "HSV", "CMY"];

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
        Self {
            id,
            name: String::new(),
            mode: MixerMode::Rgb,
            values: [1.0; 3],
            out: [1.0; 3],
            outputs: vec![PortDef::new("color", PortType::Color)],
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
        Some(serde_json::json!({
            "name": self.name,
            "mode": self.mode.to_index(),
            "values": self.values,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
            self.name = n.to_string();
        }
        if let Some(m) = data.get("mode").and_then(|v| v.as_u64()) {
            self.mode = MixerMode::from_index(m as usize);
        }
        if let Some(vs) = data.get("values").and_then(|v| v.as_array()) {
            for (slot, v) in self.values.iter_mut().zip(vs) {
                if let Some(x) = v.as_f64() {
                    *slot = (x as f32).clamp(0.0, 1.0);
                }
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
