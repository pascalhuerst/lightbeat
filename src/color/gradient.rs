use super::Rgb;
use crate::engine::types::{GRADIENT_STOP_COUNT, GRADIENT_STOP_FLOATS};

/// A single color stop in a gradient.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GradientStop {
    /// Position along the gradient, 0.0..=1.0.
    pub position: f32,
    pub color: Rgb,
    /// 0..=1 opacity. Carried through the channel-flat representation so
    /// downstream consumers can do alpha-blended writes. Serde defaults to
    /// 1.0 for back-compat with old JSON that didn't have this field.
    #[serde(default = "default_alpha")]
    pub alpha: f32,
}

fn default_alpha() -> f32 { 1.0 }

impl GradientStop {
    pub fn opaque(position: f32, color: Rgb) -> Self {
        Self { position, color, alpha: 1.0 }
    }
}

/// A color gradient defined by sorted stops, for mapping a 0..1 value to a color.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Gradient {
    stops: Vec<GradientStop>,
}

impl Gradient {
    /// Create a gradient from stops. Stops will be sorted by position.
    pub fn new(mut stops: Vec<GradientStop>) -> Self {
        stops.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap_or(std::cmp::Ordering::Equal));
        Self { stops }
    }

    /// Two-stop gradient from one color to another (both fully opaque).
    pub fn two(start: Rgb, end: Rgb) -> Self {
        Self::new(vec![
            GradientStop::opaque(0.0, start),
            GradientStop::opaque(1.0, end),
        ])
    }

    /// Sample the gradient at a position (0.0..=1.0). Clamps to edges.
    pub fn sample(&self, t: f32) -> Rgb {
        self.sample_with_alpha(t).0
    }

    /// Sample returning both color and alpha. Useful for alpha-blended
    /// writes where the caller needs to know how opaque the result is.
    pub fn sample_with_alpha(&self, t: f32) -> (Rgb, f32) {
        if self.stops.is_empty() {
            return (Rgb::BLACK, 0.0);
        }
        if self.stops.len() == 1 || t <= self.stops[0].position {
            let s = &self.stops[0];
            return (s.color, s.alpha);
        }
        let last = self.stops.last().unwrap();
        if t >= last.position {
            return (last.color, last.alpha);
        }

        for i in 1..self.stops.len() {
            if t <= self.stops[i].position {
                let a = &self.stops[i - 1];
                let b = &self.stops[i];
                let range = b.position - a.position;
                let local_t = if range > 0.0 { (t - a.position) / range } else { 0.0 };
                return (
                    a.color.lerp(b.color, local_t),
                    lerp_f32(a.alpha, b.alpha, local_t),
                );
            }
        }

        (last.color, last.alpha)
    }

    /// Sample with wrapping (t wraps around 0..1 via modulo).
    #[allow(dead_code)]
    pub fn sample_wrapped(&self, t: f32) -> Rgb {
        self.sample(t.rem_euclid(1.0))
    }

    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }

    /// Decode a flat `GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS` channel
    /// buffer into a Gradient. Layout per stop: r, g, b, a, position. Stops
    /// with `alpha < 0` are treated as unused and dropped. Positions are
    /// clamped to 0..=1. Stops come out sorted by position.
    pub fn from_channels(channels: &[f32]) -> Self {
        let mut stops = Vec::with_capacity(GRADIENT_STOP_COUNT);
        for i in 0..GRADIENT_STOP_COUNT {
            let base = i * GRADIENT_STOP_FLOATS;
            if base + 4 >= channels.len() { break; }
            let r = channels[base];
            let g = channels[base + 1];
            let b = channels[base + 2];
            let a = channels[base + 3];
            let pos = channels[base + 4];
            if a < 0.0 { continue; } // unused marker
            stops.push(GradientStop {
                position: pos.clamp(0.0, 1.0),
                color: Rgb::new(r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)),
                alpha: a.clamp(0.0, 1.0),
            });
        }
        Self::new(stops)
    }

    /// Encode this gradient into a flat channel buffer of the configured
    /// `GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS` size. Unused slots are
    /// marked with `alpha = -1.0`.
    pub fn to_channels(&self) -> [f32; GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS] {
        let mut out = [0.0f32; GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS];
        for i in 0..GRADIENT_STOP_COUNT {
            let base = i * GRADIENT_STOP_FLOATS;
            if let Some(s) = self.stops.get(i) {
                out[base] = s.color.r;
                out[base + 1] = s.color.g;
                out[base + 2] = s.color.b;
                out[base + 3] = s.alpha;
                out[base + 4] = s.position;
            } else {
                out[base + 3] = -1.0; // unused
            }
        }
        out
    }
}

fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl Default for Gradient {
    fn default() -> Self {
        Self::two(Rgb::BLACK, Rgb::WHITE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_stop_midpoint() {
        let g = Gradient::two(Rgb::BLACK, Rgb::WHITE);
        let mid = g.sample(0.5);
        assert!((mid.r - 0.5).abs() < 1e-5);
        assert!((mid.g - 0.5).abs() < 1e-5);
        assert!((mid.b - 0.5).abs() < 1e-5);
    }

    #[test]
    fn clamps_to_edges() {
        let g = Gradient::two(Rgb::new(1.0, 0.0, 0.0), Rgb::new(0.0, 0.0, 1.0));
        let below = g.sample(-1.0);
        assert!((below.r - 1.0).abs() < 1e-5);
        let above = g.sample(2.0);
        assert!((above.b - 1.0).abs() < 1e-5);
    }

    #[test]
    fn three_stops() {
        let g = Gradient::new(vec![
            GradientStop::opaque(0.0, Rgb::new(1.0, 0.0, 0.0)),
            GradientStop::opaque(0.5, Rgb::new(0.0, 1.0, 0.0)),
            GradientStop::opaque(1.0, Rgb::new(0.0, 0.0, 1.0)),
        ]);
        let at_quarter = g.sample(0.25);
        assert!((at_quarter.r - 0.5).abs() < 1e-5);
        assert!((at_quarter.g - 0.5).abs() < 1e-5);
    }

    #[test]
    fn wrapped_sampling() {
        let g = Gradient::two(Rgb::BLACK, Rgb::WHITE);
        let s = g.sample_wrapped(1.5);
        assert!((s.r - 0.5).abs() < 1e-5);
    }

    #[test]
    fn alpha_interpolates() {
        let g = Gradient::new(vec![
            GradientStop { position: 0.0, color: Rgb::WHITE, alpha: 0.0 },
            GradientStop { position: 1.0, color: Rgb::WHITE, alpha: 1.0 },
        ]);
        let (_, a) = g.sample_with_alpha(0.5);
        assert!((a - 0.5).abs() < 1e-5);
    }

    #[test]
    fn channels_roundtrip() {
        let g = Gradient::new(vec![
            GradientStop { position: 0.0, color: Rgb::new(1.0, 0.0, 0.0), alpha: 0.0 },
            GradientStop { position: 0.5, color: Rgb::new(0.0, 1.0, 0.0), alpha: 1.0 },
            GradientStop { position: 1.0, color: Rgb::new(0.0, 0.0, 1.0), alpha: 0.0 },
        ]);
        let ch = g.to_channels();
        let g2 = Gradient::from_channels(&ch);
        assert_eq!(g.stops().len(), g2.stops().len());
    }

    #[test]
    fn channels_skip_unused() {
        // Three stops + five unused markers.
        let mut ch = [0.0f32; GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS];
        // Stop 0: red @ 0.0, alpha 1.0
        ch[0] = 1.0; ch[3] = 1.0; ch[4] = 0.0;
        // Stop 1: green @ 1.0, alpha 1.0
        ch[5 * 1 + 1] = 1.0; ch[5 * 1 + 3] = 1.0; ch[5 * 1 + 4] = 1.0;
        // Stop 2-7: alpha = -1 (unused)
        for i in 2..GRADIENT_STOP_COUNT {
            ch[5 * i + 3] = -1.0;
        }
        let g = Gradient::from_channels(&ch);
        assert_eq!(g.stops().len(), 2);
    }
}
