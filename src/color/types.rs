/// RGB color with float components in 0.0..=1.0 range.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

/// RGBA color with float components in 0.0..=1.0 range.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// HSV color with hue in 0.0..=1.0 (wrapping), saturation and value in 0.0..=1.0.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

impl Rgb {
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0 };
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0 };

    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// Create from 0–255 integer values.
    pub fn from_u8(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
        }
    }

    /// Convert to 0–255 integer tuple.
    pub fn to_u8(self) -> (u8, u8, u8) {
        (
            (self.r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.b.clamp(0.0, 1.0) * 255.0).round() as u8,
        )
    }

    /// Linear interpolation between two colors.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
        }
    }

    /// Clamp all components to 0.0..=1.0.
    pub fn clamped(self) -> Self {
        Self {
            r: self.r.clamp(0.0, 1.0),
            g: self.g.clamp(0.0, 1.0),
            b: self.b.clamp(0.0, 1.0),
        }
    }

    /// Scale brightness by a factor.
    pub fn scaled(self, factor: f32) -> Self {
        Self {
            r: self.r * factor,
            g: self.g * factor,
            b: self.b * factor,
        }
    }

    pub fn to_rgba(self, a: f32) -> Rgba {
        Rgba { r: self.r, g: self.g, b: self.b, a }
    }

    pub fn to_egui(self) -> egui::Color32 {
        let (r, g, b) = self.to_u8();
        egui::Color32::from_rgb(r, g, b)
    }

    pub fn from_egui(c: egui::Color32) -> Self {
        Self::from_u8(c.r(), c.g(), c.b())
    }
}

impl Rgba {
    pub const TRANSPARENT: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    pub fn clamped(self) -> Self {
        Self {
            r: self.r.clamp(0.0, 1.0),
            g: self.g.clamp(0.0, 1.0),
            b: self.b.clamp(0.0, 1.0),
            a: self.a.clamp(0.0, 1.0),
        }
    }

    pub fn to_rgb(self) -> Rgb {
        Rgb { r: self.r, g: self.g, b: self.b }
    }

    /// Premultiply RGB by alpha.
    pub fn premultiplied(self) -> Self {
        Self {
            r: self.r * self.a,
            g: self.g * self.a,
            b: self.b * self.a,
            a: self.a,
        }
    }
}

impl Hsv {
    pub const fn new(h: f32, s: f32, v: f32) -> Self {
        Self { h, s, v }
    }

    /// Rotate hue by `amount` (wraps around 0..1).
    pub fn with_rotated_hue(self, amount: f32) -> Self {
        Self {
            h: (self.h + amount).rem_euclid(1.0),
            s: self.s,
            v: self.v,
        }
    }

    pub fn with_saturation(self, s: f32) -> Self {
        Self { h: self.h, s: s.clamp(0.0, 1.0), v: self.v }
    }

    pub fn with_value(self, v: f32) -> Self {
        Self { h: self.h, s: self.s, v: v.clamp(0.0, 1.0) }
    }
}

impl Default for Rgb {
    fn default() -> Self { Self::BLACK }
}

impl Default for Rgba {
    fn default() -> Self { Self::BLACK }
}

impl Default for Hsv {
    fn default() -> Self { Self::new(0.0, 0.0, 0.0) }
}

impl From<Rgb> for Rgba {
    fn from(c: Rgb) -> Self { c.to_rgba(1.0) }
}

impl From<Rgba> for Rgb {
    fn from(c: Rgba) -> Self { c.to_rgb() }
}
