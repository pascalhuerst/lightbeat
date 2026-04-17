use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct SampleHoldWidget {
    id: NodeId,
    shared: SharedState,
}

impl SampleHoldWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for SampleHoldWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Sample & Hold" }
    fn title(&self) -> &str { "Sample & Hold" }
    fn description(&self) -> &'static str {
        "Captures the value input on each rising edge of the trigger input."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("value", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("trigger", PortType::Logic)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", PortType::Untyped))]
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 20.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let held = {
            let shared = self.shared.lock().unwrap();
            shared.outputs.first().copied().unwrap_or(0.0)
        };
        ui.colored_label(Color32::from_gray(200), format!("held {:.3}", held));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
