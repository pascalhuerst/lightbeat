use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::ui::button::{ButtonDisplay, ButtonMode};
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct ButtonWidget {
    id: NodeId,
    shared: SharedState,
    label: String,
    mode: ButtonMode,
    /// Local mirror of toggle state (synced from engine display).
    state: bool,
    /// Monotonic counter pushed to engine on click.
    click_id: u64,
}

impl ButtonWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            label: "Button".to_string(),
            mode: ButtonMode::Trigger,
            state: false,
            click_id: 0,
        }
    }

    fn push_config(&self) {
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({
            "label": self.label,
            "mode": self.mode.as_str(),
            "click_id": self.click_id,
        }));
    }
}

impl NodeWidget for ButtonWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Button" }
    fn title(&self) -> &str { "Button" }
    fn description(&self) -> &'static str { "Clickable button outputting a trigger pulse or persistent toggle state." }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", PortType::Logic))]
    }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 30.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        // Sync state and label from engine display.
        let shared = self.shared.lock().unwrap();
        if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<ButtonDisplay>()) {
            self.label = d.label.clone();
            self.mode = d.mode;
            self.state = d.state;
        }
        drop(shared);

        let pressed = matches!(self.mode, ButtonMode::Toggle) && self.state;
        let fill = if pressed {
            Color32::from_rgb(80, 200, 240)
        } else {
            Color32::from_gray(60)
        };
        let text_color = if pressed { Color32::BLACK } else { Color32::WHITE };

        // Allocate the exact area available — paint the button manually so it
        // never overflows the node (regardless of label length or zoom level).
        let avail = ui.available_size();
        if avail.x <= 0.0 || avail.y <= 0.0 { return; }
        let (rect, resp) = ui.allocate_exact_size(avail, egui::Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, fill);
        let border = if resp.hovered() {
            Color32::from_gray(140)
        } else {
            Color32::from_gray(90)
        };
        painter.rect_stroke(rect, 3.0, egui::Stroke::new(1.0, border), egui::StrokeKind::Inside);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &self.label,
            egui::FontId::proportional(13.0 * zoom),
            text_color,
        );
        if resp.clicked() {
            self.click_id = self.click_id.wrapping_add(1);
            self.push_config();
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Label:");
            if ui.text_edit_singleline(&mut self.label).changed() {
                self.push_config();
            }
        });

        ui.horizontal(|ui| {
            ui.label("Mode:");
            let mut changed = false;
            if ui.radio_value(&mut self.mode, ButtonMode::Trigger, "Trigger").clicked() {
                changed = true;
            }
            if ui.radio_value(&mut self.mode, ButtonMode::Toggle, "Toggle").clicked() {
                changed = true;
            }
            if changed { self.push_config(); }
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
