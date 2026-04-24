use std::any::Any;
use egui::Ui;

use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
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
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Phase Accumulator" }
    fn description(&self) -> &'static str {
        "Unwraps a 0..1 wrapping phase input (X1 encoder, LFO phase, ...) \
         and accumulates the per-tick delta, clamped to [Min, Max]. Scale \
         maps one unit of input delta onto `Scale` units of output — \
         default 24 matches the X1's 24-clicks-per-revolution so one \
         click moves the output by 1."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("in", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("reset", PortType::Logic)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("value", PortType::Untyped))]
    }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 15.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let v = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(format!("{:.2}", v));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
