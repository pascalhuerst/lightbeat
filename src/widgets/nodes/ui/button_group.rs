use std::any::Any;
use std::collections::BTreeMap;

use egui::{self, Color32, Ui, Vec2};

use crate::engine::nodes::ui::button::ButtonMode;
use crate::engine::nodes::ui::button_group::ButtonGroupDisplay;
use crate::engine::types::*;
use crate::widgets::fader::highlight_alpha;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const HIGHLIGHT_DURATION: f64 = 0.5;

pub struct ButtonGroupWidget {
    id: NodeId,
    shared: SharedState,
    rows: usize,
    cols: usize,
    mode: ButtonMode,
    /// Toggle state mirror from engine.
    states: Vec<bool>,
    /// Monotonic click counters, one per cell; keyed "r,c".
    click_ids: BTreeMap<(usize, usize), u64>,
    /// Last-clicked timestamps per cell for output port highlight.
    last_click_time: Vec<Option<f64>>,
}

impl ButtonGroupWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        let rows = 2;
        let cols = 2;
        Self {
            id,
            shared,
            rows,
            cols,
            mode: ButtonMode::Trigger,
            states: vec![false; rows * cols],
            click_ids: BTreeMap::new(),
            last_click_time: vec![None; rows * cols],
        }
    }

    fn cell_count(&self) -> usize { self.rows * self.cols }

    fn push_config(&self) {
        let clicks: serde_json::Map<String, serde_json::Value> = self.click_ids.iter()
            .map(|((r, c), v)| (format!("{},{}", r, c), serde_json::json!(v)))
            .collect();
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({
            "rows": self.rows,
            "cols": self.cols,
            "mode": self.mode.as_str(),
            "clicks": clicks,
        }));
    }

    fn resize(&mut self) {
        let n = self.cell_count();
        self.states.resize(n, false);
        self.last_click_time.resize(n, None);
    }
}

impl NodeWidget for ButtonGroupWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Button Group" }
    fn title(&self) -> &str { "Button Group" }
    fn description(&self) -> &'static str {
        "Grid of buttons with one Logic output per cell. Trigger or Toggle mode applies to all."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        (0..self.rows).flat_map(|r| (0..self.cols).map(move |c| {
            UiPortDef::from_def(&PortDef::new(format!("{},{}", r, c), PortType::Logic))
        })).collect()
    }

    fn min_width(&self) -> f32 { (self.cols as f32 * 40.0).max(120.0) }
    fn min_content_height(&self) -> f32 { (self.rows as f32 * 32.0).max(60.0) }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn output_highlight(&self, port_idx: usize, now: f64) -> f32 {
        let last = self.last_click_time.get(port_idx).copied().flatten();
        highlight_alpha(last, now, HIGHLIGHT_DURATION)
    }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        // Sync dims and mode from engine.
        let snapshot: Option<(usize, usize, ButtonMode, Vec<bool>)> = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<ButtonGroupDisplay>())
                .map(|d| (d.rows, d.cols, d.mode, d.states.clone()))
        };
        if let Some((rows, cols, mode, states)) = snapshot {
            let resized = rows != self.rows || cols != self.cols;
            self.rows = rows;
            self.cols = cols;
            self.mode = mode;
            self.states = states;
            if resized { self.resize(); }
        }

        let avail = ui.available_size();
        if avail.x <= 0.0 || avail.y <= 0.0 { return; }
        let cols = self.cols.max(1);
        let rows = self.rows.max(1);
        let cw = avail.x / cols as f32;
        let rh = avail.y / rows as f32;
        let (rect, _resp) = ui.allocate_exact_size(avail, egui::Sense::hover());
        let now = ui.ctx().input(|i| i.time);

        for r in 0..rows {
            for c in 0..cols {
                let idx = r * cols + c;
                let cell_min = egui::pos2(rect.min.x + c as f32 * cw, rect.min.y + r as f32 * rh);
                let cell_rect = egui::Rect::from_min_size(cell_min, Vec2::new(cw, rh));
                let inner = cell_rect.shrink(1.5);
                let id = ui.id().with((self.id, r, c));
                let resp = ui.interact(inner, id, egui::Sense::click());

                let pressed = matches!(self.mode, ButtonMode::Toggle) && self.states.get(idx).copied().unwrap_or(false);
                let fill = if pressed {
                    Color32::from_rgb(80, 200, 240)
                } else if resp.hovered() {
                    Color32::from_gray(80)
                } else {
                    Color32::from_gray(60)
                };
                let text_color = if pressed { Color32::BLACK } else { Color32::WHITE };

                let painter = ui.painter_at(inner);
                painter.rect_filled(inner, 3.0, fill);
                let border = if resp.hovered() {
                    Color32::from_gray(140)
                } else {
                    Color32::from_gray(90)
                };
                painter.rect_stroke(inner, 3.0, egui::Stroke::new(1.0, border), egui::StrokeKind::Inside);
                painter.text(
                    inner.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{},{}", r, c),
                    egui::FontId::proportional(11.0 * zoom),
                    text_color,
                );

                if resp.clicked() {
                    let entry = self.click_ids.entry((r, c)).or_insert(0);
                    *entry = entry.wrapping_add(1);
                    if let Some(slot) = self.last_click_time.get_mut(idx) {
                        *slot = Some(now);
                    }
                    self.push_config();
                }
            }
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
        ui.horizontal(|ui| {
            ui.label("Mode:");
            if ui.radio_value(&mut self.mode, ButtonMode::Trigger, "Trigger").clicked() {
                changed = true;
            }
            if ui.radio_value(&mut self.mode, ButtonMode::Toggle, "Toggle").clicked() {
                changed = true;
            }
        });
        if changed { self.push_config(); }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
