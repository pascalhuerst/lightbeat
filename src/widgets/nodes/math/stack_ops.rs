use std::any::Any;
use egui::{self, Color32, Sense, Ui, Vec2};

use crate::engine::types::*;
use crate::objects::color_palette::STACK_SIZE;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

// ---------------------------------------------------------------------------
// Stack Split widget
// ---------------------------------------------------------------------------

pub struct StackSplitWidget {
    id: NodeId,
    shared: SharedState,
}

impl StackSplitWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for StackSplitWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Stack Split" }
    fn title(&self) -> &str { "Stack Split" }
    fn description(&self) -> &'static str { "Splits a ColorStack palette into its four individual color outputs." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("palette", PortType::ColorStack))]
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
        show_stack_swatches(ui, &vals);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ---------------------------------------------------------------------------
// Stack Merge widget
// ---------------------------------------------------------------------------

pub struct StackMergeWidget {
    id: NodeId,
    shared: SharedState,
}

impl StackMergeWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for StackMergeWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Stack Merge" }
    fn title(&self) -> &str { "Stack Merge" }
    fn description(&self) -> &'static str { "Combines four colors into a single ColorStack palette." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("primary", PortType::Color)),
            UiPortDef::from_def(&PortDef::new("secondary", PortType::Color)),
            UiPortDef::from_def(&PortDef::new("third", PortType::Color)),
            UiPortDef::from_def(&PortDef::new("fourth", PortType::Color)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("palette", PortType::ColorStack))]
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 15.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let vals = shared.outputs.clone();
        drop(shared);
        show_stack_swatches(ui, &vals);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

fn show_stack_swatches(ui: &mut Ui, channels: &[f32]) {
    for i in 0..STACK_SIZE {
        let base = i * 3;
        let r = channels.get(base).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let g = channels.get(base + 1).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let b = channels.get(base + 2).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let color = Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
        let (resp, painter) = ui.allocate_painter(
            Vec2::new(ui.available_width(), ui.available_height() / (STACK_SIZE - i) as f32),
            Sense::hover(),
        );
        painter.rect_filled(resp.rect, 1.0, color);
    }
}
