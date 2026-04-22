use std::any::Any;

use egui::{Color32, Ui};

use crate::engine::nodes::math::math_op::{MathDisplay, MathOp};
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const ADD_PORT_FILL: Color32 = Color32::from_rgb(80, 200, 100);

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

fn input_label(i: usize) -> String {
    if i < 26 {
        ((b'a' + i as u8) as char).to_string()
    } else {
        format!("in{}", i + 1)
    }
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
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({
            "input_count": self.input_count,
        }));
    }

    fn output_type(&self) -> PortType {
        self.real_input_types.iter().find_map(|t| *t).unwrap_or(PortType::Any)
    }
}

impl NodeWidget for MathWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.op.label() }
    fn title(&self) -> &str { self.op.label() }
    fn description(&self) -> &'static str {
        "Arithmetic operation folded across all inputs. Drag to the green + port to add another input."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        let mut ports: Vec<UiPortDef> = (0..self.input_count).map(|i| {
            let ty = self.real_input_types.get(i).copied().flatten().unwrap_or(PortType::Any);
            UiPortDef::from_def(&PortDef::new(input_label(i), ty))
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

    fn shared_state(&self) -> &SharedState { &self.shared }

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
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<MathDisplay>())
                .map(|d| d.input_count)
        };
        if let Some(n) = synced_count
            && n != self.input_count {
                self.input_count = n.max(1);
                self.real_input_types.resize(self.input_count, None);
            }

        let shared = self.shared.lock().unwrap();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(format!("{} {:.2}", self.op.symbol(), out));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
