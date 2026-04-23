use std::any::Any;

use egui::{self, Color32, Sense, Stroke, Ui, Vec2};

use crate::engine::nodes::transport::lfo::LfoDisplay;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct LfoWidget {
    id: NodeId,
    shared: SharedState,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl LfoWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            inputs: vec![PortDef::new("sync", PortType::Logic)],
            outputs: vec![
                PortDef::new("value", PortType::Untyped),
                PortDef::new("phase", PortType::Phase),
            ],
        }
    }
}

impl NodeWidget for LfoWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "LFO" }
    fn description(&self) -> &'static str {
        "Low-frequency oscillator: sine, triangle, saw, square, or random; rate in Hz."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.inputs.iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.outputs.iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 130.0 }
    fn min_content_height(&self) -> f32 { 40.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<LfoDisplay>());
        let (rate_hz, phase, value) = if let Some(d) = display {
            (d.rate_hz, d.phase, d.value)
        } else {
            (1.0, 0.0, 0.0)
        };
        drop(shared);

        // Rate label.
        ui.colored_label(
            Color32::from_gray(140),
            egui::RichText::new(format!("{:.2} Hz", rate_hz))
                .monospace().size(10.0 * zoom),
        );

        // Mini scope: dot moves left-to-right with phase, height shows value.
        let avail = ui.available_size();
        let h = (avail.y - 4.0).max(8.0);
        let (resp, painter) = ui.allocate_painter(
            Vec2::new(avail.x, h),
            Sense::hover(),
        );
        let rect = resp.rect;
        // Background
        painter.rect_filled(rect, 2.0, Color32::from_gray(30));
        // Center line
        let cy = rect.center().y;
        painter.line_segment(
            [egui::pos2(rect.min.x, cy), egui::pos2(rect.max.x, cy)],
            Stroke::new(0.5, Color32::from_gray(60)),
        );
        // Phase position (vertical bar)
        let px = rect.min.x + rect.width() * phase;
        painter.line_segment(
            [egui::pos2(px, rect.min.y), egui::pos2(px, rect.max.y)],
            Stroke::new(1.0, theme::SEM_PRIMARY),
        );
        // Value indicator (dot at intersection)
        let value_norm = value.clamp(-1.5, 1.5);
        let py = cy - value_norm * (rect.height() * 0.45);
        painter.circle_filled(egui::pos2(px, py), 3.0, theme::TYPE_LOGIC);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
