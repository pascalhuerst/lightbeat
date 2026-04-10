use std::any::Any;

use egui::{self, Color32, Sense, Ui, Vec2};

use crate::engine::nodes::transport::envelope::EnvelopeDisplay;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct EnvelopeWidget {
    id: NodeId,
    shared: SharedState,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl EnvelopeWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            inputs: vec![
                PortDef::new("gate", PortType::Logic),
                PortDef::new("signal", PortType::Untyped),
            ],
            outputs: vec![
                PortDef::new("envelope", PortType::Untyped),
                PortDef::new("signal", PortType::Untyped),
            ],
        }
    }
}

impl NodeWidget for EnvelopeWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "ADSR" }
    fn title(&self) -> &str { "ADSR" }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.inputs.iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.outputs.iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 140.0 }
    fn min_content_height(&self) -> f32 { 50.0 }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<EnvelopeDisplay>());

        let (stage, env_val) = if let Some(d) = display {
            (d.stage, d.envelope_value)
        } else {
            (0, 0.0)
        };
        drop(shared);

        let stage_name = match stage {
            0 => "Idle",
            1 => "Attack",
            2 => "Decay",
            3 => "Sustain",
            4 => "Release",
            _ => "?",
        };

        // Simple envelope level bar.
        let available_width = ui.available_width();
        let height = ui.available_height().max(20.0);
        let (response, painter) =
            ui.allocate_painter(Vec2::new(available_width, height), Sense::hover());
        let rect = response.rect;

        painter.rect_filled(rect, 2.0, Color32::from_gray(30));

        let bar_width = env_val * rect.width();
        if bar_width > 0.0 {
            let bar_rect = egui::Rect::from_min_size(
                rect.min,
                Vec2::new(bar_width, rect.height()),
            );
            painter.rect_filled(bar_rect, 2.0, Color32::from_rgb(80, 200, 160));
        }

        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{} {:.2}", stage_name, env_val),
            egui::FontId::monospace(10.0 * zoom),
            Color32::WHITE,
        );
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
