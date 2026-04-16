//! Solid pattern — paints every pixel of every strip with a single color.
//! Useful as a base layer or with low opacity for tinting.

use crate::color::Rgb;
use crate::engine::patterns::{Pattern, Pixel, StripFrame};
use crate::engine::types::{PortDef, PortType};

pub struct SolidPattern;

impl Pattern for SolidPattern {
    fn type_name(&self) -> &'static str { "Solid" }

    fn input_ports(&self) -> Vec<PortDef> {
        vec![PortDef::new("color", PortType::Color)]
    }

    fn render(&self, inputs: &[f32], frames: &mut [StripFrame<'_>]) {
        let r = inputs.first().copied().unwrap_or(0.0);
        let g = inputs.get(1).copied().unwrap_or(0.0);
        let b = inputs.get(2).copied().unwrap_or(0.0);
        let pixel = Pixel::opaque(Rgb::new(r, g, b));

        for frame in frames.iter_mut() {
            for px in frame.buf.iter_mut() {
                *px = pixel;
            }
        }
    }
}
