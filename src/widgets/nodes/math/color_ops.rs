use std::any::Any;
use egui::{self, Color32, Ui, Vec2, Sense};

use crate::engine::nodes::math::color_ops::{ColorMode, ColorOpsDisplay};
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

fn show_color_swatch(ui: &mut Ui, rgb: [f32; 3]) {
    let color = Color32::from_rgb(
        (rgb[0].clamp(0.0, 1.0) * 255.0) as u8,
        (rgb[1].clamp(0.0, 1.0) * 255.0) as u8,
        (rgb[2].clamp(0.0, 1.0) * 255.0) as u8,
    );
    let (resp, painter) = ui.allocate_painter(
        Vec2::new(ui.available_width(), 14.0),
        Sense::hover(),
    );
    painter.rect_filled(resp.rect, 2.0, color);
}

fn show_palette_swatches(ui: &mut Ui, channels: &[f32]) {
    for i in 0..4 {
        let base = i * 3;
        let r = channels.get(base).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let g = channels.get(base + 1).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let b = channels.get(base + 2).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let color = Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
        let remaining = 4 - i;
        let (resp, painter) = ui.allocate_painter(
            Vec2::new(ui.available_width(), ui.available_height() / remaining as f32),
            Sense::hover(),
        );
        painter.rect_filled(resp.rect, 1.0, color);
    }
}

/// Build the input port list shown in the UI for a given mode.
/// Component side = whichever side carries the per-channel components
/// (inputs for Merge, outputs for Split).
fn make_port_defs(mode: ColorMode, is_component_side: bool) -> Vec<PortDef> {
    match mode {
        ColorMode::Neutral => vec![PortDef::new("?", PortType::Any)],
        ColorMode::Palette =>
            mode.channel_names().iter().map(|n| PortDef::new(*n, PortType::Color)).collect(),
        _ => {
            if is_component_side {
                mode.channel_names().iter().map(|n| PortDef::new(*n, PortType::Untyped)).collect()
            } else {
                vec![PortDef::new("color", PortType::Color)]
            }
        }
    }
}

/// Push a mode change to the engine via the standard param channel.
fn push_mode(shared: &SharedState, mode: ColorMode) {
    let mut s = shared.lock().unwrap();
    s.pending_params.push((0, ParamValue::Choice(mode.to_index())));
}

/// Infer the mode for a Color Merge based on a newly seen wire type.
/// `is_input_side` = true when the wire was attached to one of the
/// component-side ports (which is the input for Merge / output for Split).
fn infer_mode_merge(current: ColorMode, port_type: PortType, is_input_side: bool) -> ColorMode {
    // If we're already in a definite mode, keep it. Auto-promote only from Neutral
    // (or when promoting Palette ↔ Color via a more specific signal isn't safe here).
    if current != ColorMode::Neutral {
        return current;
    }
    match (is_input_side, port_type) {
        // INPUT side (component side for Merge):
        //   Color → Palette mode (4 Color inputs)
        //   Untyped/Any/Phase/Logic → RGB (default Color mode)
        (true, PortType::Color) => ColorMode::Palette,
        (true, _) => ColorMode::Rgb,
        // OUTPUT side (color/palette side for Merge):
        //   Palette → Palette mode
        //   Color → RGB
        //   anything else → keep Neutral
        (false, PortType::Palette) => ColorMode::Palette,
        (false, PortType::Color) => ColorMode::Rgb,
        (false, _) => ColorMode::Neutral,
    }
}

/// Same idea for Split: inputs are the color/palette side.
fn infer_mode_split(current: ColorMode, port_type: PortType, is_input_side: bool) -> ColorMode {
    if current != ColorMode::Neutral {
        return current;
    }
    match (is_input_side, port_type) {
        // INPUT side (color/palette side for Split):
        (true, PortType::Palette) => ColorMode::Palette,
        (true, PortType::Color) => ColorMode::Rgb,
        (true, _) => ColorMode::Neutral,
        // OUTPUT side (component side for Split):
        (false, PortType::Color) => ColorMode::Palette,
        (false, _) => ColorMode::Rgb,
    }
}

// ---------------------------------------------------------------------------
// Color Merge widget
// ---------------------------------------------------------------------------

pub struct ColorMergeWidget {
    id: NodeId,
    shared: SharedState,
    mode: ColorMode,
}

