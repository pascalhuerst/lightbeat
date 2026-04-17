use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

#[derive(Debug, Clone, Copy)]
pub enum FlipFlopKind { Sr, Jk }

pub struct FlipFlopWidget {
    id: NodeId,
    shared: SharedState,
    kind: FlipFlopKind,
}

impl FlipFlopWidget {
    pub fn new(id: NodeId, shared: SharedState, kind: FlipFlopKind) -> Self {
        Self { id, shared, kind }
    }
}

impl NodeWidget for FlipFlopWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str {
        match self.kind {
            FlipFlopKind::Sr => "Flip-Flop",
            FlipFlopKind::Jk => "JK Flip-Flop",
        }
    }
    fn title(&self) -> &str {
        match self.kind {
            FlipFlopKind::Sr => "SR FF",
            FlipFlopKind::Jk => "JK FF",
        }
    }
    fn description(&self) -> &'static str {
        match self.kind {
            FlipFlopKind::Sr =>
                "SR latch (level-sensitive). S sets Q, R resets Q (R wins ties).",
            FlipFlopKind::Jk =>
                "JK flip-flop (rising-edge clk). J=K=1 toggles, J alone sets, K alone resets.",
        }
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        match self.kind {
            FlipFlopKind::Sr => vec![
                UiPortDef::from_def(&PortDef::new("S", PortType::Logic)),
                UiPortDef::from_def(&PortDef::new("R", PortType::Logic)),
            ],
            FlipFlopKind::Jk => vec![
                UiPortDef::from_def(&PortDef::new("J", PortType::Logic)),
                UiPortDef::from_def(&PortDef::new("K", PortType::Logic)),
                UiPortDef::from_def(&PortDef::new("clk", PortType::Logic)),
            ],
        }
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("Q", PortType::Logic)),
            UiPortDef::from_def(&PortDef::new("!Q", PortType::Logic)),
        ]
    }

    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 18.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let q = {
            let shared = self.shared.lock().unwrap();
            shared.outputs.first().copied().unwrap_or(0.0) >= 0.5
        };
        let (text, color) = if q {
            ("Q HIGH", Color32::from_rgb(80, 200, 100))
        } else {
            ("Q LOW", Color32::from_gray(140))
        };
        ui.colored_label(color, text);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
