use std::any::Any;

use egui::{self, Ui, Vec2};

use crate::engine::nodes::ui::fader_group::FaderGroupDisplay;
use crate::engine::types::*;
use crate::widgets::fader::{self, highlight_alpha, FaderStyle, Orientation};
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const HIGHLIGHT_DURATION: f64 = 0.5;

pub struct FaderGroupWidget {
    id: NodeId,
    shared: SharedState,
    rows: usize,
    cols: usize,
    values: Vec<f32>,
    last_edit_time: Vec<Option<f64>>,
}

impl FaderGroupWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        let rows = 1;
        let cols = 4;
        Self {
            id,
            shared,
            rows,
            cols,
            values: vec![0.0; rows * cols],
            last_edit_time: vec![None; rows * cols],
        }
    }

    fn cell_count(&self) -> usize { self.rows * self.cols }

    fn push_config(&self) {
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({
            "rows": self.rows,
            "cols": self.cols,
            "values": self.values,
        }));
    }

    fn resize(&mut self) {
        let n = self.cell_count();
        self.values.resize(n, 0.0);
        self.last_edit_time.resize(n, None);
    }
}

impl NodeWidget for FaderGroupWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Fader Group" }
    fn title(&self) -> &str { "Fader Group" }
    fn description(&self) -> &'static str {
        "Grid of faders with one 0..1 output per cell. Double-click to reset, shift-drag for fine-grained."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        (0..self.rows).flat_map(|r| (0..self.cols).map(move |c| {
            UiPortDef::from_def(&PortDef::new(format!("{},{}", r, c), PortType::Untyped))
        })).collect()
    }

    fn min_width(&self) -> f32 { (self.cols as f32 * 28.0).max(120.0) }
    fn min_content_height(&self) -> f32 { (self.rows as f32 * 60.0).max(80.0) }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn output_highlight(&self, port_idx: usize, now: f64) -> f32 {
        let last = self.last_edit_time.get(port_idx).copied().flatten();
        highlight_alpha(last, now, HIGHLIGHT_DURATION)
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        // Sync from engine.
        let snapshot: Option<(usize, usize, Vec<f32>)> = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<FaderGroupDisplay>())
                .map(|d| (d.rows, d.cols, d.values.clone()))
        };
        if let Some((rows, cols, values)) = snapshot {
            let resized = rows != self.rows || cols != self.cols;
            self.rows = rows;
            self.cols = cols;
            if resized { self.resize(); }
            if self.values.len() == values.len() {
                self.values = values;
            }
        }

        let avail = ui.available_size();
        if avail.x <= 0.0 || avail.y <= 0.0 { return; }
        let rows = self.rows.max(1);
        let cols = self.cols.max(1);
        let cw = avail.x / cols as f32;
        let rh = avail.y / rows as f32;
        let (rect, _resp) = ui.allocate_exact_size(avail, egui::Sense::hover());
        let now = ui.ctx().input(|i| i.time);
        let style = FaderStyle::default();

        let mut changed_any = false;
        for r in 0..rows {
            for c in 0..cols {
                let idx = r * cols + c;
                let cell_min = egui::pos2(rect.min.x + c as f32 * cw, rect.min.y + r as f32 * rh);
                let cell_rect = egui::Rect::from_min_size(cell_min, Vec2::new(cw, rh)).shrink(1.5);
                let id = ui.id().with((self.id, r, c));
                let resp = ui.interact(cell_rect, id, egui::Sense::click_and_drag());

                let value = self.values.get(idx).copied().unwrap_or(0.0);
                let painter = ui.painter_at(cell_rect);
                fader::draw_fader(&painter, cell_rect, value, Orientation::Vertical, &style, false);

                let mut v = value;
                let before = v;
                let touched = fader::handle_fader_interaction(
                    ui, &resp, cell_rect, Orientation::Vertical, &mut v,
                );
                if touched || (v - before).abs() > f32::EPSILON {
                    if let Some(slot) = self.values.get_mut(idx) { *slot = v; }
                    if let Some(t) = self.last_edit_time.get_mut(idx) { *t = Some(now); }
                    changed_any = true;
                }
            }
        }

        if changed_any {
            self.push_config();
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Rows:");
            let mut r = self.rows as i32;
            if ui.add(egui::DragValue::new(&mut r).range(1..=16)).changed() {
                self.rows = r as usize;
                self.resize();
                changed = true;
            }
            ui.label("Cols:");
            let mut c = self.cols as i32;
            if ui.add(egui::DragValue::new(&mut c).range(1..=16)).changed() {
                self.cols = c as usize;
                self.resize();
                changed = true;
            }
        });
        if changed { self.push_config(); }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
