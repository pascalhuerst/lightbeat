use egui::{self, Ui};

use crate::engine::nodes::transport::sample_hold::ports_for;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeWidget, node_identity};
use crate::widgets::nodes::transport::signal_type::SignalTypeSetting;
use crate::widgets::nodes::types::UiPortDef;

pub struct SampleHoldWidget {
    id: NodeId,
    shared: SharedState,
    value_type: SignalTypeSetting,
}

impl SampleHoldWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, value_type: SignalTypeSetting::new(0) }
    }
}

impl NodeWidget for SampleHoldWidget {
    fn restore_from_save(&mut self, _data: Option<&serde_json::Value>, params: &[(usize, ParamValue)]) {
        self.value_type.restore(params);
    }

    node_identity!("Sample & Hold");
    fn description(&self) -> &'static str {
        "Captures the value input on each rising edge of the trigger input. \
         Works on any type (a number, a color, a position ...); Type is set \
         by the first wire or in the inspector."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&ports_for(self.value_type.port_type).0)
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&ports_for(self.value_type.port_type).1)
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 20.0 }

    fn on_ui_connect(&mut self, input_port: usize, source_type: PortType) {
        if input_port == 0 { self.value_type.adopt(&self.shared, source_type); }
    }
    fn on_ui_output_connect(&mut self, output_port: usize, dest_type: PortType) {
        if output_port == 0 { self.value_type.adopt(&self.shared, dest_type); }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.value_type.sync(&self.shared);
        let held = {
            let shared = self.shared.lock();
            shared.outputs.first().copied().unwrap_or(0.0)
        };
        ui.colored_label(theme::TEXT, format!("held {}", self.value_type.summary(held)));
    }
}
