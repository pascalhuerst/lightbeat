//! Ripple Burst: each trigger sends a ring of light out from a point.

use crate::color::Rgb;
use crate::engine::types::*;

use super::node::{EffectKind, RenderCtx, UpdateCtx};
use super::{LayerBuffer, Pixel};

/// Rings alive at once; more than this and the oldest is dropped.
const MAX_BURSTS: usize = 24;

pub struct Ripple {
    origin: f32,
    speed: f32,
    width: f32,
    decay: f32,
    intensity: f32,
    /// Age in seconds of each live ring, oldest first.
    bursts: Vec<f32>,
}

impl Default for Ripple {
    fn default() -> Self {
        Self { origin: 0.5, speed: 1.0, width: 0.15, decay: 1.2, intensity: 1.0, bursts: Vec::new() }
    }
}

/// How bright a ring of the given age is at distance `d` from its centre.
fn ring(d: f32, age: f32, speed: f32, width: f32, decay: f32) -> f32 {
    let life = 1.0 - age / decay;
    if life <= 0.0 {
        return 0.0;
    }
    let radius = speed * age;
    let edge = 1.0 - (d - radius).abs() / (width * 0.5).max(1e-4);
    edge.max(0.0) * life * life
}

impl EffectKind for Ripple {
    fn type_name(&self) -> &'static str { "Ripple Burst" }

    fn inputs(&self) -> Vec<PortDef> {
        vec![PortDef::new("trigger", PortType::Logic), PortDef::new("color", PortType::Gradient)]
    }

    fn trigger_ports(&self) -> Vec<usize> { vec![0] }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float { name: "Origin".into(), value: self.origin, min: 0.0, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Float { name: "Speed".into(), value: self.speed, min: 0.1, max: 6.0, step: 0.05, unit: "" },
            ParamDef::Float { name: "Width".into(), value: self.width, min: 0.02, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Float { name: "Decay".into(), value: self.decay, min: 0.1, max: 6.0, step: 0.05, unit: "s" },
            ParamDef::Float { name: "Intensity".into(), value: self.intensity, min: 0.0, max: 1.0, step: 0.01, unit: "" },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        let v = value.as_f32();
        match index {
            0 => self.origin = v.clamp(0.0, 1.0),
            1 => self.speed = v.max(0.01),
            2 => self.width = v.max(0.01),
            3 => self.decay = v.max(0.05),
            4 => self.intensity = v.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        for age in self.bursts.iter_mut() {
            *age += ctx.dt;
        }
        let decay = self.decay;
        self.bursts.retain(|age| *age < decay);
        for _ in 0..ctx.triggers.first().copied().unwrap_or(0) {
            self.bursts.push(0.0);
        }
        if self.bursts.len() > MAX_BURSTS {
            let extra = self.bursts.len() - MAX_BURSTS;
            self.bursts.drain(..extra);
        }
    }

    fn render(&self, ctx: &RenderCtx, buf: &mut LayerBuffer) {
        if self.bursts.is_empty() {
            return;
        }
        for (t, strip) in buf.iter_mut().enumerate() {
            for (i, px) in strip.iter_mut().enumerate() {
                let (x, y) = ctx.geometry.position(t, i);
                let d = ((x - self.origin).powi(2) + (y - 0.5).powi(2)).sqrt();
                let (mut total, mut color) = (0.0f32, Rgb::BLACK);
                for &age in &self.bursts {
                    let amount = ring(d, age, self.speed, self.width, self.decay);
                    if amount > 0.0 {
                        // Colour follows the ring's age along the gradient.
                        let (c, a) = ctx.gradient.sample_with_alpha(age / self.decay);
                        let w = amount * a;
                        total += w;
                        color = Rgb::new(color.r + c.r * w, color.g + c.g * w, color.b + c.b * w);
                    }
                }
                if total > 0.0 {
                    let c = color.scaled(1.0 / total);
                    *px = Pixel { color: c, alpha: total.min(1.0) * self.intensity };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ring_is_brightest_at_its_radius_and_fades_with_age() {
        let at_radius = ring(0.5, 0.5, 1.0, 0.2, 1.0);
        assert!(at_radius > ring(0.4, 0.5, 1.0, 0.2, 1.0));
        assert!(at_radius > ring(0.7, 0.5, 1.0, 0.2, 1.0));
        assert!(ring(1.0, 1.0, 1.0, 0.2, 1.0) == 0.0, "gone at the end of its life");
        assert!(ring(0.2, 0.2, 1.0, 0.2, 1.0) > ring(0.8, 0.8, 1.0, 0.2, 1.0));
    }

    #[test]
    fn triggers_spawn_rings_that_expire_and_are_capped() {
        let mut r = Ripple::default();
        r.update(&UpdateCtx { dt: 0.0, triggers: &[3] });
        assert_eq!(r.bursts.len(), 3);
        r.update(&UpdateCtx { dt: 2.0, triggers: &[0] });
        assert!(r.bursts.is_empty(), "older than the decay time");
        r.update(&UpdateCtx { dt: 0.0, triggers: &[100] });
        assert_eq!(r.bursts.len(), MAX_BURSTS);
    }
}
