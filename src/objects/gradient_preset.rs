use serde::{Deserialize, Serialize};

use crate::color::{GradientStop, Rgb};

/// Maximum stops per preset; matches `engine::types::GRADIENT_STOP_COUNT`.
/// We don't import that constant here to keep `objects` independent of the
/// engine module — the assertion in main.rs verifies they stay in sync.
pub const GRADIENT_PRESET_MAX_STOPS: usize = 8;

/// A named, reusable gradient preset stored in the setup library.
/// Mirrors how `ColorPalette` works: a flat library entry that can be
/// referenced from the Gradient Source widget to populate its stops.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradientPreset {
    pub id: u32,
    pub name: String,
    /// Stops in the order they were authored. The Gradient Source clamps
    /// this list to `GRADIENT_PRESET_MAX_STOPS` on load.
    pub stops: Vec<GradientStop>,
}

impl GradientPreset {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            stops: vec![
                GradientStop::opaque(0.0, Rgb::BLACK),
                GradientStop::opaque(1.0, Rgb::WHITE),
            ],
        }
    }
}
