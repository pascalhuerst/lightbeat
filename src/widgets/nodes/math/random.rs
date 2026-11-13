use egui::{self, Ui};

use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeWidget, node_identity};
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
    node_identity!("Random");
    fn description(&self) -> &'static str {
        "Picks a new random value between min and max on each rising edge of the trigger and holds it. With steps it picks one of that many evenly spaced values; glide slides to the new value."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::random::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::random::ports().1) }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 20.0 }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let value = self.shared.lock().outputs.first().copied().unwrap_or(0.0);
        ui.colored_label(theme::TEXT, format!("{:.3}", value));
    }
}
