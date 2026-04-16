//! Bar pattern — a moving bar of given width and color along the group's
//! logical 0..1 axis.

use crate::color::Rgb;
use crate::engine::patterns::{Pattern, Pixel, StripFrame};
use crate::engine::types::{PortDef, PortType};

pub struct BarPattern;

impl Pattern for BarPattern {
    fn type_name(&self) -> &'static str { "Bar" }

    fn input_ports(&self) -> Vec<PortDef> {
        vec![
            PortDef::new("position", PortType::Untyped),
            PortDef::new("width", PortType::Untyped),
            PortDef::new("color", PortType::Color),
        ]
    }

    fn render(&self, inputs: &[f32], frames: &mut [StripFrame<'_>]) {
        // Channels: position(1) + width(1) + color(3) = 5
        let pos = inputs.first().copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let width = inputs.get(1).copied().unwrap_or(0.1).clamp(0.0, 1.0);
        let r = inputs.get(2).copied().unwrap_or(0.0);
        let g = inputs.get(3).copied().unwrap_or(0.0);
        let b = inputs.get(4).copied().unwrap_or(0.0);
        let color = Rgb::new(r, g, b);

        let lo = pos - width * 0.5;
        let hi = pos + width * 0.5;

        for frame in frames.iter_mut() {
            let layout = frame.layout;
            let span = layout.logical_end - layout.logical_start;
            let n = frame.pixel_count;
            if n == 0 { continue; }
            let denom = (n as f32 - 1.0).max(1.0);

            for px in 0..n {
                let t = px as f32 / denom;
                let logical = layout.logical_start + t * span;
                if logical >= lo && logical <= hi {
                    frame.buf[px] = Pixel::opaque(color);
                } else {
                    frame.buf[px] = Pixel::TRANSPARENT;
                }
            }
        }
    }
}
