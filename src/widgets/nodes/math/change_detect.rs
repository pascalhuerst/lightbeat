use std::any::Any;
use egui::Ui;

use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct ChangeDetectWidget {
    id: NodeId,
    shared: SharedState,
}

impl ChangeDetectWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for ChangeDetectWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Change Detect" }
    fn title(&self) -> &str { "Change Detect" }
    fn description(&self) -> &'static str { "Fires a logic pulse whenever the input value changes." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("trigger", PortType::Logic)),
            UiPortDef::from_def(&PortDef::new("value", PortType::Any)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("changed", PortType::Logic)),
            UiPortDef::from_def(&PortDef::new("trigger", PortType::Logic)),
        ]
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 15.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(if out >= 0.5 { "CHANGED" } else { "same" });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
