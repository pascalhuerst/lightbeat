use egui::Ui;

use crate::engine::nodes::transport::clock_gen::ClockGenDisplay;
use crate::engine::types::*;
use crate::widgets::nodes::node::{NodeWidget, node_identity, with_display};
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
    node_identity!("Clock Gen");
    fn description(&self) -> &'static str { "Generates N evenly spaced triggers per phase cycle." }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::transport::clock_gen::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::transport::clock_gen::ports().1) }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 15.0 }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let count = with_display::<ClockGenDisplay, _>(&self.shared, |d| d.count).unwrap_or(4);
        ui.label(format!("×{}", count));
    }
}
