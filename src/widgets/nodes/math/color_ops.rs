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

fn show_stack_swatches(ui: &mut Ui, channels: &[f32]) {
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

fn make_port_defs(mode: ColorMode, is_component_side: bool) -> Vec<PortDef> {
    if mode.is_stack() {
        mode.channel_names().iter().map(|n| PortDef::new(*n, PortType::Color)).collect()
    } else if is_component_side {
        mode.channel_names().iter().map(|n| PortDef::new(*n, PortType::Untyped)).collect()
    } else {
        vec![PortDef::new("color", PortType::Color)]
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
        Self { id, shared, mode: ColorMode::Rgb }
    }
}

impl NodeWidget for ColorMergeWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color Merge" }
    fn title(&self) -> &str { "Color Merge" }
    fn description(&self) -> &'static str { "Combines components into a color in RGB, HSV, RGBW, CMY, or Stack mode." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        make_port_defs(self.mode, true).iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        if self.mode.is_stack() {
            vec![UiPortDef::from_def(&PortDef::new("palette", PortType::ColorStack))]
        } else {
            vec![UiPortDef::from_def(&PortDef::new("color", PortType::Color))]
        }
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { if self.mode.is_stack() { 40.0 } else { 25.0 } }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref().and_then(|d| d.downcast_ref::<ColorOpsDisplay>());
        let (mode, rgb) = if let Some(d) = display {
            (d.mode, d.rgb)
        } else {
            (self.mode, [0.0; 3])
        };
        let stack_channels = if mode.is_stack() { shared.outputs.clone() } else { vec![] };
        drop(shared);

        if mode != self.mode { self.mode = mode; }

        if mode.is_stack() {
            show_stack_swatches(ui, &stack_channels);
        } else {
            show_color_swatch(ui, rgb);
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
        Self { id, shared, mode: ColorMode::Rgb }
    }
}

impl NodeWidget for ColorSplitWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color Split" }
    fn title(&self) -> &str { "Color Split" }
    fn description(&self) -> &'static str { "Splits a color into components in RGB, HSV, RGBW, CMY, or Stack mode." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        if self.mode.is_stack() {
            vec![UiPortDef::from_def(&PortDef::new("palette", PortType::ColorStack))]
        } else {
            vec![UiPortDef::from_def(&PortDef::new("color", PortType::Color))]
        }
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        make_port_defs(self.mode, true).iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { if self.mode.is_stack() { 40.0 } else { 25.0 } }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref().and_then(|d| d.downcast_ref::<ColorOpsDisplay>());
        let (mode, rgb) = if let Some(d) = display {
            (d.mode, d.rgb)
        } else {
            (self.mode, [0.0; 3])
        };
        let stack_channels = if mode.is_stack() { shared.inputs.clone() } else { vec![] };
        drop(shared);

        if mode != self.mode { self.mode = mode; }

        if mode.is_stack() {
            show_stack_swatches(ui, &stack_channels);
        } else {
            show_color_swatch(ui, rgb);
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
