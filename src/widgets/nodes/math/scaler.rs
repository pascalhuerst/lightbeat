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
    fn type_name(&self) -> &'static str { "Scaler" }
    fn description(&self) -> &'static str { "Linearly remaps an input range to an output range." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("in", PortType::Any)),
            UiPortDef::from_def(&PortDef::new("min", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("max", PortType::Untyped)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", PortType::Untyped))]
    }

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
