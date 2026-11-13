use std::any::Any;

use egui::{self, Ui};

use crate::engine::nodes::transport::latch::ports_for;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::transport::signal_type::SignalTypeSetting;
use crate::widgets::nodes::types::UiPortDef;

pub struct LatchWidget {
    id: NodeId,
    shared: SharedState,
    value_type: SignalTypeSetting,
}

impl LatchWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, value_type: SignalTypeSetting::new(0) }
    }
}

impl NodeWidget for LatchWidget {
    fn restore_from_save(&mut self, _data: Option<&serde_json::Value>, params: &[(usize, ParamValue)]) {
        self.value_type.restore(params);
    }

    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Tick Delay" }
    fn description(&self) -> &'static str {
        "Adds one tick of delay. Useful for explicit feedback paths."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&ports_for(self.value_type.port_type).0)
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&ports_for(self.value_type.port_type).1)
    }

    fn min_width(&self) -> f32 { 90.0 }
    fn min_content_height(&self) -> f32 { 18.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn on_ui_connect(&mut self, input_port: usize, source_type: PortType) {
        if input_port == 0 { self.value_type.adopt(&self.shared, source_type); }
    }
    fn on_ui_output_connect(&mut self, output_port: usize, dest_type: PortType) {
        if output_port == 0 { self.value_type.adopt(&self.shared, dest_type); }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.value_type.sync(&self.shared);
        let v = {
            let shared = self.shared.lock().unwrap();
            shared.outputs.first().copied().unwrap_or(0.0)
        };
        ui.colored_label(theme::TEXT_MUTED, self.value_type.summary(v));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
