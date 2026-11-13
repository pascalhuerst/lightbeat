use egui::Ui;

use crate::engine::nodes::math::compare::CompareOp;
use crate::engine::types::*;
use crate::widgets::nodes::node::{current_choice, NodeWidget, node_identity};
use crate::widgets::nodes::types::UiPortDef;

pub struct CompareWidget {
    id: NodeId,
    op: CompareOp,
    shared: SharedState,
}

impl CompareWidget {
    pub fn new(id: NodeId, op: CompareOp, shared: SharedState) -> Self {
        Self {
            id, op, shared,
        }
    }
}

impl NodeWidget for CompareWidget {
    node_identity!(by |w| w.op.label());
    fn follow_params(&mut self) {
        if let Some(i) = current_choice(&self.shared, 0) {
            self.op = CompareOp::from_index(i);
        }
    }
    fn title(&self) -> &str { self.op.label() }
    fn description(&self) -> &'static str { "Compares two values (>=, <=, ==, !=) and outputs a logic signal." }
    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::compare::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::compare::ports().1) }
    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 15.0 }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(if out >= 0.5 { "TRUE" } else { "FALSE" });
    }
}
