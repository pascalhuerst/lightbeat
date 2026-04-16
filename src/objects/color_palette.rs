use serde::{Deserialize, Serialize};
use crate::color::Rgb;

pub const PALETTE_SIZE: usize = 4;
pub const SLOT_NAMES: [&str; PALETTE_SIZE] = ["Primary", "Secondary", "Third", "Fourth"];

/// A palette: a fixed set of 4 named colors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColorPalette {
    pub id: u32,
    pub name: String,
    pub colors: [Rgb; PALETTE_SIZE],
}

impl ColorPalette {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            colors: [
                Rgb::new(1.0, 0.0, 0.0), // primary: red
                Rgb::new(0.0, 0.0, 1.0), // secondary: blue
                Rgb::new(0.0, 1.0, 0.0), // third: green
                Rgb::new(1.0, 1.0, 0.0), // fourth: yellow
            ],
        }
    }
}

/// A named collection of palettes. A palette can appear in multiple groups.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColorPaletteGroup {
    pub id: u32,
    pub name: String,
    /// IDs of palettes belonging to this group.
    pub palette_ids: Vec<u32>,
}

impl ColorPaletteGroup {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            palette_ids: Vec::new(),
        }
    }
}
