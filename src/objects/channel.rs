use crate::color::{Rgb, ColorConvert};
use crate::color::convert::{rgb_to_cmy, float_to_dmx};
use crate::color::temperature::rgb_to_rgbw;

/// How a color channel maps to DMX bytes.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ColorMode {
    /// 3 channels: R, G, B
    Rgb,
    /// 4 channels: R, G, B, W — white extracted using the LED's color temperature.
    /// The temperature (in Kelvin) determines how the white channel blends.
    /// Common values: 2700 (warm), 4000 (neutral), 6500 (daylight).
    Rgbw { white_temperature: u16 },
    /// 3 channels: C, M, Y (subtractive)
    Cmy,
    /// 2 channels: Hue, Saturation
    Hs,
}

impl ColorMode {
    /// Number of DMX channels this mode occupies.
    pub fn channel_count(self) -> usize {
        match self {
            ColorMode::Rgb | ColorMode::Cmy => 3,
            ColorMode::Rgbw { .. } => 4,
            ColorMode::Hs => 2,
        }
    }
}

/// The kind of channel, determining what value it holds and how it serializes to DMX.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ChannelKind {
    /// Single intensity value (0.0–1.0) → 1 DMX channel.
    Dimmer,
    /// Color value → 2–4 DMX channels depending on mode.
    Color { mode: ColorMode },
    /// Pan/Tilt → 2 DMX channels (coarse only) or 4 (with fine).
    PanTilt { fine: bool },
    /// Arbitrary float values → N DMX channels.
    Raw { count: usize },
}

impl ChannelKind {
    /// Number of DMX channels this channel occupies.
    pub fn dmx_channel_count(&self) -> usize {
        match self {
            ChannelKind::Dimmer => 1,
            ChannelKind::Color { mode } => mode.channel_count(),
            ChannelKind::PanTilt { fine } => if *fine { 4 } else { 2 },
            ChannelKind::Raw { count } => *count,
        }
    }
}

/// A channel on a fixture. Holds its current value and knows how to write DMX bytes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Channel {
    pub name: String,
    pub kind: ChannelKind,
    /// Offset from the fixture's base address (0-based).
    pub offset: u16,
    /// Current value(s). Meaning depends on kind:
    /// - Dimmer: [intensity]
    /// - Color: [r, g, b] (always stored as RGB internally)
    /// - PanTilt: [pan, tilt] (0.0–1.0)
    /// - Raw: [v0, v1, ..., vN]
    pub values: Vec<f32>,
}

impl Channel {
    pub fn dimmer(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: ChannelKind::Dimmer,
            offset: 0, // auto-assigned by Fixture::recalc_offsets()
            values: vec![0.0],
        }
    }

    pub fn color(name: impl Into<String>, mode: ColorMode) -> Self {
        Self {
            name: name.into(),
            kind: ChannelKind::Color { mode },
            offset: 0,
            values: vec![0.0, 0.0, 0.0], // always RGB internally
        }
    }

    pub fn pan_tilt(name: impl Into<String>, fine: bool) -> Self {
        Self {
            name: name.into(),
            kind: ChannelKind::PanTilt { fine },
            offset: 0,
            values: vec![0.0, 0.0], // pan, tilt
        }
    }

    pub fn raw(name: impl Into<String>, count: usize) -> Self {
        Self {
            name: name.into(),
            kind: ChannelKind::Raw { count },
            offset: 0,
            values: vec![0.0; count],
        }
    }

    // -- Typed accessors --

    /// Get dimmer intensity (0.0–1.0).
    pub fn dimmer_value(&self) -> f32 {
        self.values[0]
    }

    /// Set dimmer intensity.
    pub fn set_dimmer(&mut self, v: f32) {
        self.values[0] = v.clamp(0.0, 1.0);
    }

    /// Get color as Rgb.
    pub fn color_rgb(&self) -> Rgb {
        Rgb::new(self.values[0], self.values[1], self.values[2])
    }

    /// Set color from Rgb.
    pub fn set_color(&mut self, c: Rgb) {
        self.values[0] = c.r;
        self.values[1] = c.g;
        self.values[2] = c.b;
    }

    /// Get pan value (0.0–1.0).
    pub fn pan(&self) -> f32 {
        self.values[0]
    }

    /// Get tilt value (0.0–1.0).
    pub fn tilt(&self) -> f32 {
        self.values[1]
    }

    /// Set pan and tilt.
    pub fn set_pan_tilt(&mut self, pan: f32, tilt: f32) {
        self.values[0] = pan.clamp(0.0, 1.0);
        self.values[1] = tilt.clamp(0.0, 1.0);
    }

    /// Write this channel's current value(s) into a DMX buffer at the correct offset.
    /// `base_address` is the fixture's 0-based start address in the universe.
    pub fn write_dmx(&self, buf: &mut [u8; 512], base_address: u16) {
        let start = (base_address + self.offset) as usize;
        if start >= 512 {
            return;
        }

        match &self.kind {
            ChannelKind::Dimmer => {
                if start < 512 {
                    buf[start] = float_to_dmx(self.values[0]);
                }
            }
            ChannelKind::Color { mode } => {
                let rgb = self.color_rgb();
                match mode {
                    ColorMode::Rgb => {
                        write_bytes(buf, start, &[
                            float_to_dmx(rgb.r),
                            float_to_dmx(rgb.g),
                            float_to_dmx(rgb.b),
                        ]);
                    }
                    ColorMode::Rgbw { white_temperature } => {
                        let rgbw = rgb_to_rgbw(rgb, *white_temperature);
                        write_bytes(buf, start, &[
                            float_to_dmx(rgbw.r),
                            float_to_dmx(rgbw.g),
                            float_to_dmx(rgbw.b),
                            float_to_dmx(rgbw.w),
                        ]);
                    }
                    ColorMode::Cmy => {
                        let (c, m, y) = rgb_to_cmy(rgb);
                        write_bytes(buf, start, &[
                            float_to_dmx(c),
                            float_to_dmx(m),
                            float_to_dmx(y),
                        ]);
                    }
                    ColorMode::Hs => {
                        let hsv = rgb.to_hsv();
                        write_bytes(buf, start, &[
                            float_to_dmx(hsv.h),
                            float_to_dmx(hsv.s),
                        ]);
                    }
                }
            }
            ChannelKind::PanTilt { fine } => {
                let pan = self.values[0].clamp(0.0, 1.0);
                let tilt = self.values[1].clamp(0.0, 1.0);

                if *fine {
                    let (pan_c, pan_f) = float_to_dmx_16bit(pan);
                    let (tilt_c, tilt_f) = float_to_dmx_16bit(tilt);
                    write_bytes(buf, start, &[pan_c, pan_f, tilt_c, tilt_f]);
                } else {
                    write_bytes(buf, start, &[
                        float_to_dmx(pan),
                        float_to_dmx(tilt),
                    ]);
                }
            }
            ChannelKind::Raw { count } => {
                for i in 0..*count {
                    let idx = start + i;
                    if idx < 512 {
                        buf[idx] = float_to_dmx(self.values.get(i).copied().unwrap_or(0.0));
                    }
                }
            }
        }
    }
}

