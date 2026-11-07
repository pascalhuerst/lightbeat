use std::any::Any;

use egui::Ui;

use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct ScaleOffsetWidget {
    id: NodeId,
    shared: SharedState,
}

impl ScaleOffsetWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for ScaleOffsetWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Scale Offset" }
    fn description(&self) -> &'static str {
        "Multiplies the input by a scale and adds an offset: out = in * scale + offset. Wire scale or offset to drive them from a signal."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("in", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("scale", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("offset", PortType::Untyped)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", PortType::Untyped))]
    }

    fn min_width(&self) -> f32 { 90.0 }
    fn min_content_height(&self) -> f32 { 15.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn overridden_param_indices(&self) -> Vec<usize> {
        // Scale = param 0, wired from port 1. Offset = param 1, wired from port 2.
        let s = self.shared.lock().unwrap();
        let mut hidden = Vec::new();
        if s.inputs_connected.get(1).copied().unwrap_or(false) { hidden.push(0); }
        if s.inputs_connected.get(2).copied().unwrap_or(false) { hidden.push(1); }
        hidden
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let s = self.shared.lock().unwrap();
        let out = s.outputs.first().copied().unwrap_or(0.0);
        drop(s);
        ui.label(format!("{:+.2}", out));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
