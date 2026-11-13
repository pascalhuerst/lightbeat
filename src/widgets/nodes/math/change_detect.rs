use egui::Ui;

use crate::engine::types::*;
use crate::widgets::nodes::node::{NodeWidget, node_identity};
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
    node_identity!("Change Detect");
    fn description(&self) -> &'static str { "Fires a logic pulse whenever the input value changes." }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::change_detect::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::change_detect::ports().1) }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 15.0 }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(if out >= 0.5 { "CHANGED" } else { "same" });
    }
}
