use std::any::Any;

use egui::{self, Sense, Ui, Vec2};

use crate::engine::nodes::display::color_display::ColorDisplayData;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct ColorDisplayWidget {
    id: NodeId,
    shared: SharedState,
    inputs: Vec<PortDef>,
}

impl ColorDisplayWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            inputs: vec![
                PortDef::new("R", PortType::Untyped),
                PortDef::new("G", PortType::Untyped),
                PortDef::new("B", PortType::Untyped),
            ],
        }
    }
}

impl NodeWidget for ColorDisplayWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color Display" }
    fn title(&self) -> &str { "Color Display" }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.inputs.iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 60.0 }
    fn resizable(&self) -> bool { true }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<ColorDisplayData>());

        let (r, g, b) = if let Some(d) = display {
            (d.r, d.g, d.b)
        } else {
            (0.0, 0.0, 0.0)
        };
        drop(shared);

        let color = egui::Color32::from_rgb(
            (r * 255.0) as u8,
            (g * 255.0) as u8,
            (b * 255.0) as u8,
        );

        let size = Vec2::new(ui.available_width(), ui.available_height().max(40.0));
        let (response, painter) = ui.allocate_painter(size, Sense::hover());
        let rect = response.rect;

        painter.rect_filled(rect, 4.0, color);

        // Show hex value.
        let luma = r * 0.299 + g * 0.587 + b * 0.114;
        let text_color = if luma > 0.5 {
            egui::Color32::BLACK
        } else {
            egui::Color32::WHITE
        };
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("#{:02X}{:02X}{:02X}", (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8),
            egui::FontId::monospace(11.0 * zoom),
            text_color,
        );
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