/// Split a 0.0–1.0 float into coarse + fine DMX bytes (16-bit precision).
fn float_to_dmx_16bit(v: f32) -> (u8, u8) {
    let val = (v.clamp(0.0, 1.0) * 65535.0).round() as u16;
    ((val >> 8) as u8, (val & 0xFF) as u8)
}

/// Write bytes into a DMX buffer, respecting the 512-channel limit.
fn write_bytes(buf: &mut [u8; 512], start: usize, bytes: &[u8]) {
    for (i, &b) in bytes.iter().enumerate() {
        let idx = start + i;
        if idx < 512 {
            buf[idx] = b;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimmer_writes_correct_channel() {
        let mut ch = Channel::dimmer("Dim");
        ch.set_dimmer(0.5);
        let mut buf = [0u8; 512];
        ch.write_dmx(&mut buf, 10);
        assert_eq!(buf[10], 128); // 0.5 * 255 ≈ 128
    }

    #[test]
    fn color_rgb_writes_three_channels() {
        let mut ch = Channel::color("Col", ColorMode::Rgb);
        ch.offset = 1;
        ch.set_color(Rgb::new(1.0, 0.5, 0.0));
        let mut buf = [0u8; 512];
        ch.write_dmx(&mut buf, 0);
        assert_eq!(buf[1], 255);
        assert_eq!(buf[2], 128);
        assert_eq!(buf[3], 0);
    }

    #[test]
    fn pan_tilt_fine_writes_four_channels() {
        let mut ch = Channel::pan_tilt("PT", true);
        ch.set_pan_tilt(0.5, 1.0);
        let mut buf = [0u8; 512];
        ch.write_dmx(&mut buf, 0);
        // pan=0.5 → 32768 → coarse=128, fine=0
        assert_eq!(buf[0], 128);
        assert_eq!(buf[1], 0);
        // tilt=1.0 → 65535 → coarse=255, fine=255
        assert_eq!(buf[2], 255);
        assert_eq!(buf[3], 255);
    }

    #[test]
    fn channel_count_matches_kind() {
        assert_eq!(ChannelKind::Dimmer.dmx_channel_count(), 1);
        assert_eq!((ChannelKind::Color { mode: ColorMode::Rgb }).dmx_channel_count(), 3);
        assert_eq!((ChannelKind::Color { mode: ColorMode::Rgbw { white_temperature: 6500 } }).dmx_channel_count(), 4);
        assert_eq!((ChannelKind::PanTilt { fine: false }).dmx_channel_count(), 2);
        assert_eq!((ChannelKind::PanTilt { fine: true }).dmx_channel_count(), 4);
    }
}
