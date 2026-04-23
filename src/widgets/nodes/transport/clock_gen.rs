use std::any::Any;
use egui::Ui;

use crate::engine::nodes::transport::clock_gen::ClockGenDisplay;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct ClockGenWidget {
    id: NodeId,
    shared: SharedState,
}

impl ClockGenWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for ClockGenWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Clock Gen" }
    fn description(&self) -> &'static str { "Generates N evenly spaced triggers per phase cycle." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("phase", PortType::Phase))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("trigger", PortType::Logic))]
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 15.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let count = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<ClockGenDisplay>())
            .map(|d| d.count)
            .unwrap_or(4);
        drop(shared);
        ui.label(format!("×{}", count));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
