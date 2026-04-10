use std::any::Any;
use egui::Ui;

use crate::engine::nodes::math::logic_gate::LogicOp;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct LogicGateWidget {
    id: NodeId,
    op: LogicOp,
    shared: SharedState,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl LogicGateWidget {
    pub fn new(id: NodeId, op: LogicOp, shared: SharedState) -> Self {
        let inputs = if op == LogicOp::Not {
            vec![PortDef::new("in", PortType::Logic)]
        } else {
            vec![
                PortDef::new("a", PortType::Logic),
                PortDef::new("b", PortType::Logic),
            ]
        };
        Self {
            id, op, shared, inputs,
            outputs: vec![PortDef::new("out", PortType::Logic)],
        }
    }
}

impl NodeWidget for LogicGateWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.op.label() }
    fn title(&self) -> &str { self.op.label() }
    fn ui_inputs(&self) -> Vec<UiPortDef> { self.inputs.iter().map(UiPortDef::from_def).collect() }
    fn ui_outputs(&self) -> Vec<UiPortDef> { self.outputs.iter().map(UiPortDef::from_def).collect() }
    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 15.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(if out >= 0.5 { "HIGH" } else { "LOW" });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
