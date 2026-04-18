// Re-export all engine types.
pub use crate::engine::types::*;

use egui::Color32;

// ---------------------------------------------------------------------------
// UI extensions for PortType
// ---------------------------------------------------------------------------

/// UI-specific methods for PortType (colors).
pub trait PortTypeUi {
    fn color(&self) -> Color32;
}

impl PortTypeUi for PortType {
    fn color(&self) -> Color32 {
        match self {
            PortType::Logic => Color32::from_rgb(240, 200, 40),
            PortType::Phase => Color32::from_rgb(180, 100, 220),
            PortType::Untyped => Color32::from_rgb(220, 80, 80),
            PortType::Any => Color32::from_gray(160),
            PortType::Color => Color32::from_rgb(60, 200, 220),
            PortType::Position => Color32::from_rgb(80, 200, 140),
            PortType::Palette => Color32::from_rgb(220, 180, 100),
            PortType::Gradient => Color32::from_rgb(40, 200, 180),
        }
    }
}

// ---------------------------------------------------------------------------
// UI-extended port definition (adds fill_color for rendering)
// ---------------------------------------------------------------------------

/// Port definition with optional UI fill color override and a small glyph
/// drawn inside the port circle (used for the variadic "+" add port).
#[derive(Debug, Clone)]
pub struct UiPortDef {
    pub def: PortDef,
    pub fill_color: Option<Color32>,
    pub marker: Option<&'static str>,
    /// When true, the port is rendered grayed out and the graph refuses
    /// connections targeting it. Used to keep the visual port layout stable
    /// while individual cells in a group toggle their input on/off.
    pub disabled: bool,
}

impl UiPortDef {
    pub fn from_def(def: &PortDef) -> Self {
        Self {
            def: def.clone(),
            fill_color: None,
            marker: None,
            disabled: false,
        }
    }

    pub fn with_fill(mut self, color: Color32) -> Self {
        self.fill_color = Some(color);
        self
    }

    pub fn with_marker(mut self, glyph: &'static str) -> Self {
        self.marker = Some(glyph);
        self
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}
