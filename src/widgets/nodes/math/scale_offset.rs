use egui::Ui;

use crate::engine::types::*;
use crate::widgets::nodes::node::{NodeWidget, node_identity};
use crate::widgets::nodes::types::UiPortDef;

pub struct ScaleOffsetWidget {
    id: NodeId,
    shared: SharedState,
}

impl ScaleOffsetWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for ScaleOffsetWidget {
    node_identity!("Scale Offset");
    fn description(&self) -> &'static str {
        "Multiplies the input by a scale and adds an offset: out = in * scale + offset. Wire scale or offset to drive them from a signal."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::scale_offset::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::scale_offset::ports().1) }

    fn min_width(&self) -> f32 { 90.0 }
    fn min_content_height(&self) -> f32 { 15.0 }

    fn overridden_param_indices(&self) -> Vec<usize> {
        // Scale = param 0, wired from port 1. Offset = param 1, wired from port 2.
        let s = self.shared.lock();
        let mut hidden = Vec::new();
        if s.inputs_connected.get(1).copied().unwrap_or(false) { hidden.push(0); }
        if s.inputs_connected.get(2).copied().unwrap_or(false) { hidden.push(1); }
        hidden
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let s = self.shared.lock();
        let out = s.outputs.first().copied().unwrap_or(0.0);
        drop(s);
        ui.label(format!("{:+.2}", out));
    }
}
