use egui::Ui;

use crate::engine::types::*;
use crate::widgets::nodes::node::{NodeWidget, node_identity};
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
    node_identity!("Schmitt Trigger");
    fn description(&self) -> &'static str {
        "Comparator with hysteresis. Output flips HIGH when input rises above \
         `threshold + hysteresis` and LOW when it falls below \
         `threshold - hysteresis`. Set hysteresis to 0 for a plain comparator."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::schmitt::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::schmitt::ports().1) }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 15.0 }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(if out >= 0.5 { "HIGH" } else { "low" });
    }
}
