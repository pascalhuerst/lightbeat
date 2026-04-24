use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::math::trigger_bank::{
    MAX_CHANNELS, MIN_CHANNELS, TriggerBankDisplay,
};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

/// How long the per-channel LED stays lit after a trigger fires, so pulses
/// are actually visible at a normal frame rate.
const LED_HOLD_SECS: f64 = 0.12;

pub struct TriggerBankWidget {
    id: NodeId,
    shared: SharedState,
    n: usize,
    /// Last-pulse timestamp per channel, for the visual flash.
    last_pulse_time: Vec<Option<f64>>,
}

impl TriggerBankWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, n: 4, last_pulse_time: vec![None; 4] }
    }

    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("n").and_then(|v| v.as_u64()) {
            self.n = (n as usize).clamp(MIN_CHANNELS, MAX_CHANNELS);
            self.last_pulse_time.resize(self.n, None);
        }
    }
}

impl NodeWidget for TriggerBankWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Trigger Bank" }
    fn description(&self) -> &'static str {
        "Bank of value-to-trigger converters. Each input watches a 0..1 \
         value; when it rises across 0.5, the matching output fires a \
         one-tick Logic pulse. Use to turn held values (fader cells, \
         palette select, etc.) into graph triggers."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        (0..self.n)
            .map(|i| UiPortDef::from_def(&PortDef::new(format!("In {}", i + 1), PortType::Untyped)))
            .collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        (0..self.n)
            .map(|i| UiPortDef::from_def(&PortDef::new(format!("T{}", i + 1), PortType::Logic)))
            .collect()
    }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { self.n.max(1) as f32 * 16.0 + 4.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        let now = ui.ctx().input(|i| i.time);

        // Sync pulse state from engine; latch the pulse time for the
        // fading LED effect.
        {
            let shared = self.shared.lock().unwrap();
            if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<TriggerBankDisplay>()) {
                if d.n != self.n {
                    self.n = d.n;
                    self.last_pulse_time.resize(self.n, None);
                }
                for (i, &p) in d.pulses.iter().enumerate() {
                    if p {
                        if let Some(slot) = self.last_pulse_time.get_mut(i) {
                            *slot = Some(now);
                        }
                    }
                }
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
            let alpha = self.last_pulse_time
                .get(i)
                .and_then(|t| *t)
                .map(|t| {
                    let age = (now - t).max(0.0);
                    if age >= LED_HOLD_SECS { 0.0 } else { 1.0 - (age / LED_HOLD_SECS) as f32 }
                })
                .unwrap_or(0.0);

            let fill = if alpha > 0.0 {
                let base = theme::SEM_PRIMARY;
                Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), (alpha * 255.0) as u8)
            } else {
                Color32::from_gray(60)
            };
            let center = egui::pos2(rect.center().x - row_h * 0.9, y);
            painter.circle_filled(center, dot_r, fill);
            painter.circle_stroke(center, dot_r, egui::Stroke::new(1.0, Color32::from_gray(120)));

            let label = format!("{}", i + 1);
            painter.text(
                egui::pos2(rect.center().x - row_h * 0.9 + dot_r + 4.0 * zoom, y),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(10.0 * zoom.max(0.5)),
                if alpha > 0.0 { theme::TEXT } else { theme::TEXT_DIM },
            );
        }

        // Trigger a repaint while any LED is still fading so the visual
        // flash animates without requiring mouse input.
        if self.last_pulse_time.iter().filter_map(|t| *t).any(|t| (now - t) < LED_HOLD_SECS) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
