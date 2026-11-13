use egui::Ui;

use crate::engine::types::*;
use crate::widgets::nodes::node::{NodeWidget, node_identity};
use crate::widgets::nodes::types::UiPortDef;

pub struct BipolarWidget {
    id: NodeId,
    shared: SharedState,
}

impl BipolarWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for BipolarWidget {
    node_identity!("Bipolar");
    fn description(&self) -> &'static str {
        "Maps a unipolar input (0..1) to a bipolar signal centered on `center` with ±range/2 swing. \
         Formula: out = (in - 0.5) * range + center. Defaults (range=1, center=0) give -0.5..+0.5."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::bipolar::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::bipolar::ports().1) }

    fn min_width(&self) -> f32 { 90.0 }
    fn min_content_height(&self) -> f32 { 15.0 }

    fn default_promoted(&self) -> &'static [(&'static str, PromotedKind)] {
        &[("Range", PromotedKind::Float), ("Center", PromotedKind::Float)]
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let s = self.shared.lock();
        let out = s.outputs.first().copied().unwrap_or(0.0);
        drop(s);
        ui.label(format!("{:+.2}", out));
    }
}
