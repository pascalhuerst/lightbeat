use super::Rgb;

/// Pre-computed color temperature table (1000K–12000K in 100K steps).
/// Values from the Planckian locus / blackbody radiation curve, matching
/// the table used by Blux/Chataigne for accurate RGBW LED white extraction.
const TEMP_TABLE: [(u16, (u8, u8, u8)); 111] = [
    (1000, (255, 56, 0)),
    (1100, (255, 71, 0)),
    (1200, (255, 83, 0)),
    (1300, (255, 93, 0)),
    (1400, (255, 101, 0)),
    (1500, (255, 109, 0)),
    (1600, (255, 115, 0)),
    (1700, (255, 121, 0)),
    (1800, (255, 126, 0)),
    (1900, (255, 131, 0)),
    (2000, (255, 138, 18)),
    (2100, (255, 142, 33)),
    (2200, (255, 147, 44)),
    (2300, (255, 152, 54)),
    (2400, (255, 157, 63)),
    (2500, (255, 161, 72)),
    (2600, (255, 165, 79)),
    (2700, (255, 169, 87)),
    (2800, (255, 173, 94)),
    (2900, (255, 177, 101)),
    (3000, (255, 180, 107)),
    (3100, (255, 184, 114)),
    (3200, (255, 187, 120)),
    (3300, (255, 190, 126)),
    (3400, (255, 193, 132)),
    (3500, (255, 196, 137)),
    (3600, (255, 199, 143)),
    (3700, (255, 201, 148)),
    (3800, (255, 204, 153)),
    (3900, (255, 206, 159)),
    (4000, (255, 209, 163)),
    (4100, (255, 211, 168)),
    (4200, (255, 213, 173)),
    (4300, (255, 215, 177)),
    (4400, (255, 217, 182)),
    (4500, (255, 219, 186)),
    (4600, (255, 221, 190)),
    (4700, (255, 223, 194)),
    (4800, (255, 225, 198)),
    (4900, (255, 227, 202)),
    (5000, (255, 228, 206)),
    (5100, (255, 230, 210)),
    (5200, (255, 232, 213)),
    (5300, (255, 233, 217)),
    (5400, (255, 235, 220)),
    (5500, (255, 236, 224)),
    (5600, (255, 238, 227)),
    (5700, (255, 239, 230)),
    (5800, (255, 240, 233)),
    (5900, (255, 242, 236)),
    (6000, (255, 243, 239)),
    (6100, (255, 244, 242)),
    (6200, (255, 245, 245)),
    (6300, (255, 246, 247)),
    (6400, (255, 248, 251)),
    (6500, (255, 249, 253)),
    (6600, (254, 249, 255)),
    (6700, (252, 247, 255)),
    (6800, (249, 246, 255)),
    (6900, (247, 245, 255)),
    (7000, (245, 243, 255)),
    (7100, (243, 242, 255)),
    (7200, (240, 241, 255)),
    (7300, (239, 240, 255)),
    (7400, (237, 239, 255)),
    (7500, (235, 238, 255)),
    (7600, (233, 237, 255)),
    (7700, (231, 236, 255)),
    (7800, (230, 235, 255)),
    (7900, (228, 234, 255)),
    (8000, (227, 233, 255)),
    (8100, (225, 232, 255)),
    (8200, (224, 231, 255)),
    (8300, (222, 230, 255)),
    (8400, (221, 230, 255)),
    (8500, (220, 229, 255)),
    (8600, (218, 229, 255)),
    (8700, (217, 227, 255)),
    (8800, (216, 227, 255)),
    (8900, (215, 226, 255)),
    (9000, (214, 225, 255)),
    (9100, (212, 225, 255)),
    (9200, (211, 224, 255)),
    (9300, (210, 223, 255)),
    (9400, (209, 223, 255)),
    (9500, (208, 222, 255)),
    (9600, (207, 221, 255)),
    (9700, (207, 221, 255)),
    (9800, (206, 220, 255)),
    (9900, (205, 220, 255)),
    (10000, (207, 218, 255)),
    (10100, (207, 218, 255)),
    (10200, (206, 217, 255)),
    (10300, (205, 217, 255)),
    (10400, (204, 216, 255)),
    (10500, (204, 216, 255)),
    (10600, (203, 215, 255)),
    (10700, (202, 215, 255)),
    (10800, (202, 214, 255)),
    (10900, (201, 214, 255)),
    (11000, (200, 213, 255)),
    (11100, (200, 213, 255)),
    (11200, (199, 212, 255)),
    (11300, (198, 212, 255)),
    (11400, (198, 212, 255)),
    (11500, (197, 211, 255)),
    (11600, (197, 211, 255)),
    (11700, (197, 210, 255)),
    (11800, (196, 210, 255)),
    (11900, (195, 210, 255)),
    (12000, (195, 209, 255)),
];

