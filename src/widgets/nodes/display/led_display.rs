use std::any::Any;
use egui::{self, Color32, Sense, Ui, Vec2};

use crate::engine::nodes::display::led_display::LedDisplayData;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const NAME_GAP: f32 = 2.0;

pub struct LedDisplayWidget {
    id: NodeId,
    shared: SharedState,
    name: String,
}

impl LedDisplayWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, name: String::new() }
    }

    fn push_config(&self) {
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({ "name": self.name }));
    }

    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
            self.name = n.to_string();
        }
    }
}

impl NodeWidget for LedDisplayWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "LED Display" }
    fn title(&self) -> &str { "LED Display" }
    fn description(&self) -> &'static str { "Shows a value (0..1) as a glowing LED. Name appears above the LED and on parent subgraph nodes." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("in", PortType::Any))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 25.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let (name, value) = {
            let shared = self.shared.lock().unwrap();
            let display = shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<LedDisplayData>());
            if let Some(d) = display {
                (d.name.clone(), d.value)
            } else {
                (self.name.clone(), 0.0)
            }
        };
        self.name = name.clone();

        let avail = ui.available_size();
        let (resp, painter) = ui.allocate_painter(avail, Sense::hover());
        let mut rect = resp.rect;

        if !name.is_empty() {
            let label_h = (rect.height() * 0.25).clamp(10.0, 18.0);
            let label_rect = egui::Rect::from_min_size(rect.min, Vec2::new(rect.width(), label_h));
            let label_size = fit_label_size(ui, &name, label_rect.size());
            painter.text(
                label_rect.center(),
                egui::Align2::CENTER_CENTER,
                &name,
                egui::FontId::proportional(label_size),
                Color32::from_gray(180),
            );
            rect = egui::Rect::from_min_max(
                egui::pos2(rect.min.x, label_rect.max.y + NAME_GAP),
                rect.max,
            );
        }

        let brightness = value.clamp(0.0, 1.0);
        let center = rect.center();
        let radius = (rect.width().min(rect.height()) * 0.45).max(2.0);

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

    fn show_inspector(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Name:");
            if ui.text_edit_singleline(&mut self.name).changed() {
                self.push_config();
            }
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

fn fit_label_size(ui: &Ui, text: &str, area: Vec2) -> f32 {
    let pad_x = 6.0;
    let pad_y = 4.0;
    let target_w = (area.x - pad_x).max(4.0);
    let target_h = (area.y - pad_y).max(4.0);
    let mut lo = 6.0_f32;
    let mut hi = target_h.max(8.0);
    for _ in 0..14 {
        let mid = 0.5 * (lo + hi);
        let galley = ui.fonts(|f| {
            f.layout_no_wrap(text.to_string(), egui::FontId::proportional(mid), Color32::WHITE)
        });
        let size = galley.size();
        if size.x <= target_w && size.y <= target_h {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo.max(6.0)
}
