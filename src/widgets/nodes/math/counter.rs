use std::any::Any;
use egui::Ui;

use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct CounterWidget {
    id: NodeId,
    shared: SharedState,
}

impl CounterWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for CounterWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Counter" }
    fn description(&self) -> &'static str { "Counts trigger pulses and wraps at a configurable max, with a wrap signal output." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("trigger", PortType::Logic)),
            UiPortDef::from_def(&PortDef::new("reset", PortType::Logic)),
            UiPortDef::from_def(&PortDef::new("max", PortType::Untyped)),
        ]
    }

    fn overridden_param_indices(&self) -> Vec<usize> {
        // Hide the "Max" param (index 0) when the `max` input port (logical
        // index 2) is wired — its value is what the engine actually uses.
        let s = self.shared.lock().unwrap();
        if s.inputs_connected.get(2).copied().unwrap_or(false) {
            vec![0]
        } else {
            vec![]
        }
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("count", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("wrap", PortType::Logic)),
        ]
    }

    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 15.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let count = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(format!("{}", count as i64));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
