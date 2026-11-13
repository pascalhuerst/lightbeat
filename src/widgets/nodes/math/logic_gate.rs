use egui::{Color32, Ui};

use crate::engine::nodes::math::logic_gate::{ports_for, LogicDisplay, LogicOp};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{current_choice, NodeWidget, node_identity};
use crate::widgets::nodes::types::UiPortDef;

const ADD_PORT_FILL: Color32 = theme::SEM_SUCCESS;

pub struct LogicGateWidget {
    id: NodeId,
    op: LogicOp,
    shared: SharedState,
    /// Number of "real" inputs. NOT is fixed at 1.
    input_count: usize,
}

impl LogicGateWidget {
    pub fn new(id: NodeId, op: LogicOp, shared: SharedState) -> Self {
        let input_count = if op == LogicOp::Not { 1 } else { 2 };
        Self { id, op, shared, input_count }
    }

    fn push_config(&self) {
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({
            "input_count": self.input_count,
        }));
    }
}

impl NodeWidget for LogicGateWidget {
    node_identity!(by |w| w.op.label());
    fn follow_params(&mut self) {
        if self.op == LogicOp::Not { return; }
        if let Some(op) = current_choice(&self.shared, 0).and_then(|i| LogicOp::VARIADIC.get(i)) {
            self.op = *op;
        }
    }
    fn title(&self) -> &str { self.op.label() }
    fn description(&self) -> &'static str {
        "Boolean logic gate. AND/OR/XOR fold across all inputs; drag to the green + port to add another. NOT is unary."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        let mut ports = UiPortDef::from_defs(&ports_for(self.op, self.input_count).0);
        if self.op == LogicOp::Not {
            return ports;
        }
        ports.push(
            UiPortDef::from_def(&PortDef::new("+", PortType::Logic))
                .with_fill(ADD_PORT_FILL)
                .with_marker("+"),
        );
        ports
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&ports_for(self.op, self.input_count).1)
    }
    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 15.0 }

    fn on_ui_connect(&mut self, input_port: usize, _source_type: PortType) {
        if self.op == LogicOp::Not { return; }
        if input_port == self.input_count {
            self.input_count += 1;
            self.push_config();
        }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let synced_count = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<LogicDisplay>())
                .map(|d| d.input_count)
        };
        if let Some(n) = synced_count
            && self.op != LogicOp::Not && n != self.input_count {
                self.input_count = n.max(1);
            }

        let shared = self.shared.lock().unwrap();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(if out >= 0.5 { "HIGH" } else { "LOW" });
    }
}
