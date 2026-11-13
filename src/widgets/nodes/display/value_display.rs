use egui::{self, Sense, Ui, Vec2};

use crate::engine::nodes::display::value_display::ValueDisplayData;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeLayout, NodeWidget, PORT_RADIUS, node_identity};
use crate::widgets::nodes::types::UiPortDef;

const NAME_GAP: f32 = 2.0;

pub struct ValueDisplayWidget {
    id: NodeId,
    shared: SharedState,
    /// Mirror of the engine's name; edited live via the inspector and pushed
    /// back through `pending_config`.
    name: String,
}

impl ValueDisplayWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, name: String::new() }
    }

    fn push_config(&self) {
        let mut shared = self.shared.lock();
        shared.pending_config = Some(serde_json::json!({ "name": self.name }));
    }
}

impl NodeWidget for ValueDisplayWidget {
    fn restore_from_save(&mut self, data: Option<&serde_json::Value>, _params: &[(usize, crate::engine::types::ParamValue)]) {
        let Some(data) = data else { return };
        if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
            self.name = n.to_string();
        }
    }

    node_identity!("Value Display");
    fn description(&self) -> &'static str { "Shows a numeric value. Name appears above the value and on parent subgraph nodes." }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::display::value_display::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::display::value_display::ports().1) }

    fn layout(&self) -> NodeLayout { NodeLayout::compact().with_side_inset(PORT_RADIUS + 2.0) }
    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 25.0 }
    fn resizable(&self) -> bool { true }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let (name, value) = {
            let shared = self.shared.lock();
            let display = shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<ValueDisplayData>());
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
            let label_size = fit_text_size(ui, &name, label_rect.size());
            painter.text(
                label_rect.center(),
                egui::Align2::CENTER_CENTER,
                &name,
                egui::FontId::proportional(label_size),
                theme::TEXT_MUTED,
            );
            rect = egui::Rect::from_min_max(
                egui::pos2(rect.min.x, label_rect.max.y + NAME_GAP),
                rect.max,
            );
        }

        let text = format!("{:.3}", value);
        let font_size = fit_text_size(ui, &text, rect.size());
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::monospace(font_size),
            ui.visuals().text_color(),
        );
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Name");
            if ui.text_edit_singleline(&mut self.name).changed() {
                self.push_config();
            }
        });
    }
}

/// Binary-search the largest monospace font size that lets `text` fit inside
/// `area`. Returns a screen-pixel size suitable for `painter.text`.
fn fit_text_size(ui: &Ui, text: &str, area: Vec2) -> f32 {
    let pad_x = 6.0;
    let pad_y = 4.0;
    let target_w = (area.x - pad_x).max(4.0);
    let target_h = (area.y - pad_y).max(4.0);
    let mut lo = 6.0_f32;
    let mut hi = target_h.max(8.0);
    for _ in 0..14 {
        let mid = 0.5 * (lo + hi);
        let galley = ui.fonts(|f| {
            f.layout_no_wrap(text.to_string(), egui::FontId::monospace(mid), theme::TEXT_BRIGHT)
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
