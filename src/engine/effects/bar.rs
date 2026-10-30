//! Bar Loop: a bar of light at a position on the group.
//!
//! The position, width and softness are parameters, so each can be promoted
//! to an input and driven by an LFO, the beat phase or a fader.

use crate::engine::types::*;

use super::node::{EffectKind, RenderCtx};
use super::{LayerBuffer, Pixel};

const ANCHORS: [&str; 3] = ["Start", "Center", "End"];

pub struct Bar {
    position: f32,
    width: f32,
    anchor: usize,
    softness: f32,
    intensity: f32,
}

impl Default for Bar {
    fn default() -> Self {
        Self { position: 0.5, width: 0.15, anchor: 1, softness: 0.0, intensity: 1.0 }
    }
}

impl Bar {
    /// Where the bar starts, on the group's 0..1 axis.
    fn start(&self) -> f32 {
        match self.anchor {
            0 => self.position,
            1 => self.position - self.width * 0.5,
            _ => self.position - self.width,
        }
    }
}

/// Coverage of a pixel at `x` for a bar from `lo` to `hi`, with edges that
/// fade over `softness` of the bar's half width.
fn coverage(x: f32, lo: f32, hi: f32, softness: f32) -> f32 {
    if x < lo || x > hi {
        return 0.0;
    }
    let fade = softness * (hi - lo) * 0.5;
    if fade <= 1e-6 {
        return 1.0;
    }
    ((x - lo).min(hi - x) / fade).clamp(0.0, 1.0)
}

impl EffectKind for Bar {
    fn type_name(&self) -> &'static str { "Bar Loop" }

    fn inputs(&self) -> Vec<PortDef> {
        vec![PortDef::new("color", PortType::Gradient)]
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float { name: "Position".into(), value: self.position, min: 0.0, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Float { name: "Width".into(), value: self.width, min: 0.01, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Choice { name: "Anchor".into(), value: self.anchor, options: ANCHORS.iter().map(|s| s.to_string()).collect() },
            ParamDef::Float { name: "Softness".into(), value: self.softness, min: 0.0, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Float { name: "Intensity".into(), value: self.intensity, min: 0.0, max: 1.0, step: 0.01, unit: "" },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.position = value.as_f32(),
            1 => self.width = value.as_f32().max(0.001),
            2 => self.anchor = value.as_usize().min(ANCHORS.len() - 1),
            3 => self.softness = value.as_f32().clamp(0.0, 1.0),
            4 => self.intensity = value.as_f32().clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn render(&self, ctx: &RenderCtx, buf: &mut LayerBuffer) {
        let lo = self.start();
        let hi = lo + self.width;
        for (t, strip) in buf.iter_mut().enumerate() {
            for (i, px) in strip.iter_mut().enumerate() {
                let (x, _) = ctx.geometry.position(t, i);
                let cover = coverage(x, lo, hi, self.softness);
                if cover > 0.0 {
                    // The gradient runs along the bar, not along the group.
                    let (color, alpha) = ctx.gradient.sample_with_alpha(((x - lo) / self.width).clamp(0.0, 1.0));
                    *px = Pixel { color, alpha: alpha * cover * self.intensity };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_anchor_decides_which_edge_the_position_marks() {
        let mut bar = Bar { position: 0.5, width: 0.2, ..Default::default() };
        bar.anchor = 0;
        assert!((bar.start() - 0.5).abs() < 1e-6);
        bar.anchor = 1;
        assert!((bar.start() - 0.4).abs() < 1e-6);
        bar.anchor = 2;
        assert!((bar.start() - 0.3).abs() < 1e-6);
    }

    #[test]
    fn coverage_is_full_inside_zero_outside_and_fades_at_soft_edges() {
        assert_eq!(coverage(0.5, 0.4, 0.6, 0.0), 1.0);
        assert_eq!(coverage(0.39, 0.4, 0.6, 0.0), 0.0);
        assert_eq!(coverage(0.61, 0.4, 0.6, 0.0), 0.0);
        let edge = coverage(0.41, 0.4, 0.6, 1.0);
        assert!(edge > 0.0 && edge < 0.5, "soft edge {edge}");
        assert!((coverage(0.5, 0.4, 0.6, 1.0) - 1.0).abs() < 1e-4, "full in the middle");
    }
}
