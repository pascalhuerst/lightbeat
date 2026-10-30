//! Wave: a repeating pattern that travels across the group with the phase
//! it is given, e.g. the beat clock's bar phase.

use crate::engine::types::*;

use super::node::{EffectKind, RenderCtx};
use super::{LayerBuffer, Pixel};

const SHAPES: [&str; 4] = ["Sine", "Triangle", "Saw", "Square"];
const DIRECTIONS: [&str; 4] = ["Right", "Left", "Outward", "Inward"];

pub struct Wave {
    wavelength: f32,
    shape: usize,
    direction: usize,
    intensity: f32,
}

impl Default for Wave {
    fn default() -> Self {
        Self { wavelength: 0.5, shape: 0, direction: 0, intensity: 1.0 }
    }
}

/// 0..1 for a position in a cycle (`p` in cycles, any real number).
fn shape_value(shape: usize, p: f32) -> f32 {
    let f = p.rem_euclid(1.0);
    match shape {
        0 => 0.5 + 0.5 * (std::f32::consts::TAU * f).cos(),
        1 => 1.0 - (2.0 * f - 1.0).abs(),
        2 => 1.0 - f,
        _ => (f < 0.5) as u8 as f32,
    }
}

impl EffectKind for Wave {
    fn type_name(&self) -> &'static str { "Wave Loop" }

    fn inputs(&self) -> Vec<PortDef> {
        vec![PortDef::new("phase", PortType::Phase), PortDef::new("color", PortType::Gradient)]
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float { name: "Wavelength".into(), value: self.wavelength, min: 0.05, max: 4.0, step: 0.01, unit: "" },
            ParamDef::Choice { name: "Shape".into(), value: self.shape, options: SHAPES.iter().map(|s| s.to_string()).collect() },
            ParamDef::Choice { name: "Direction".into(), value: self.direction, options: DIRECTIONS.iter().map(|s| s.to_string()).collect() },
            ParamDef::Float { name: "Intensity".into(), value: self.intensity, min: 0.0, max: 1.0, step: 0.01, unit: "" },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.wavelength = value.as_f32().max(0.01),
            1 => self.shape = value.as_usize().min(SHAPES.len() - 1),
            2 => self.direction = value.as_usize().min(DIRECTIONS.len() - 1),
            3 => self.intensity = value.as_f32().clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn render(&self, ctx: &RenderCtx, buf: &mut LayerBuffer) {
        let phase = ctx.inputs.first().copied().unwrap_or(0.0);
        for (t, strip) in buf.iter_mut().enumerate() {
            for (i, px) in strip.iter_mut().enumerate() {
                let (x, _) = ctx.geometry.position(t, i);
                // Distance along the direction of travel.
                let (u, sign) = match self.direction {
                    0 => (x, -1.0),
                    1 => (x, 1.0),
                    2 => ((x - 0.5).abs() * 2.0, -1.0),
                    _ => ((x - 0.5).abs() * 2.0, 1.0),
                };
                let v = shape_value(self.shape, u / self.wavelength + sign * phase);
                let (color, alpha) = ctx.gradient.sample_with_alpha(x.clamp(0.0, 1.0));
                *px = Pixel { color, alpha: alpha * v * self.intensity };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_stay_in_range_and_peak_where_expected() {
        for shape in 0..4 {
            for k in -40..40 {
                let v = shape_value(shape, k as f32 * 0.137);
                assert!((0.0..=1.0).contains(&v), "shape {shape} gave {v}");
            }
        }
        assert!((shape_value(0, 0.0) - 1.0).abs() < 1e-6);
        assert!((shape_value(0, 0.5) - 0.0).abs() < 1e-6);
        assert!((shape_value(1, 0.5) - 1.0).abs() < 1e-6);
        assert_eq!(shape_value(3, 0.25), 1.0);
        assert_eq!(shape_value(3, 0.75), 0.0);
    }
}
