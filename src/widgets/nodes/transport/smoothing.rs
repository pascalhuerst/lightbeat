use std::any::Any;
use egui::Ui;

use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct SmoothingWidget {
    id: NodeId,
    shared: SharedState,
}

impl SmoothingWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for SmoothingWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Smoothing" }
    fn description(&self) -> &'static str {
        "Low-pass smoothing for noisy control signals. Modes: Exponential \
         (one-pole IIR, α derived from window length), Moving Average \
         (arithmetic mean of the window), Median (picks the middle value — \
         rejects spikes). Window is in engine ticks (~1 ms each)."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("in", PortType::Untyped))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", PortType::Untyped))]
    }

    fn min_width(&self) -> f32 { 110.0 }
    fn min_content_height(&self) -> f32 { 15.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(format!("{:.3}", out));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
