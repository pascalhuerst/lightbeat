use std::any::Any;
use egui::Ui;

use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct SchmittTriggerWidget {
    id: NodeId,
    shared: SharedState,
}

impl SchmittTriggerWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for SchmittTriggerWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Schmitt Trigger" }
    fn description(&self) -> &'static str {
        "Comparator with hysteresis. Output flips HIGH when input rises above \
         `threshold + hysteresis` and LOW when it falls below \
         `threshold - hysteresis`. Set hysteresis to 0 for a plain comparator."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("in", PortType::Untyped))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", PortType::Logic))]
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 15.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(if out >= 0.5 { "HIGH" } else { "low" });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
