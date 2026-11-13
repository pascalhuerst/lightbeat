use egui::Ui;

use crate::engine::types::*;
use crate::widgets::nodes::node::{NodeWidget, node_identity};
use crate::widgets::nodes::types::UiPortDef;

pub struct PhaseAccumulatorWidget {
    id: NodeId,
    shared: SharedState,
}

impl PhaseAccumulatorWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for PhaseAccumulatorWidget {
    node_identity!("Phase Accumulator");
    fn description(&self) -> &'static str {
        "Unwraps a 0..1 wrapping phase input (X1 encoder, LFO phase, ...) \
         and accumulates the per-tick delta, clamped to [Min, Max]. Scale \
         maps one unit of input delta onto `Scale` units of output — \
         default 24 matches the X1's 24-clicks-per-revolution so one \
         click moves the output by 1."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::phase_accumulator::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::phase_accumulator::ports().1) }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 15.0 }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock();
        let v = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(format!("{:.2}", v));
    }
}
