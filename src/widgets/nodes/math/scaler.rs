use std::any::Any;
use egui::Ui;

use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct ScalerWidget {
    id: NodeId,
    shared: SharedState,
}

impl ScalerWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for ScalerWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Range to Unit" }
    fn description(&self) -> &'static str { "Maps the range from in min to in max onto 0..1, clamped. The range comes from the settings or from wires." }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::scaler::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::scaler::ports().1) }

    fn overridden_param_indices(&self) -> Vec<usize> {
        // "In Min" = param 0, wired from port 1. "In Max" = param 1, wired from port 2.
        let s = self.shared.lock().unwrap();
        let mut hidden = Vec::new();
        if s.inputs_connected.get(1).copied().unwrap_or(false) { hidden.push(0); }
        if s.inputs_connected.get(2).copied().unwrap_or(false) { hidden.push(1); }
        hidden
    }

    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 15.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(format!("{:.2}", out));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
