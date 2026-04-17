use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct LatchWidget {
    id: NodeId,
    shared: SharedState,
}

impl LatchWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for LatchWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Latch" }
    fn title(&self) -> &str { "Latch (z-1)" }
    fn description(&self) -> &'static str {
        "Adds one tick of delay. Useful for explicit feedback paths."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("in", PortType::Untyped))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", PortType::Untyped))]
    }

    fn min_width(&self) -> f32 { 90.0 }
    fn min_content_height(&self) -> f32 { 18.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let v = {
            let shared = self.shared.lock().unwrap();
            shared.outputs.first().copied().unwrap_or(0.0)
        };
        ui.colored_label(Color32::from_gray(180), format!("{:.3}", v));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
