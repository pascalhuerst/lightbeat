use egui::Ui;

use crate::engine::nodes::math::oscillator::OscFunc;
use crate::engine::types::*;
use crate::widgets::nodes::node::{NodeWidget, node_identity};
use crate::widgets::nodes::types::UiPortDef;

pub struct OscillatorWidget {
    id: NodeId,
    func: OscFunc,
    shared: SharedState,
}

impl OscillatorWidget {
    pub fn new(id: NodeId, func: OscFunc, shared: SharedState) -> Self {
        Self {
            id,
            func,
            shared,
        }
    }
}

impl NodeWidget for OscillatorWidget {
    node_identity!(by |w| w.func.label());
    fn title(&self) -> &str { self.func.label() }
    fn description(&self) -> &'static str { "Sine or cosine wave generator driven by phase, scaled by amplitude." }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::oscillator::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::oscillator::ports().1) }

    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 15.0 }


    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(format!("{:.2}", out));
    }
}
