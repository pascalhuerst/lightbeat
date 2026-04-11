use std::any::Any;
use egui::{self, Color32, Sense, Ui, Vec2};

use crate::engine::nodes::display::value_display::ValueDisplayData;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct ValueDisplayWidget {
    id: NodeId,
    shared: SharedState,
}

impl ValueDisplayWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for ValueDisplayWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Value Display" }
    fn title(&self) -> &str { "Value Display" }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("in", PortType::Any))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 25.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<ValueDisplayData>());

        let (value, mode) = if let Some(d) = display {
            (d.value, d.mode)
        } else {
            (0.0, 0)
        };
        drop(shared);

        match mode {
            1 => {
                // LED mode.
                let brightness = value.clamp(0.0, 1.0);
                let size = (20.0 * zoom).max(10.0);
                let (resp, painter) = ui.allocate_painter(
                    Vec2::new(ui.available_width(), size),
                    Sense::hover(),
                );
                let center = resp.rect.center();
                let radius = size * 0.4;

                // Glow effect.
                if brightness > 0.01 {
                    let glow_radius = radius * (1.0 + brightness * 0.5);
                    let glow_color = Color32::from_rgba_premultiplied(
                        (200.0 * brightness) as u8,
                        (40.0 * brightness) as u8,
                        (30.0 * brightness) as u8,
                        (80.0 * brightness) as u8,
                    );
                    painter.circle_filled(center, glow_radius, glow_color);
                }

                let led_color = Color32::from_rgb(
                    (40.0 + 215.0 * brightness) as u8,
                    (10.0 + 20.0 * brightness) as u8,
                    (10.0 + 10.0 * brightness) as u8,
                );
                painter.circle_filled(center, radius, led_color);
            }
            _ => {
                // Number mode.
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{:.3}", value))
                            .monospace()
                            .size(14.0 * zoom),
                    );
                });
            }
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
