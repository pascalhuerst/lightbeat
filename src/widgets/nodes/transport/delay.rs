use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::transport::delay::TriggerDelayDisplay;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct TriggerDelayWidget {
    id: NodeId,
    shared: SharedState,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl TriggerDelayWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            inputs: vec![
                PortDef::new("trigger", PortType::Logic),
                PortDef::new("phase", PortType::Phase),
            ],
            outputs: vec![PortDef::new("trigger", PortType::Logic)],
        }
    }
}

impl NodeWidget for TriggerDelayWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Trigger Delay" }
    fn title(&self) -> &str { "Trigger Delay" }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.inputs.iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.outputs.iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 110.0 }
    fn min_content_height(&self) -> f32 { 20.0 }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<TriggerDelayDisplay>());

        let (num, den, pending) = if let Some(d) = display {
            (d.numerator, d.denominator, d.has_pending)
        } else {
            (1, 4, false)
        };
        drop(shared);

        ui.horizontal(|ui| {
            let label = if den == 1 {
                format!("{} beat", num)
            } else {
                format!("{}/{} beat", num, den)
            };

            let color = if pending {
                Color32::from_rgb(240, 200, 40)
            } else {
                Color32::from_gray(180)
            };
            ui.colored_label(color, label);
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
