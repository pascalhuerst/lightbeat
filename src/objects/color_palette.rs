use serde::{Deserialize, Serialize};
use crate::color::Rgb;

pub const STACK_SIZE: usize = 4;
pub const SLOT_NAMES: [&str; STACK_SIZE] = ["Primary", "Secondary", "Third", "Fourth"];

/// A fixed set of 4 named colors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorStack {
    pub id: u32,
    pub name: String,
    pub colors: [Rgb; STACK_SIZE],
}

impl ColorStack {
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

/// A named collection of color stacks. A stack can appear in multiple groups.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorGroup {
    pub id: u32,
    pub name: String,
    pub stack_ids: Vec<u32>,
}

impl ColorGroup {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            stack_ids: Vec::new(),
        }
    }
}
