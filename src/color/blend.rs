use super::Rgb;

/// Blend modes for compositing colors, matching common lighting console behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[derive(Default)]
pub enum BlendMode {
    /// Replace base with effect color.
    #[default]
    Override,
    /// Add effect color to base (can exceed 1.0, clamp at output).
    Add,
    /// Take the brighter channel of base and effect.
    Max,
    /// Take the dimmer channel of base and effect.
    Min,
    /// Multiply base by effect (darkens).
    Multiply,
}

impl BlendMode {
    /// Blend a single float value.
    pub fn blend_float(self, base: f32, effect: f32, weight: f32) -> f32 {
        let target = match self {
            BlendMode::Override => effect,
            BlendMode::Add => base + effect,
            BlendMode::Max => base.max(effect),
            BlendMode::Min => base.min(effect),
            BlendMode::Multiply => base * effect,
        };
        // Interpolate between base and blended value by weight
        base + (target - base) * weight
    }

    /// Blend two RGB colors with a weight (0.0 = base only, 1.0 = full effect).
    pub fn blend(self, base: Rgb, effect: Rgb, weight: f32) -> Rgb {
        Rgb {
            r: self.blend_float(base.r, effect.r, weight),
            g: self.blend_float(base.g, effect.g, weight),
            b: self.blend_float(base.b, effect.b, weight),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_full_weight() {
        let base = Rgb::new(0.2, 0.4, 0.6);
        let effect = Rgb::new(1.0, 0.0, 0.5);
        let result = BlendMode::Override.blend(base, effect, 1.0);
        assert!((result.r - 1.0).abs() < 1e-5);
        assert!((result.g - 0.0).abs() < 1e-5);
        assert!((result.b - 0.5).abs() < 1e-5);
    }

    #[test]
    fn override_half_weight() {
        let base = Rgb::new(0.0, 0.0, 0.0);
        let effect = Rgb::new(1.0, 1.0, 1.0);
        let result = BlendMode::Override.blend(base, effect, 0.5);
        assert!((result.r - 0.5).abs() < 1e-5);
    }

    #[test]
    fn additive_blend() {
        let base = Rgb::new(0.3, 0.4, 0.5);
        let effect = Rgb::new(0.2, 0.1, 0.3);
        let result = BlendMode::Add.blend(base, effect, 1.0);
        assert!((result.r - 0.5).abs() < 1e-5);
        assert!((result.g - 0.5).abs() < 1e-5);
        assert!((result.b - 0.8).abs() < 1e-5);
    }

    #[test]
    fn multiply_blend() {
        let base = Rgb::new(0.5, 1.0, 0.8);
        let effect = Rgb::new(0.5, 0.5, 0.5);
        let result = BlendMode::Multiply.blend(base, effect, 1.0);
        assert!((result.r - 0.25).abs() < 1e-5);
        assert!((result.g - 0.5).abs() < 1e-5);
        assert!((result.b - 0.4).abs() < 1e-5);
    }
}
