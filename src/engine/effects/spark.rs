//! Spark Burst: each trigger throws a handful of sparks from a point. They fly
//! off with a random speed and direction, slow down, flicker and burn out,
//! leaving a short streak behind them.

use crate::color::Rgb;
use crate::engine::types::*;

use super::node::{EffectKind, RenderCtx, UpdateCtx};
use super::{LayerBuffer, Pixel};

/// Sparks alive at once; more than this and the oldest are dropped.
const MAX_SPARKS: usize = 256;
/// Flicker rate of a burning spark, in cycles per second.
const FLICKER_HZ: f32 = 22.0;

#[derive(Clone, Copy, Debug)]
struct Spark {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
    phase: f32,
}

pub struct SparkBurst {
    origin: f32,
    count: usize,
    speed: f32,
    variance: f32,
    drag: f32,
    lifetime: f32,
    size: f32,
    tail: f32,
    vertical: f32,
    intensity: f32,
    sparks: Vec<Spark>,
    rng: u32,
}

impl Default for SparkBurst {
    fn default() -> Self {
        Self {
            origin: 0.5,
            count: 12,
            speed: 1.2,
            variance: 0.6,
            drag: 2.0,
            lifetime: 0.9,
            size: 0.03,
            tail: 0.06,
            vertical: 0.3,
            intensity: 1.0,
            sparks: Vec::new(),
            rng: 0x9E37_79B9,
        }
    }
}

impl SparkBurst {
    /// Uniform in 0..1 (xorshift; the sparks only need to look random).
    fn random(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn throw(&mut self) {
        let angle = self.random() * std::f32::consts::TAU;
        let speed = self.speed * (1.0 - self.variance * self.random());
        let life = self.lifetime * (1.0 - 0.5 * self.variance * self.random());
        let phase = self.random();
        self.sparks.push(Spark {
            x: self.origin,
            y: 0.5,
            vx: angle.cos() * speed,
            vy: angle.sin() * speed * self.vertical,
            age: 0.0,
            life,
            phase,
        });
    }
}

/// How bright a spark at `p` makes the pixel at `(x, y)`: a soft dot with a
/// streak trailing back along its velocity, fading towards the tail's end.
fn glow(spark: &Spark, tail: f32, size: f32, x: f32, y: f32) -> f32 {
    let (tx, ty) = (spark.x - spark.vx * tail, spark.y - spark.vy * tail);
    let (sx, sy) = (spark.x - tx, spark.y - ty);
    let len2 = sx * sx + sy * sy;
    let u = if len2 > 1e-12 { (((x - tx) * sx + (y - ty) * sy) / len2).clamp(0.0, 1.0) } else { 1.0 };
    let (cx, cy) = (tx + sx * u, ty + sy * u);
    let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
    let falloff = (1.0 - d / size.max(1e-4)).max(0.0);
    falloff * (0.25 + 0.75 * u)
}

impl EffectKind for SparkBurst {
    fn type_name(&self) -> &'static str { "Spark Burst" }

    fn inputs(&self) -> Vec<PortDef> {
        vec![PortDef::new("trigger", PortType::Logic), PortDef::new("color", PortType::Gradient)]
    }

