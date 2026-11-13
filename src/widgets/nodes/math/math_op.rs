use egui::{Color32, Ui};

use crate::engine::nodes::math::math_op::{port_name, MathDisplay, MathOp};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{current_choice, NodeWidget, node_identity};
use crate::widgets::nodes::types::UiPortDef;

const ADD_PORT_FILL: Color32 = theme::SEM_SUCCESS;

pub struct MathWidget {
    id: NodeId,
    op: MathOp,
    shared: SharedState,
    /// Number of "real" inputs (a, b, c, ...). The widget always shows one
    /// extra "+" port at the end for adding more.
    input_count: usize,
    /// One per real input; None = unconnected (Any).
    real_input_types: Vec<Option<PortType>>,
}

impl MathWidget {
    pub fn new(id: NodeId, op: MathOp, shared: SharedState) -> Self {
        Self {
            id, op, shared,
            input_count: 2,
            real_input_types: vec![None; 2],
        }
    }

    fn push_config(&self) {
        let mut shared = self.shared.lock();
        shared.pending_config = Some(serde_json::json!({
            "input_count": self.input_count,
        }));
    }

    fn output_type(&self) -> PortType {
        self.real_input_types.iter().find_map(|t| *t).unwrap_or(PortType::Any)
    }
}

impl NodeWidget for MathWidget {
    node_identity!(by |w| w.op.label());
    fn follow_params(&mut self) {
        if let Some(i) = current_choice(&self.shared, 0) {
            self.op = MathOp::from_index(i);
        }
    }
    fn title(&self) -> &str { self.op.label() }
    fn neutral_input_value(&self, _port: usize) -> f32 {
        match self.op {
            MathOp::Mul | MathOp::Div => 1.0,
            _ => 0.0,
        }
    }
    fn description(&self) -> &'static str {
        "Arithmetic operation folded across all inputs. Drag to the green + port to add another input."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        let mut ports: Vec<UiPortDef> = (0..self.input_count).map(|i| {
            let ty = self.real_input_types.get(i).copied().flatten().unwrap_or(PortType::Any);
            UiPortDef::from_def(&PortDef::new(port_name(i), ty))
        }).collect();
        // The "+" add port is always last.
        ports.push(
            UiPortDef::from_def(&PortDef::new("+", PortType::Any))
                .with_fill(ADD_PORT_FILL)
                .with_marker("+"),
        );
        ports
    }

    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", self.output_type()))]
    }

    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 15.0 }


    fn on_ui_connect(&mut self, input_port: usize, source_type: PortType) {
        if input_port == self.input_count {
            // Connection landed on the "+" port — promote to a real input
            // and grow by one (which puts a fresh "+" beneath it next frame).
            self.real_input_types.push(Some(source_type));
            self.input_count += 1;
            self.push_config();
        } else if input_port < self.input_count {
            self.real_input_types[input_port] = Some(source_type);
        }
    }

    fn on_ui_disconnect(&mut self, input_port: usize) {
        if let Some(slot) = self.real_input_types.get_mut(input_port) {
            *slot = None;
        }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        // Sync input_count from engine display (handles project load).
        let synced_count = {
            let shared = self.shared.lock();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<MathDisplay>())
                .map(|d| d.input_count)
        };
        if let Some(n) = synced_count
            && n != self.input_count {
                self.input_count = n.max(1);
                self.real_input_types.resize(self.input_count, None);
            }

        let shared = self.shared.lock();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(format!("{} {:.2}", self.op.symbol(), out));
    }
}
