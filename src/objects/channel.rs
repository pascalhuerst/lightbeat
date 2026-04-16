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

/// Pixel layout for an LED strip.
/// Defines the byte order of one pixel on the wire as a sequence of color
/// channels. Each W channel carries its own temperature in Kelvin (metadata
/// describing the physical LED, used to extract a white component from RGB).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PixelFormat {
    pub channels: Vec<PixelChannel>,
}

// Manual deserializer that also accepts the legacy enum form
// (`{"Rgb": null}`, `{"Rgbw": {"white_temperature": 6500}}`, etc.).
impl<'de> serde::Deserialize<'de> for PixelFormat {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(deserializer)?;

        // New form: {"channels": [...]}
        if let Some(channels) = v.get("channels") {
            let channels: Vec<PixelChannel> = serde_json::from_value(channels.clone())
                .map_err(serde::de::Error::custom)?;
            return Ok(PixelFormat { channels });
        }

        // Plain string form: "Rgb", "Grb"
        if let Some(s) = v.as_str() {
            return Ok(match s {
                "Rgb" => PixelFormat::rgb(),
                "Grb" => PixelFormat::grb(),
                _ => return Err(serde::de::Error::custom(format!("unknown PixelFormat \"{}\"", s))),
            });
        }

        // Legacy externally-tagged enum: { "Rgbw": { "white_temperature": 6500 } } etc.
        if let Some(obj) = v.as_object() {
            if let Some((tag, body)) = obj.iter().next() {
                let temp = body.get("white_temperature")
                    .or_else(|| body.get("warm_temperature"))
                    .and_then(|t| t.as_u64())
                    .unwrap_or(6500) as u16;
                return Ok(match tag.as_str() {
                    "Rgb" => PixelFormat::rgb(),
                    "Grb" => PixelFormat::grb(),
                    "Rgbw" => PixelFormat::rgbw(temp),
                    "Grbw" => PixelFormat::grbw(temp),
                    "Grbww" => PixelFormat::grbww(temp, temp),
                    other => return Err(serde::de::Error::custom(format!("unknown PixelFormat tag \"{}\"", other))),
                });
            }
        }
        Err(serde::de::Error::custom("could not parse PixelFormat"))
    }
}

/// A single byte slot in a pixel.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PixelChannel {
    Red,
    Green,
    Blue,
    /// White channel with its physical color temperature in Kelvin
    /// (e.g. 3000 = warm white, 6500 = daylight). The DMX writer
    /// extracts a white component from RGB using this temperature.
    White { temperature_k: u16 },
}

impl PixelChannel {
    pub fn label(self) -> &'static str {
        match self {
            PixelChannel::Red => "R",
            PixelChannel::Green => "G",
            PixelChannel::Blue => "B",
            PixelChannel::White { .. } => "W",
        }
    }
}

impl PixelFormat {
    pub fn new(channels: Vec<PixelChannel>) -> Self { Self { channels } }

    pub fn bytes_per_pixel(&self) -> usize { self.channels.len() }
    pub fn floats_per_pixel(&self) -> usize { 3 } // always stored as RGB internally

    /// Display label like "GRBWW" (white channels show as W regardless of temp).
    pub fn label(&self) -> String {
        self.channels.iter().map(|c| c.label()).collect::<Vec<_>>().join("")
    }

    // Common presets.
    pub fn rgb() -> Self {
        Self::new(vec![PixelChannel::Red, PixelChannel::Green, PixelChannel::Blue])
    }
    pub fn grb() -> Self {
        Self::new(vec![PixelChannel::Green, PixelChannel::Red, PixelChannel::Blue])
    }
    pub fn rgbw(temp: u16) -> Self {
        Self::new(vec![
            PixelChannel::Red, PixelChannel::Green, PixelChannel::Blue,
            PixelChannel::White { temperature_k: temp },
        ])
    }
    pub fn grbw(temp: u16) -> Self {
        Self::new(vec![
            PixelChannel::Green, PixelChannel::Red, PixelChannel::Blue,
            PixelChannel::White { temperature_k: temp },
        ])
    }
    pub fn grbww(t1: u16, t2: u16) -> Self {
        Self::new(vec![
            PixelChannel::Green, PixelChannel::Red, PixelChannel::Blue,
            PixelChannel::White { temperature_k: t1 },
            PixelChannel::White { temperature_k: t2 },
        ])
    }
}

