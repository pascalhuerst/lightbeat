use super::Rgb;

/// A single color stop in a gradient.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GradientStop {
    /// Position along the gradient, 0.0..=1.0.
    pub position: f32,
    pub color: Rgb,
}

/// A color gradient defined by sorted stops, for mapping a 0..1 value to a color.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Gradient {
    stops: Vec<GradientStop>,
}

impl Gradient {
    /// Create a gradient from stops. Stops will be sorted by position.
    pub fn new(mut stops: Vec<GradientStop>) -> Self {
        stops.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap());
        Self { stops }
    }

    /// Two-stop gradient from one color to another.
    pub fn two(start: Rgb, end: Rgb) -> Self {
        Self::new(vec![
            GradientStop { position: 0.0, color: start },
            GradientStop { position: 1.0, color: end },
        ])
    }

    /// Sample the gradient at a position (0.0..=1.0). Clamps to edges.
    pub fn sample(&self, t: f32) -> Rgb {
        if self.stops.is_empty() {
            return Rgb::BLACK;
        }
        if self.stops.len() == 1 || t <= self.stops[0].position {
            return self.stops[0].color;
        }
        let last = self.stops.last().unwrap();
        if t >= last.position {
            return last.color;
        }

        // Find surrounding stops
        for i in 1..self.stops.len() {
            if t <= self.stops[i].position {
                let a = &self.stops[i - 1];
                let b = &self.stops[i];
                let range = b.position - a.position;
                let local_t = if range > 0.0 { (t - a.position) / range } else { 0.0 };
                return a.color.lerp(b.color, local_t);
            }
        }

        last.color
    }

    /// Sample with wrapping (t wraps around 0..1 via modulo).
    pub fn sample_wrapped(&self, t: f32) -> Rgb {
        self.sample(t.rem_euclid(1.0))
    }

    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }
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
            GradientStop { position: 0.0, color: Rgb::new(1.0, 0.0, 0.0) },
            GradientStop { position: 0.5, color: Rgb::new(0.0, 1.0, 0.0) },
            GradientStop { position: 1.0, color: Rgb::new(0.0, 0.0, 1.0) },
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
}
