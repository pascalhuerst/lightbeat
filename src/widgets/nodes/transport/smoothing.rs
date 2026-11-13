use std::any::Any;
use egui::Ui;

use crate::engine::nodes::transport::smoothing::ports_for;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::transport::signal_type::SignalTypeSetting;
use crate::widgets::nodes::types::UiPortDef;

pub struct SmoothingWidget {
    id: NodeId,
    shared: SharedState,
    value_type: SignalTypeSetting,
}

impl SmoothingWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, value_type: SignalTypeSetting::new(2) }
    }
}

impl NodeWidget for SmoothingWidget {
    fn restore_from_save(&mut self, _data: Option<&serde_json::Value>, params: &[(usize, ParamValue)]) {
        self.value_type.restore(params);
    }

    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Smoothing" }
    fn description(&self) -> &'static str {
        "Low-pass smoothing for noisy control signals. Modes: Exponential \
         (one-pole IIR, α derived from window length), Moving Average \
         (arithmetic mean of the window), Median (picks the middle value — \
         rejects spikes). Window is in milliseconds."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&ports_for(self.value_type.port_type).0)
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&ports_for(self.value_type.port_type).1)
    }

    fn min_width(&self) -> f32 { 110.0 }
    fn min_content_height(&self) -> f32 { 15.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn on_ui_connect(&mut self, input_port: usize, source_type: PortType) {
        if input_port == 0 { self.value_type.adopt(&self.shared, source_type); }
    }
    fn on_ui_output_connect(&mut self, output_port: usize, dest_type: PortType) {
        if output_port == 0 { self.value_type.adopt(&self.shared, dest_type); }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.value_type.sync(&self.shared);
        let shared = self.shared.lock().unwrap();
        let out = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(self.value_type.summary(out));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
