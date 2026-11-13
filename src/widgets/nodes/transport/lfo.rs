use egui::{self, Sense, Stroke, Ui, Vec2};

use crate::engine::nodes::transport::lfo::{ports_for, LfoDisplay};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeWidget, node_identity, with_display};
use crate::widgets::nodes::types::UiPortDef;

/// The index of the "Phase input" parameter.
const PHASE_INPUT_PARAM: usize = 3;

pub struct LfoWidget {
    id: NodeId,
    shared: SharedState,
}

impl LfoWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }

    /// Whether the phase comes from a wire. A change still waiting for the
    /// engine counts, so wires saved in this mode are not dropped as the
    /// wrong type while a loaded project settles.
    fn phase_input(&self) -> bool {
        let s = self.shared.lock();
        if let Some((_, v)) = s.pending_params.iter().rev().find(|(i, _)| *i == PHASE_INPUT_PARAM) {
            return matches!(v, ParamValue::Bool(true));
        }
        matches!(s.current_params.get(PHASE_INPUT_PARAM), Some(ParamDef::Bool { value: true, .. }))
    }
}

impl NodeWidget for LfoWidget {
    node_identity!("LFO");
    fn description(&self) -> &'static str {
        "Low-frequency oscillator: sine, triangle, saw, square, or random; rate in Hz. \
         With Phase input on (inspector) it follows a phase wire instead of running on its own."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&ports_for(self.phase_input()).0)
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&ports_for(self.phase_input()).1)
    }

    fn min_width(&self) -> f32 { 130.0 }
    fn min_content_height(&self) -> f32 { 40.0 }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        let (rate_hz, phase, value) = with_display::<LfoDisplay, _>(&self.shared, |d| (d.rate_hz, d.phase, d.value)).unwrap_or((1.0, 0.0, 0.0));

        // Rate label.
        ui.colored_label(
            theme::TEXT_DIM,
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
        painter.rect_filled(rect, 2.0, theme::BG_GRID);
        // Center line
        let cy = rect.center().y;
        painter.line_segment(
            [egui::pos2(rect.min.x, cy), egui::pos2(rect.max.x, cy)],
            Stroke::new(0.5_f32, theme::PLOT_BORDER),
        );
        // Phase position (vertical bar)
        let px = rect.min.x + rect.width() * phase;
        painter.line_segment(
            [egui::pos2(px, rect.min.y), egui::pos2(px, rect.max.y)],
            Stroke::new(1.0_f32, theme::SEM_PRIMARY),
        );
        // Value indicator (dot at intersection)
        let value_norm = value.clamp(-1.5, 1.5);
        let py = cy - value_norm * (rect.height() * 0.45);
        painter.circle_filled(egui::pos2(px, py), 3.0, theme::TYPE_LOGIC);
    }
}
