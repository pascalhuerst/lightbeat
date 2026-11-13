use egui::{self, Sense, Ui, Vec2};

use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeWidget, node_identity};
use crate::widgets::nodes::types::UiPortDef;

pub struct TimeToPhaseWidget {
    id: NodeId,
    shared: SharedState,
}

impl TimeToPhaseWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for TimeToPhaseWidget {
    node_identity!("Time to Phase");
    fn description(&self) -> &'static str {
        "Makes a phase from a time: a trigger starts one run from 0 to 1 over the set time and it \
         stays at 1; with Run continuously it repeats without end (a trigger restarts it). Wire \
         the phase into whatever should take that long: a Transition, a snapshot fade, an effect."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::transport::time_to_phase::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::transport::time_to_phase::ports().1) }

    fn min_width(&self) -> f32 { 110.0 }
    fn min_content_height(&self) -> f32 { 18.0 }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let (phase, running) = {
            let s = self.shared.lock();
            (s.outputs.first().copied().unwrap_or(0.0), s.outputs.get(1).copied().unwrap_or(0.0) > 0.5)
        };
        let (resp, painter) = ui.allocate_painter(Vec2::new(ui.available_width(), 10.0), Sense::hover());
        let r = resp.rect;
        painter.rect_filled(r, 2.0, theme::BG_GRID);
        let fill = egui::Rect::from_min_max(r.min, egui::pos2(r.min.x + r.width() * phase.clamp(0.0, 1.0), r.max.y));
        painter.rect_filled(fill, 2.0, if running { theme::SEM_PRIMARY } else { theme::TEXT_DIM });
    }
}
