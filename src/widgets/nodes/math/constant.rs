use std::any::Any;

use egui::Ui;

use crate::engine::nodes::math::constant::ConstantProcessNode;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct ConstantWidget {
    id: NodeId,
    port_type: PortType,
    shared: SharedState,
    outputs: Vec<PortDef>,
}

impl ConstantWidget {
    pub fn new(id: NodeId, port_type: PortType, shared: SharedState) -> Self {
        Self {
            id,
            port_type,
            shared,
            outputs: vec![PortDef::new("out", port_type)],
        }
    }
}

impl NodeWidget for ConstantWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { ConstantProcessNode::type_name_for(self.port_type) }
    fn description(&self) -> &'static str { "Constant value source for any port type, edited via the inspector." }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.outputs.iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 90.0 }
    fn min_content_height(&self) -> f32 { 15.0 }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let val = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);

        let label = match self.port_type {
            PortType::Logic => if val >= 0.5 { "HIGH" } else { "LOW" },
            _ => "",
        };

        if label.is_empty() {
            ui.label(format!("{:.3}", val));
        } else {
            ui.label(label);
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
