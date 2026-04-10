use std::any::Any;

use egui::Ui;

use crate::engine::nodes::math::math_op::{MathDisplay, MathOp};
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct MathWidget {
    id: NodeId,
    op: MathOp,
    shared: SharedState,
    ui_inputs: Vec<UiPortDef>,
    ui_outputs: Vec<UiPortDef>,
}

impl MathWidget {
    pub fn new(id: NodeId, op: MathOp, shared: SharedState) -> Self {
        Self {
            id,
            op,
            shared,
            ui_inputs: vec![
                UiPortDef::from_def(&PortDef::new("a", PortType::Any)),
                UiPortDef::from_def(&PortDef::new("b", PortType::Any)),
            ],
            ui_outputs: vec![UiPortDef::from_def(&PortDef::new("out", PortType::Any))],
        }
    }

    fn update_output_type(&mut self) {
        let out_type = self.ui_inputs.iter()
            .find(|p| p.def.port_type != PortType::Any)
            .map(|p| p.def.port_type)
            .unwrap_or(PortType::Any);
        self.ui_outputs[0] = UiPortDef::from_def(&PortDef::new("out", out_type));
    }
}

impl NodeWidget for MathWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.op.label() }
    fn title(&self) -> &str { self.op.label() }

    fn ui_inputs(&self) -> Vec<UiPortDef> { self.ui_inputs.clone() }
    fn ui_outputs(&self) -> Vec<UiPortDef> { self.ui_outputs.clone() }

    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 15.0 }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn on_ui_connect(&mut self, input_port: usize, source_type: PortType) {
        if input_port < 2 {
            let name = if input_port == 0 { "a" } else { "b" };
            self.ui_inputs[input_port] = UiPortDef::from_def(&PortDef::new(name, source_type));
            self.update_output_type();
        }
    }

    fn on_ui_disconnect(&mut self, input_port: usize) {
        if input_port < 2 {
            let name = if input_port == 0 { "a" } else { "b" };
            self.ui_inputs[input_port] = UiPortDef::from_def(&PortDef::new(name, PortType::Any));
            self.update_output_type();
        }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(format!("{} {:.2}", self.op.symbol(), out));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