impl ColorMergeWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, mode: ColorMode::Neutral }
    }
}

impl NodeWidget for ColorMergeWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color Merge" }
    fn title(&self) -> &str { "Color Merge" }
    fn description(&self) -> &'static str { "Combines components into a color. Mode (RGB/HSV/RGBW/CMY/Palette) is auto-detected from the first connection." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        make_port_defs(self.mode, true).iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        match self.mode {
            ColorMode::Neutral => vec![UiPortDef::from_def(&PortDef::new("?", PortType::Any))],
            ColorMode::Palette => vec![UiPortDef::from_def(&PortDef::new("palette", PortType::Palette))],
            _ => vec![UiPortDef::from_def(&PortDef::new("color", PortType::Color))],
        }
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { if self.mode.is_palette() { 40.0 } else { 25.0 } }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn on_ui_connect(&mut self, _input_port: usize, source_type: PortType) {
        let new_mode = infer_mode_merge(self.mode, source_type, true);
        if new_mode != self.mode {
            self.mode = new_mode;
            push_mode(&self.shared, new_mode);
        }
    }
    fn on_ui_output_connect(&mut self, _output_port: usize, dest_type: PortType) {
        let new_mode = infer_mode_merge(self.mode, dest_type, false);
        if new_mode != self.mode {
            self.mode = new_mode;
            push_mode(&self.shared, new_mode);
        }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref().and_then(|d| d.downcast_ref::<ColorOpsDisplay>());
        let (mode, rgb) = if let Some(d) = display {
            (d.mode, d.rgb)
        } else {
            (self.mode, [0.0; 3])
        };
        let palette_channels = if mode.is_palette() { shared.outputs.clone() } else { vec![] };
        drop(shared);

        if mode != self.mode { self.mode = mode; }

        match mode {
            ColorMode::Neutral => {
                ui.colored_label(Color32::from_gray(120), "Connect to set type");
            }
            ColorMode::Palette => show_palette_swatches(ui, &palette_channels),
            _ => show_color_swatch(ui, rgb),
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ---------------------------------------------------------------------------
// Color Split widget
// ---------------------------------------------------------------------------

pub struct ColorSplitWidget {
    id: NodeId,
    shared: SharedState,
    mode: ColorMode,
}

impl ColorSplitWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, mode: ColorMode::Neutral }
    }
}

impl NodeWidget for ColorSplitWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color Split" }
    fn title(&self) -> &str { "Color Split" }
    fn description(&self) -> &'static str { "Splits a color into components. Mode (RGB/HSV/RGBW/CMY/Palette) is auto-detected from the first connection." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        match self.mode {
            ColorMode::Neutral => vec![UiPortDef::from_def(&PortDef::new("?", PortType::Any))],
            ColorMode::Palette => vec![UiPortDef::from_def(&PortDef::new("palette", PortType::Palette))],
            _ => vec![UiPortDef::from_def(&PortDef::new("color", PortType::Color))],
        }
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        make_port_defs(self.mode, true).iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { if self.mode.is_palette() { 40.0 } else { 25.0 } }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn on_ui_connect(&mut self, _input_port: usize, source_type: PortType) {
        let new_mode = infer_mode_split(self.mode, source_type, true);
        if new_mode != self.mode {
            self.mode = new_mode;
            push_mode(&self.shared, new_mode);
        }
    }
    fn on_ui_output_connect(&mut self, _output_port: usize, dest_type: PortType) {
        let new_mode = infer_mode_split(self.mode, dest_type, false);
        if new_mode != self.mode {
            self.mode = new_mode;
            push_mode(&self.shared, new_mode);
        }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref().and_then(|d| d.downcast_ref::<ColorOpsDisplay>());
        let (mode, rgb) = if let Some(d) = display {
            (d.mode, d.rgb)
        } else {
            (self.mode, [0.0; 3])
        };
        let palette_channels = if mode.is_palette() { shared.inputs.clone() } else { vec![] };
        drop(shared);

        if mode != self.mode { self.mode = mode; }

        match mode {
            ColorMode::Neutral => {
                ui.colored_label(Color32::from_gray(120), "Connect to set type");
            }
            ColorMode::Palette => show_palette_swatches(ui, &palette_channels),
            _ => show_color_swatch(ui, rgb),
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
