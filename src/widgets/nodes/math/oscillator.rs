use std::any::Any;

use egui::Ui;

use crate::engine::nodes::math::oscillator::OscFunc;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct OscillatorWidget {
    id: NodeId,
    func: OscFunc,
    shared: SharedState,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl OscillatorWidget {
    pub fn new(id: NodeId, func: OscFunc, shared: SharedState) -> Self {
        Self {
            id,
            func,
            shared,
            inputs: vec![
                PortDef::new("phase", PortType::Phase),
                PortDef::new("amp", PortType::Untyped),
            ],
            outputs: vec![PortDef::new("out", PortType::Untyped)],
        }
    }
}

impl NodeWidget for OscillatorWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.func.label() }
    fn title(&self) -> &str { self.func.label() }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.inputs.iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.outputs.iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 15.0 }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(format!("{:.2}", out));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
