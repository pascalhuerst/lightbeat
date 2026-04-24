use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::math::toggle_bank::{
    MAX_CHANNELS, MIN_CHANNELS, ToggleBankDisplay,
};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct ToggleBankWidget {
    id: NodeId,
    shared: SharedState,
    n: usize,
    states: Vec<bool>,
}

impl ToggleBankWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, n: 4, states: vec![false; 4] }
    }

    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("n").and_then(|v| v.as_u64()) {
            self.n = (n as usize).clamp(MIN_CHANNELS, MAX_CHANNELS);
            self.states.resize(self.n, false);
        }
    }
}

impl NodeWidget for ToggleBankWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Toggle Bank" }
    fn description(&self) -> &'static str {
        "Bank of independent toggle flip-flops. Each input is a rising-edge \
         trigger; each trigger flips its matching output between 0 and 1. \
         A small LED per channel shows the current state."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        (0..self.n)
            .map(|i| UiPortDef::from_def(&PortDef::new(format!("T{}", i + 1), PortType::Logic)))
            .collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        (0..self.n)
            .map(|i| UiPortDef::from_def(&PortDef::new(format!("O{}", i + 1), PortType::Untyped)))
            .collect()
    }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { self.n.max(1) as f32 * 16.0 + 4.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        // Pull the latest state from the engine.
        {
            let shared = self.shared.lock().unwrap();
            if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<ToggleBankDisplay>()) {
                self.n = d.n;
                self.states = d.states.clone();
            }
        }

        let avail = ui.available_size();
        if avail.x <= 0.0 || avail.y <= 0.0 { return; }
        let (rect, _) = ui.allocate_exact_size(avail, egui::Sense::hover());
        let painter = ui.painter_at(rect);

        let n = self.n.max(1);
        let row_h = rect.height() / n as f32;
        let dot_r = (row_h * 0.35).min(5.0 * zoom).max(2.0 * zoom);

        for i in 0..n {
            let y = rect.min.y + row_h * (i as f32 + 0.5);
            let on = self.states.get(i).copied().unwrap_or(false);
            let fill = if on { theme::SEM_PRIMARY } else { Color32::from_gray(60) };
            let stroke_col = if on { Color32::from_gray(220) } else { Color32::from_gray(120) };

            let center = egui::pos2(rect.center().x - row_h * 0.9, y);
            painter.circle_filled(center, dot_r, fill);
            painter.circle_stroke(center, dot_r, egui::Stroke::new(1.0, stroke_col));

            let label = format!("{}", i + 1);
            painter.text(
                egui::pos2(rect.center().x - row_h * 0.9 + dot_r + 4.0 * zoom, y),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(10.0 * zoom.max(0.5)),
                if on { theme::TEXT } else { theme::TEXT_DIM },
            );
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