/// Get the RGB tint of a white LED at a given color temperature (Kelvin).
/// Clamped to 1000–12000K, snapped to nearest 100K.
pub fn white_point(kelvin: u16) -> Rgb {
    let k = kelvin.clamp(1000, 12000);
    let k = (k / 100) * 100; // snap to 100K

    for &(temp, (r, g, b)) in &TEMP_TABLE {
        if temp == k {
            return Rgb::from_u8(r, g, b);
        }
    }

    // Fallback (shouldn't happen with valid clamped input)
    Rgb::WHITE
}

/// Result of RGBW decomposition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgbw {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub w: f32,
}

/// Extract an RGBW decomposition from an RGB color, accounting for the white
/// LED's color temperature.
///
/// A warm white LED (e.g. 2700K) emits a tinted white — roughly (255, 169, 87).
/// Naively using `min(r,g,b)` as white would over-drive the red channel because
/// the white LED already contributes significant red. This function divides each
/// channel by the white LED's tint to correctly determine how much white to use.
///
/// `temperature_kelvin`: the color temperature of the fixture's white LED
/// (common values: 2700 warm, 4000 neutral, 6500 daylight).
pub fn rgb_to_rgbw(color: Rgb, temperature_kelvin: u16) -> Rgbw {
    let wp = white_point(temperature_kelvin);

    // How much white would each channel need (normalized by white tint)?
    // Guard against division by zero for very low temperatures where blue=0.
    let w_r = if wp.r > 0.001 { color.r / wp.r } else { f32::MAX };
    let w_g = if wp.g > 0.001 { color.g / wp.g } else { f32::MAX };
    let w_b = if wp.b > 0.001 { color.b / wp.b } else { f32::MAX };

    // The limiting channel determines max white we can use.
    let w_min = w_r.min(w_g).min(w_b);

    // The white output is the actual value of the limiting channel.
    let w_out = if w_min == w_r {
        color.r
    } else if w_min == w_g {
        color.g
    } else {
        color.b
    };

    // Subtract the white LED's contribution from each RGB channel.
    let r_out = (color.r - w_out * wp.r).max(0.0);
    let g_out = (color.g - w_out * wp.g).max(0.0);
    let b_out = (color.b - w_out * wp.b).max(0.0);

    Rgbw {
        r: r_out,
        g: g_out,
        b: b_out,
        w: w_out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_white_at_6500k_goes_mostly_to_w() {
        let rgbw = rgb_to_rgbw(Rgb::WHITE, 6500);
        // At 6500K the white point is nearly (1, 0.976, 0.992) so almost all
        // goes to W with tiny RGB residuals.
        assert!(rgbw.w > 0.95, "w={}", rgbw.w);
        assert!(rgbw.r < 0.05, "r={}", rgbw.r);
        assert!(rgbw.g < 0.05, "g={}", rgbw.g);
        assert!(rgbw.b < 0.05, "b={}", rgbw.b);
    }

    #[test]
    fn pure_red_stays_in_rgb() {
        let rgbw = rgb_to_rgbw(Rgb::new(1.0, 0.0, 0.0), 6500);
        assert!((rgbw.w - 0.0).abs() < 0.01);
        assert!(rgbw.r > 0.95);
    }

    #[test]
    fn warm_white_temperature_extracts_more_white() {
        // A warm-ish color that matches a 2700K white LED tint
        // should extract more white at 2700K than at 6500K.
        let color = Rgb::new(0.8, 0.5, 0.3);
        let warm = rgb_to_rgbw(color, 2700);
        let cool = rgb_to_rgbw(color, 6500);
        assert!(warm.w > cool.w,
            "warm.w={} should be > cool.w={}", warm.w, cool.w);
    }

    #[test]
    fn black_produces_zero() {
        let rgbw = rgb_to_rgbw(Rgb::BLACK, 6500);
        assert!((rgbw.r + rgbw.g + rgbw.b + rgbw.w) < 0.001);
    }

    #[test]
    fn white_point_clamping() {
        // Should not panic at extremes
        let _ = white_point(0);
        let _ = white_point(50000);
        let _ = white_point(6500);
    }
}