    fn trigger_ports(&self) -> Vec<usize> { vec![0] }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float { name: "Origin".into(), value: self.origin, min: 0.0, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Int { name: "Sparks".into(), value: self.count as i64, min: 1, max: 64 },
            ParamDef::Float { name: "Speed".into(), value: self.speed, min: 0.1, max: 5.0, step: 0.05, unit: "" },
            ParamDef::Float { name: "Variance".into(), value: self.variance, min: 0.0, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Float { name: "Drag".into(), value: self.drag, min: 0.0, max: 10.0, step: 0.1, unit: "" },
            ParamDef::Float { name: "Lifetime".into(), value: self.lifetime, min: 0.1, max: 4.0, step: 0.05, unit: "s" },
            ParamDef::Float { name: "Size".into(), value: self.size, min: 0.005, max: 0.3, step: 0.005, unit: "" },
            ParamDef::Float { name: "Tail".into(), value: self.tail, min: 0.0, max: 0.4, step: 0.005, unit: "s" },
            ParamDef::Float { name: "Vertical".into(), value: self.vertical, min: 0.0, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Float { name: "Intensity".into(), value: self.intensity, min: 0.0, max: 1.0, step: 0.01, unit: "" },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        let v = value.as_f32();
        match index {
            0 => self.origin = v.clamp(0.0, 1.0),
            1 => self.count = (v.round() as usize).clamp(1, 64),
            2 => self.speed = v.max(0.01),
            3 => self.variance = v.clamp(0.0, 1.0),
            4 => self.drag = v.max(0.0),
            5 => self.lifetime = v.max(0.05),
            6 => self.size = v.max(0.001),
            7 => self.tail = v.max(0.0),
            8 => self.vertical = v.clamp(0.0, 1.0),
            9 => self.intensity = v.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn update(&mut self, ctx: &UpdateCtx) {
        let slow = (-self.drag * ctx.dt).exp();
        for s in self.sparks.iter_mut() {
            s.age += ctx.dt;
            s.x += s.vx * ctx.dt;
            s.y += s.vy * ctx.dt;
            s.vx *= slow;
            s.vy *= slow;
        }
        self.sparks.retain(|s| s.age < s.life);
        for _ in 0..ctx.triggers.first().copied().unwrap_or(0) {
            for _ in 0..self.count {
                self.throw();
            }
        }
        if self.sparks.len() > MAX_SPARKS {
            let extra = self.sparks.len() - MAX_SPARKS;
            self.sparks.drain(..extra);
        }
    }

    fn render(&self, ctx: &RenderCtx, buf: &mut LayerBuffer) {
        if self.sparks.is_empty() {
            return;
        }
        // Per spark: its colour along the gradient, and how bright it burns.
        let lit: Vec<(Spark, Rgb, f32)> = self
            .sparks
            .iter()
            .map(|s| {
                let t = s.age / s.life;
                let (color, alpha) = ctx.gradient.sample_with_alpha(t);
                let flicker = 0.65 + 0.35 * ((s.age * FLICKER_HZ + s.phase * std::f32::consts::TAU).sin() * 0.5 + 0.5);
                (*s, color, (1.0 - t).powi(2) * flicker * alpha)
            })
            .collect();
        for (t, strip) in buf.iter_mut().enumerate() {
            for (i, px) in strip.iter_mut().enumerate() {
                let (x, y) = ctx.geometry.position(t, i);
                let (mut total, mut color) = (0.0f32, Rgb::BLACK);
                for (s, c, brightness) in &lit {
                    let w = glow(s, self.tail, self.size, x, y) * brightness;
                    if w > 0.0 {
                        total += w;
                        color = Rgb::new(color.r + c.r * w, color.g + c.g * w, color.b + c.b * w);
                    }
                }
                if total > 0.0 {
                    *px = Pixel { color: color.scaled(1.0 / total), alpha: total.min(1.0) * self.intensity };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spark_at(x: f32, vx: f32) -> Spark {
        Spark { x, y: 0.5, vx, vy: 0.0, age: 0.0, life: 1.0, phase: 0.0 }
    }

    #[test]
    fn a_spark_lights_its_head_and_streaks_behind_it_but_not_ahead() {
        let s = spark_at(0.5, 1.0);
        let head = glow(&s, 0.1, 0.03, 0.5, 0.5);
        let behind = glow(&s, 0.1, 0.03, 0.45, 0.5);
        let ahead = glow(&s, 0.1, 0.03, 0.55, 0.5);
        assert!(head > behind && behind > 0.0, "the streak fades towards its end");
        assert_eq!(ahead, 0.0);
        assert_eq!(glow(&s, 0.1, 0.03, 0.9, 0.5), 0.0);
    }

    #[test]
    fn a_trigger_throws_sparks_that_slow_down_and_burn_out() {
        let mut b = SparkBurst::default();
        b.update(&UpdateCtx { dt: 0.0, triggers: &[2] });
        assert_eq!(b.sparks.len(), 2 * b.count);
        let fast = b.sparks.iter().map(|s| s.vx.abs()).fold(0.0, f32::max);
        b.update(&UpdateCtx { dt: 0.3, triggers: &[0] });
        let slower = b.sparks.iter().map(|s| s.vx.abs()).fold(0.0, f32::max);
        assert!(slower < fast);
        b.update(&UpdateCtx { dt: 5.0, triggers: &[0] });
        assert!(b.sparks.is_empty());
    }

    #[test]
    fn sparks_are_capped_and_not_all_alike() {
        let mut b = SparkBurst::default();
        b.update(&UpdateCtx { dt: 0.0, triggers: &[100] });
        assert_eq!(b.sparks.len(), MAX_SPARKS);
        assert!(b.sparks.windows(2).any(|w| (w[0].vx - w[1].vx).abs() > 1e-3));
    }
}