/// The kind of channel, determining what value it holds and how it serializes to DMX.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ChannelKind {
    /// Single intensity value (0.0–1.0) → 1 DMX channel.
    Dimmer,
    /// Color value → 2–4 DMX channels depending on mode.
    Color { mode: ColorMode },
    /// Pan/Tilt → 2 DMX channels (coarse only) or 4 (with fine).
    PanTilt { fine: bool },
    /// Arbitrary float values → N DMX channels.
    Raw { count: usize },
    /// LED strip with N pixels. Stored internally as N×3 floats (RGB).
    /// Serializes to N × bytes_per_pixel DMX channels.
    LedStrip { count: usize, format: PixelFormat },
}

impl ChannelKind {
    /// Number of DMX channels this channel occupies.
    pub fn dmx_channel_count(&self) -> usize {
        match self {
            ChannelKind::Dimmer => 1,
            ChannelKind::Color { mode } => mode.channel_count(),
            ChannelKind::PanTilt { fine } => if *fine { 4 } else { 2 },
            ChannelKind::Raw { count } => *count,
            ChannelKind::LedStrip { count, format } => count * format.bytes_per_pixel(),
        }
    }
}

/// A channel on a fixture. Holds its current value and knows how to write DMX bytes.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

    /// Create an LED strip channel with N pixels.
    pub fn led_strip(name: impl Into<String>, count: usize, format: PixelFormat) -> Self {
        Self {
            name: name.into(),
            kind: ChannelKind::LedStrip { count, format },
            offset: 0,
            values: vec![0.0; count * 3], // RGB internally
        }
    }

    /// Number of pixels in this LED strip channel (0 if not a strip).
    pub fn pixel_count(&self) -> usize {
        match self.kind {
            ChannelKind::LedStrip { count, .. } => count,
            _ => 0,
        }
    }

    /// Set one pixel of an LED strip (no-op if not a strip or index out of range).
    pub fn set_pixel(&mut self, index: usize, color: Rgb) {
        if let ChannelKind::LedStrip { count, .. } = self.kind {
            if index < count {
                let base = index * 3;
                self.values[base] = color.r;
                self.values[base + 1] = color.g;
                self.values[base + 2] = color.b;
            }
        }
    }

    /// Get one pixel of an LED strip.
    pub fn pixel(&self, index: usize) -> Rgb {
        if let ChannelKind::LedStrip { count, .. } = self.kind {
            if index < count {
                let base = index * 3;
                return Rgb::new(self.values[base], self.values[base + 1], self.values[base + 2]);
            }
        }
        Rgb::new(0.0, 0.0, 0.0)
    }

    /// Clear all pixels (set to black).
    pub fn clear_pixels(&mut self) {
        if matches!(self.kind, ChannelKind::LedStrip { .. }) {
            for v in &mut self.values {
                *v = 0.0;
            }
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
            ChannelKind::LedStrip { count, format } => {
                let bpp = format.bytes_per_pixel();
                if bpp == 0 { return; }
                for px in 0..*count {
                    let pos = start + px * bpp;
                    if pos + bpp > 512 {
                        break;
                    }
                    let base = px * 3;
                    let r = self.values.get(base).copied().unwrap_or(0.0);
                    let g = self.values.get(base + 1).copied().unwrap_or(0.0);
                    let b = self.values.get(base + 2).copied().unwrap_or(0.0);
                    let rgb = Rgb::new(r, g, b);

                    for (i, ch) in format.channels.iter().enumerate() {
                        let val = match ch {
                            PixelChannel::Red => r,
                            PixelChannel::Green => g,
                            PixelChannel::Blue => b,
                            PixelChannel::White { temperature_k } => {
                                rgb_to_rgbw(rgb, *temperature_k).w
                            }
                        };
                        buf[pos + i] = float_to_dmx(val);
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
