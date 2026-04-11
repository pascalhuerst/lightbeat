use crate::color::{Rgb, Hsv, ColorConvert};
use crate::color::convert::{rgb_to_cmy, cmy_to_rgb, rgb_to_rgbw_naive};
use crate::engine::types::*;

/// Color merge/split mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Rgb,
    Hsv,
    Rgbw,
    Cmy,
}

impl ColorMode {
    pub fn label(&self) -> &'static str {
        match self {
            ColorMode::Rgb => "RGB",
            ColorMode::Hsv => "HSV",
            ColorMode::Rgbw => "RGBW",
            ColorMode::Cmy => "CMY",
        }
    }

    pub fn channel_count(&self) -> usize {
        match self {
            ColorMode::Rgb | ColorMode::Hsv | ColorMode::Cmy => 3,
            ColorMode::Rgbw => 4,
        }
    }

    pub fn channel_names(&self) -> &[&'static str] {
        match self {
            ColorMode::Rgb => &["R", "G", "B"],
            ColorMode::Hsv => &["H", "S", "V"],
            ColorMode::Rgbw => &["R", "G", "B", "W"],
            ColorMode::Cmy => &["C", "M", "Y"],
        }
    }

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => ColorMode::Rgb,
            1 => ColorMode::Hsv,
            2 => ColorMode::Rgbw,
            3 => ColorMode::Cmy,
            _ => ColorMode::Rgb,
        }
    }

    pub fn to_index(&self) -> usize {
        match self {
            ColorMode::Rgb => 0,
            ColorMode::Hsv => 1,
            ColorMode::Rgbw => 2,
            ColorMode::Cmy => 3,
        }
    }
}

/// Display state.
pub struct ColorOpsDisplay {
    pub mode: ColorMode,
    pub rgb: [f32; 3],
}

// ---------------------------------------------------------------------------
// Color Merge: component inputs → Color (RGB) output
// ---------------------------------------------------------------------------

pub struct ColorMergeProcessNode {
    id: NodeId,
    mode: ColorMode,
    components: [f32; 4], // up to 4 input components
    rgb_out: [f32; 3],
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl ColorMergeProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mode = ColorMode::Rgb;
        Self {
            id,
            mode,
            components: [0.0; 4],
            rgb_out: [0.0; 3],
            inputs: Self::make_inputs(mode),
            outputs: vec![PortDef::new("color", PortType::Color)],
        }
    }

    fn make_inputs(mode: ColorMode) -> Vec<PortDef> {
        mode.channel_names().iter().map(|n| PortDef::new(*n, PortType::Untyped)).collect()
    }

    fn convert_to_rgb(&self) -> [f32; 3] {
        let c = &self.components;
        match self.mode {
            ColorMode::Rgb => [c[0], c[1], c[2]],
            ColorMode::Hsv => {
                let rgb = Hsv::new(c[0], c[1], c[2]).to_rgb();
                [rgb.r, rgb.g, rgb.b]
            }
            ColorMode::Rgbw => {
                // RGBW → RGB: add white to each channel.
                [c[0] + c[3], c[1] + c[3], c[2] + c[3]]
            }
            ColorMode::Cmy => {
                let rgb = cmy_to_rgb(c[0], c[1], c[2]);
                [rgb.r, rgb.g, rgb.b]
            }
        }
    }
}

impl ProcessNode for ColorMergeProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color Merge" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi < 4 { self.components[pi] = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi < 4 { self.components[pi] } else { 0.0 }
    }

    fn process(&mut self) {
        self.rgb_out = self.convert_to_rgb();
    }

    fn read_output(&self, channel: usize) -> f32 {
        match channel { 0 => self.rgb_out[0], 1 => self.rgb_out[1], 2 => self.rgb_out[2], _ => 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Choice {
            name: "Mode".into(),
            value: self.mode.to_index(),
            options: vec!["RGB".into(), "HSV".into(), "RGBW".into(), "CMY".into()],
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if let (0, ParamValue::Choice(v)) = (index, value) {
            let new_mode = ColorMode::from_index(v);
            if new_mode != self.mode {
                self.mode = new_mode;
                self.inputs = Self::make_inputs(new_mode);
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(ColorOpsDisplay {
            mode: self.mode,
            rgb: self.rgb_out,
        }));
    }
}

// ---------------------------------------------------------------------------
// Color Split: Color (RGB) input → component outputs
// ---------------------------------------------------------------------------

pub struct ColorSplitProcessNode {
    id: NodeId,
    mode: ColorMode,
    rgb_in: [f32; 3],
    components_out: [f32; 4],
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl ColorSplitProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mode = ColorMode::Rgb;
        Self {
            id,
            mode,
            rgb_in: [0.0; 3],
            components_out: [0.0; 4],
            inputs: vec![PortDef::new("color", PortType::Color)],
            outputs: Self::make_outputs(mode),
        }
    }

    fn make_outputs(mode: ColorMode) -> Vec<PortDef> {
        mode.channel_names().iter().map(|n| PortDef::new(*n, PortType::Untyped)).collect()
    }

    fn convert_from_rgb(&self) -> [f32; 4] {
        let rgb = Rgb::new(self.rgb_in[0], self.rgb_in[1], self.rgb_in[2]);
        match self.mode {
            ColorMode::Rgb => [rgb.r, rgb.g, rgb.b, 0.0],
            ColorMode::Hsv => {
                let hsv = rgb.to_hsv();
                [hsv.h, hsv.s, hsv.v, 0.0]
            }
            ColorMode::Rgbw => {
                let (r, g, b, w) = rgb_to_rgbw_naive(rgb);
                [r, g, b, w]
            }
            ColorMode::Cmy => {
                let (c, m, y) = rgb_to_cmy(rgb);
                [c, m, y, 0.0]
            }
        }
    }
}

impl ProcessNode for ColorSplitProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color Split" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, channel: usize, v: f32) {
        if channel < 3 { self.rgb_in[channel] = v; }
    }
    fn read_input(&self, channel: usize) -> f32 {
        if channel < 3 { self.rgb_in[channel] } else { 0.0 }
    }

    fn process(&mut self) {
        self.components_out = self.convert_from_rgb();
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi < 4 { self.components_out[pi] } else { 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Choice {
            name: "Mode".into(),
            value: self.mode.to_index(),
            options: vec!["RGB".into(), "HSV".into(), "RGBW".into(), "CMY".into()],
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if let (0, ParamValue::Choice(v)) = (index, value) {
            let new_mode = ColorMode::from_index(v);
            if new_mode != self.mode {
                self.mode = new_mode;
                self.outputs = Self::make_outputs(new_mode);
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(ColorOpsDisplay {
            mode: self.mode,
            rgb: self.rgb_in,
        }));
    }
}
