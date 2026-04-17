use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::transport::hold::TriggerHoldDisplay;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct TriggerHoldWidget {
    id: NodeId,
    shared: SharedState,
}

impl TriggerHoldWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for TriggerHoldWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Trigger Hold" }
    fn title(&self) -> &str { "Trigger Hold" }
    fn description(&self) -> &'static str {
        "Holds a trigger high for N ticks after each rising edge. Duration via input or param."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("trigger", PortType::Logic)),
            UiPortDef::from_def(&PortDef::new("duration", PortType::Untyped)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", PortType::Logic))]
    }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 28.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let (remaining, dur) = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<TriggerHoldDisplay>())
                .map(|d| (d.remaining_ticks, d.effective_duration))
                .unwrap_or((0, 0))
        };

        ui.horizontal(|ui| {
            if remaining > 0 {
                ui.colored_label(
                    Color32::from_rgb(80, 200, 100),
                    format!("HIGH {}/{}", remaining, dur),
                );
            } else {
                ui.colored_label(
                    Color32::from_gray(120),
                    format!("idle ({})", dur),
                );
            }
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
