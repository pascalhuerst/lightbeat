use std::any::Any;
use egui::{self, Color32, Sense, Ui, Vec2};

use crate::engine::types::*;
use crate::objects::color_palette::PALETTE_SIZE;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

// ---------------------------------------------------------------------------
// Palette Split widget
// ---------------------------------------------------------------------------

pub struct PaletteSplitWidget {
    id: NodeId,
    shared: SharedState,
}

impl PaletteSplitWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for PaletteSplitWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Palette Split" }
    fn title(&self) -> &str { "Palette Split" }
    fn description(&self) -> &'static str { "Splits a palette (4-color set) into its four individual color outputs." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("palette", PortType::Palette))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("primary", PortType::Color)),
            UiPortDef::from_def(&PortDef::new("secondary", PortType::Color)),
            UiPortDef::from_def(&PortDef::new("third", PortType::Color)),
            UiPortDef::from_def(&PortDef::new("fourth", PortType::Color)),
        ]
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 15.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let vals = shared.inputs.clone();
        drop(shared);
        show_palette_swatches(ui, &vals);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ---------------------------------------------------------------------------
// Palette Merge widget
// ---------------------------------------------------------------------------

pub struct PaletteMergeWidget {
    id: NodeId,
    shared: SharedState,
}

impl PaletteMergeWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for PaletteMergeWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Palette Merge" }
    fn title(&self) -> &str { "Palette Merge" }
    fn description(&self) -> &'static str { "Combines four colors into a single palette (4-color set)." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("primary", PortType::Color)),
            UiPortDef::from_def(&PortDef::new("secondary", PortType::Color)),
            UiPortDef::from_def(&PortDef::new("third", PortType::Color)),
            UiPortDef::from_def(&PortDef::new("fourth", PortType::Color)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("palette", PortType::Palette))]
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 15.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let vals = shared.outputs.clone();
        drop(shared);
        show_palette_swatches(ui, &vals);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

fn show_palette_swatches(ui: &mut Ui, channels: &[f32]) {
    for i in 0..PALETTE_SIZE {
        let base = i * 3;
        let r = channels.get(base).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let g = channels.get(base + 1).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let b = channels.get(base + 2).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let color = Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
        let (resp, painter) = ui.allocate_painter(
            Vec2::new(ui.available_width(), ui.available_height() / (PALETTE_SIZE - i) as f32),
            Sense::hover(),
        );
        painter.rect_filled(resp.rect, 1.0, color);
    }
}
