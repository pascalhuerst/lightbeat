use crate::color::{Rgb, Hsv, ColorConvert};
use crate::color::convert::{rgb_to_cmy, cmy_to_rgb, rgb_to_rgbw_naive};
use crate::engine::types::*;

/// Color merge/split mode.
/// `Neutral` is the initial mode before anything is connected — ports are
/// `Any` (white-ring) and the node does nothing. Connecting a wire on either
/// side promotes the node to a concrete mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Neutral,
    Rgb,
    Hsv,
    Rgbw,
    Cmy,
}

/// Names visible in the inspector dropdown.
pub const COLOR_MODE_NAMES: &[&str] = &["Auto", "RGB", "HSV", "RGBW", "CMY"];

impl ColorMode {
    pub fn channel_names(&self) -> &[&'static str] {
        match self {
            ColorMode::Neutral => &["?"],
            ColorMode::Rgb => &["R", "G", "B"],
            ColorMode::Hsv => &["H", "S", "V"],
            ColorMode::Rgbw => &["R", "G", "B", "W"],
            ColorMode::Cmy => &["C", "M", "Y"],
        }
    }

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => ColorMode::Neutral,
            1 => ColorMode::Rgb,
            2 => ColorMode::Hsv,
            3 => ColorMode::Rgbw,
            4 => ColorMode::Cmy,
            _ => ColorMode::Neutral,
        }
    }

    pub fn to_index(&self) -> usize {
        match self {
            ColorMode::Neutral => 0,
            ColorMode::Rgb => 1,
            ColorMode::Hsv => 2,
            ColorMode::Rgbw => 3,
            ColorMode::Cmy => 4,
        }
    }
}

/// Display state.
pub struct ColorOpsDisplay {
    pub mode: ColorMode,
    pub rgb: [f32; 3],
}

// ---------------------------------------------------------------------------
// Components to Color: component inputs → Color (RGB) output
// ---------------------------------------------------------------------------

pub struct ComponentsToColorProcessNode {
    id: NodeId,
    mode: ColorMode,
    channels: [f32; 4],
    rgb_out: [f32; 3],
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl ComponentsToColorProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mode = ColorMode::Neutral;
        Self {
            id,
            mode,
            channels: [0.0; 4],
            rgb_out: [0.0; 3],
            inputs: Self::make_inputs(mode),
            outputs: Self::make_outputs(mode),
        }
    }

    fn make_inputs(mode: ColorMode) -> Vec<PortDef> {
        match mode {
            ColorMode::Neutral => vec![PortDef::new("?", PortType::Any)],
            _ => mode.channel_names().iter().map(|n| PortDef::new(*n, PortType::Untyped)).collect(),
        }
    }

    fn make_outputs(mode: ColorMode) -> Vec<PortDef> {
        match mode {
            ColorMode::Neutral => vec![PortDef::new("?", PortType::Any)],
            _ => vec![PortDef::new("color", PortType::Color)],
        }
    }

    fn convert_to_rgb(&self) -> [f32; 3] {
        let c = &self.channels;
        match self.mode {
            ColorMode::Rgb => [c[0], c[1], c[2]],
            ColorMode::Hsv => {
                let rgb = Hsv::new(c[0], c[1], c[2]).to_rgb();
                [rgb.r, rgb.g, rgb.b]
            }
            ColorMode::Rgbw => {
                [c[0] + c[3], c[1] + c[3], c[2] + c[3]]
            }
            ColorMode::Cmy => {
                let rgb = cmy_to_rgb(c[0], c[1], c[2]);
                [rgb.r, rgb.g, rgb.b]
            }
            ColorMode::Neutral => [0.0; 3],
        }
    }
}

impl ProcessNode for ComponentsToColorProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Components to Color" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi < 4 { self.channels[pi] = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi < 4 { self.channels[pi] } else { 0.0 }
    }

    fn process(&mut self) {
        match self.mode {
            ColorMode::Neutral => {} // nothing connected yet
            _ => { self.rgb_out = self.convert_to_rgb(); }
        }
    }

    fn read_output(&self, channel: usize) -> f32 {
        match self.mode {
            ColorMode::Neutral => 0.0,
            _ => match channel {
                0 => self.rgb_out[0], 1 => self.rgb_out[1], 2 => self.rgb_out[2], _ => 0.0
            },
        }
    }

    fn non_promotable_params(&self) -> &'static [&'static str] {
        &["Mode"]
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Choice {
            name: "Mode".into(),
            value: self.mode.to_index(),
            options: COLOR_MODE_NAMES.iter().map(|s| s.to_string()).collect(),
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 {
            let new_mode = ColorMode::from_index(value.as_usize());
            if new_mode != self.mode {
                self.mode = new_mode;
                self.inputs = Self::make_inputs(new_mode);
                self.outputs = Self::make_outputs(new_mode);
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
// Color to Components: Color (RGB) input → component outputs
// ---------------------------------------------------------------------------

pub struct ColorToComponentsProcessNode {
    id: NodeId,
    mode: ColorMode,
    channels: [f32; 4],
    components_out: [f32; 4],
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl ColorToComponentsProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mode = ColorMode::Neutral;
        Self {
            id,
            mode,
            channels: [0.0; 4],
            components_out: [0.0; 4],
            inputs: Self::make_inputs(mode),
            outputs: Self::make_outputs(mode),
        }
    }

    fn make_inputs(mode: ColorMode) -> Vec<PortDef> {
        match mode {
            ColorMode::Neutral => vec![PortDef::new("?", PortType::Any)],
            _ => vec![PortDef::new("color", PortType::Color)],
        }
    }

    fn make_outputs(mode: ColorMode) -> Vec<PortDef> {
        match mode {
            ColorMode::Neutral => vec![PortDef::new("?", PortType::Any)],
            _ => mode.channel_names().iter().map(|n| PortDef::new(*n, PortType::Untyped)).collect(),
        }
    }

    fn convert_from_rgb(&self) -> [f32; 4] {
        let rgb = Rgb::new(self.channels[0], self.channels[1], self.channels[2]);
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
            ColorMode::Neutral => [0.0; 4],
        }
    }
}

impl ProcessNode for ColorToComponentsProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color to Components" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, channel: usize, v: f32) {
        if channel < 4 { self.channels[channel] = v; }
    }
    fn read_input(&self, channel: usize) -> f32 {
        if channel < 4 { self.channels[channel] } else { 0.0 }
    }

    fn process(&mut self) {
        match self.mode {
            ColorMode::Neutral => {} // nothing connected yet
            _ => { self.components_out = self.convert_from_rgb(); }
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        match self.mode {
            ColorMode::Neutral => 0.0,
            _ => if pi < 4 { self.components_out[pi] } else { 0.0 },
        }
    }

    fn non_promotable_params(&self) -> &'static [&'static str] {
        &["Mode"]
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Choice {
            name: "Mode".into(),
            value: self.mode.to_index(),
            options: COLOR_MODE_NAMES.iter().map(|s| s.to_string()).collect(),
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 {
            let new_mode = ColorMode::from_index(value.as_usize());
            if new_mode != self.mode {
                self.mode = new_mode;
                self.inputs = Self::make_inputs(new_mode);
                self.outputs = Self::make_outputs(new_mode);
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(ColorOpsDisplay {
            mode: self.mode,
            rgb: [self.channels[0], self.channels[1], self.channels[2]],
        }));
    }
}
