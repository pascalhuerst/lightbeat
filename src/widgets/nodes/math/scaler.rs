use egui::Ui;

use crate::engine::types::*;
use crate::widgets::nodes::node::{NodeWidget, node_identity};
use crate::widgets::nodes::types::UiPortDef;

pub struct ScalerWidget {
    id: NodeId,
    shared: SharedState,
}

impl ScalerWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for ScalerWidget {
    node_identity!("Range to Unit");
    fn description(&self) -> &'static str { "Maps the range from in min to in max onto 0..1, clamped. The range is set in the settings, or by wires into its in min and in max inputs." }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::scaler::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::scaler::ports().1) }

    fn default_promoted(&self) -> &'static [(&'static str, PromotedKind)] {
        &[("In Min", PromotedKind::Float), ("In Max", PromotedKind::Float)]
    }

    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 15.0 }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(format!("{:.2}", out));
    }
}
