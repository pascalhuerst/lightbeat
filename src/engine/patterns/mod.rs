//! Patterns are render functions that produce a per-LED `(Rgb, alpha)` buffer
//! for each strip in a target group. The Effect Stack composes multiple
//! patterns into a final image using `color::blend::BlendMode`.

use crate::color::Rgb;
use crate::engine::types::{PortDef, PortType};
use crate::objects::group::StripLayout;

pub mod bar;
pub mod solid;

/// One render slot in the per-strip pattern buffer: color + coverage alpha.
/// Alpha 0.0 = "this pattern doesn't touch this pixel" (transparent),
/// 1.0 = fully painted by this pattern.
#[derive(Debug, Clone, Copy)]
pub struct Pixel {
    pub color: Rgb,
    pub alpha: f32,
}

impl Pixel {
    pub const TRANSPARENT: Self = Self { color: Rgb::BLACK, alpha: 0.0 };
    pub fn opaque(color: Rgb) -> Self { Self { color, alpha: 1.0 } }
}

/// Render target passed to each pattern: a per-strip buffer plus the strip's
/// pixel count and layout entry, so the pattern can map logical 0..1 axis
/// positions to LED indices.
pub struct StripFrame<'a> {
    pub layout: &'a StripLayout,
    pub pixel_count: usize,
    pub buf: &'a mut [Pixel],
}

/// A pattern declares its input-port shape and a render function.
pub trait Pattern: Send {
    /// Stable type name (used for save/load).
    fn type_name(&self) -> &'static str;

    /// Input ports this pattern needs. Total channel count = sum of
    /// `port_type.channel_count()`. Inputs are fed to `render` in this order
    /// by channel.
    fn input_ports(&self) -> Vec<PortDef>;

    /// Render the pattern into every strip frame, given input channel values.
    fn render(&self, inputs: &[f32], frames: &mut [StripFrame<'_>]);
}

// ---------------------------------------------------------------------------
// Pattern factory — central registry of pattern types.
// ---------------------------------------------------------------------------

pub fn create_pattern(type_name: &str) -> Option<Box<dyn Pattern>> {
    match type_name {
        "Bar" => Some(Box::new(bar::BarPattern)),
        "Solid" => Some(Box::new(solid::SolidPattern)),
        _ => None,
    }
}

/// All available pattern type names (for the inspector dropdown).
pub fn all_pattern_types() -> &'static [&'static str] {
    &["Bar", "Solid"]
}

/// Compute total input channel count for a pattern type.
pub fn pattern_channel_count(type_name: &str) -> usize {
    create_pattern(type_name)
        .map(|p| p.input_ports().iter().map(|d| d.port_type.channel_count()).sum())
        .unwrap_or(0)
}
