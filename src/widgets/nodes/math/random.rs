use std::any::Any;

use egui::{self, Ui};

use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct RandomWidget {
    id: NodeId,
    shared: SharedState,
}

impl RandomWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for RandomWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Random" }
    fn description(&self) -> &'static str {
        "Picks a new random value between min and max on each rising edge of the trigger and holds it. With steps it picks one of that many evenly spaced values; glide slides to the new value."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("trigger", PortType::Logic))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", PortType::Untyped))]
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 20.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let value = self.shared.lock().unwrap().outputs.first().copied().unwrap_or(0.0);
        ui.colored_label(theme::TEXT, format!("{:.3}", value));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
