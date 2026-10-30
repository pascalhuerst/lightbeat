//! Fill: the colour (or gradient across the group) on every pixel.

use crate::engine::types::*;

use super::node::{EffectKind, RenderCtx};
use super::{LayerBuffer, Pixel};

pub struct Fill {
    intensity: f32,
}

impl Default for Fill {
    fn default() -> Self {
        Self { intensity: 1.0 }
    }
}

impl EffectKind for Fill {
    fn type_name(&self) -> &'static str { "Fill" }

    fn inputs(&self) -> Vec<PortDef> {
        vec![PortDef::new("color", PortType::Gradient)]
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Float { name: "Intensity".into(), value: self.intensity, min: 0.0, max: 1.0, step: 0.01, unit: "" }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 {
            self.intensity = value.as_f32().clamp(0.0, 1.0);
        }
    }

    fn render(&self, ctx: &RenderCtx, buf: &mut LayerBuffer) {
        for (t, strip) in buf.iter_mut().enumerate() {
            for (i, px) in strip.iter_mut().enumerate() {
                let (x, _) = ctx.geometry.position(t, i);
                let (color, alpha) = ctx.gradient.sample_with_alpha(x.clamp(0.0, 1.0));
                *px = Pixel { color, alpha: alpha * self.intensity };
            }
        }
    }
}
