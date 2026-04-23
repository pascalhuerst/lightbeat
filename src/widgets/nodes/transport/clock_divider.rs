use std::any::Any;
use egui::{self, Color32, Ui};

use crate::engine::nodes::transport::clock_divider::ClockDividerDisplay;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct ClockDividerWidget {
    id: NodeId,
    shared: SharedState,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl ClockDividerWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id, shared,
            inputs: vec![PortDef::new("trigger", PortType::Logic)],
            outputs: vec![PortDef::new("trigger", PortType::Logic)],
        }
    }
}

impl NodeWidget for ClockDividerWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Clock Divider" }
    fn description(&self) -> &'static str { "Emits one trigger for every N input triggers." }

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
            .and_then(|d| d.downcast_ref::<ClockDividerDisplay>());

        let (divisor, count) = if let Some(d) = display {
            (d.divisor, d.count)
        } else {
            (2, 0)
        };
        drop(shared);

        ui.horizontal(|ui| {
            ui.label(format!("1/{}", divisor));
            // Show progress dots.
            let progress = format!("{}/{}", count + 1, divisor);
            ui.colored_label(Color32::from_gray(120), progress);
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
