use super::{Rgb, Hsv};

/// Conversion trait for color types.
pub trait ColorConvert {
    fn to_rgb(&self) -> Rgb;
    fn to_hsv(&self) -> Hsv;
}

impl ColorConvert for Rgb {
    fn to_rgb(&self) -> Rgb {
        *self
    }

    fn to_hsv(&self) -> Hsv {
        let r = self.r;
        let g = self.g;
        let b = self.b;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;

        let h = if delta == 0.0 {
            0.0
        } else if max == r {
            ((g - b) / delta).rem_euclid(6.0) / 6.0
        } else if max == g {
            ((b - r) / delta + 2.0) / 6.0
        } else {
            ((r - g) / delta + 4.0) / 6.0
        };

        let s = if max == 0.0 { 0.0 } else { delta / max };
        let v = max;

        Hsv { h, s, v }
    }
}

impl ColorConvert for Hsv {
    fn to_rgb(&self) -> Rgb {
        let h = self.h.rem_euclid(1.0) * 6.0;
        let s = self.s.clamp(0.0, 1.0);
        let v = self.v.clamp(0.0, 1.0);

        let c = v * s;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let m = v - c;

        let (r, g, b) = if h < 1.0 {
            (c, x, 0.0)
        } else if h < 2.0 {
            (x, c, 0.0)
        } else if h < 3.0 {
            (0.0, c, x)
        } else if h < 4.0 {
            (0.0, x, c)
        } else if h < 5.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };

        Rgb {
            r: r + m,
            g: g + m,
            b: b + m,
        }
    }

    fn to_hsv(&self) -> Hsv {
        *self
    }
}

/// Convert RGB to CMY (subtractive color model used by some fixtures).
pub fn rgb_to_cmy(c: Rgb) -> (f32, f32, f32) {
    (1.0 - c.r, 1.0 - c.g, 1.0 - c.b)
}

/// Convert CMY back to RGB.
pub fn cmy_to_rgb(c: f32, m: f32, y: f32) -> Rgb {
    Rgb::new(1.0 - c, 1.0 - m, 1.0 - y)
}

/// Naive RGBW extraction (assumes pure white LED, no temperature correction).
/// For temperature-aware extraction, use `color::temperature::rgb_to_rgbw()`.
pub fn rgb_to_rgbw_naive(c: Rgb) -> (f32, f32, f32, f32) {
    let w = c.r.min(c.g).min(c.b);
    (c.r - w, c.g - w, c.b - w, w)
}

/// Convert DMX-style 0–255 byte values to 0.0–1.0 float.
pub fn dmx_to_float(v: u8) -> f32 {
    v as f32 / 255.0
}

/// Convert 0.0–1.0 float to DMX-style 0–255 byte.
pub fn float_to_dmx(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_to_hsv_roundtrip() {
        let colors = [
            Rgb::new(1.0, 0.0, 0.0), // red
            Rgb::new(0.0, 1.0, 0.0), // green
            Rgb::new(0.0, 0.0, 1.0), // blue
            Rgb::new(1.0, 1.0, 1.0), // white
            Rgb::new(0.0, 0.0, 0.0), // black
            Rgb::new(0.5, 0.3, 0.8), // arbitrary
        ];

        for c in colors {
            let hsv = c.to_hsv();
            let back = hsv.to_rgb();
            assert!((c.r - back.r).abs() < 1e-5, "r mismatch for {:?}", c);
            assert!((c.g - back.g).abs() < 1e-5, "g mismatch for {:?}", c);
            assert!((c.b - back.b).abs() < 1e-5, "b mismatch for {:?}", c);
        }
    }

    #[test]
    fn rgbw_naive_extraction() {
        let (r, g, b, w) = rgb_to_rgbw_naive(Rgb::new(0.8, 0.5, 0.3));
        assert!((w - 0.3).abs() < 1e-5);
        assert!((r - 0.5).abs() < 1e-5);
        assert!((g - 0.2).abs() < 1e-5);
        assert!((b - 0.0).abs() < 1e-5);
    }
}
