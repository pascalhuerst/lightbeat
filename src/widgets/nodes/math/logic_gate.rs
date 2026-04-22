use std::any::Any;
use egui::{Color32, Ui};

use crate::engine::nodes::math::logic_gate::{LogicDisplay, LogicOp};
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const ADD_PORT_FILL: Color32 = Color32::from_rgb(80, 200, 100);

pub struct LogicGateWidget {
    id: NodeId,
    op: LogicOp,
    shared: SharedState,
    /// Number of "real" inputs. NOT is fixed at 1.
    input_count: usize,
}

fn input_label(i: usize) -> String {
    if i < 26 {
        ((b'a' + i as u8) as char).to_string()
    } else {
        format!("in{}", i + 1)
    }
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
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.op.label() }
    fn title(&self) -> &str { self.op.label() }
    fn description(&self) -> &'static str {
        "Boolean logic gate. AND/OR/XOR fold across all inputs; drag to the green + port to add another. NOT is unary."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        if self.op == LogicOp::Not {
            return vec![UiPortDef::from_def(&PortDef::new("in", PortType::Logic))];
        }
        let mut ports: Vec<UiPortDef> = (0..self.input_count).map(|i| {
            UiPortDef::from_def(&PortDef::new(input_label(i), PortType::Logic))
        }).collect();
        ports.push(
            UiPortDef::from_def(&PortDef::new("+", PortType::Logic))
                .with_fill(ADD_PORT_FILL)
                .with_marker("+"),
        );
        ports
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", PortType::Logic))]
    }
    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 15.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

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

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
