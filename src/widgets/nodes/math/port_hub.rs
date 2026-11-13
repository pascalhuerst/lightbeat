use std::any::Any;

use egui::{self, Ui};

use crate::engine::nodes::math::multiplex::{MUX_TYPE_NAMES, clamp_slots, type_from_index, type_to_index};
use crate::engine::nodes::math::port_hub::{HUB_DEFAULT_SLOTS, PortHubDisplay};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeLayout, NodeWidget, labelled_layout, param_usize};
use crate::widgets::nodes::types::UiPortDef;

pub struct PortHubWidget {
    id: NodeId,
    shared: SharedState,
    port_type: PortType,
    slots: usize,
}

impl PortHubWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            port_type: PortType::Any,
            slots: HUB_DEFAULT_SLOTS,
        }
    }

    fn sync_from_display(&mut self) {
        let shared = self.shared.lock().unwrap();
        if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<PortHubDisplay>()) {
            if d.port_type != self.port_type { self.port_type = d.port_type; }
            if d.slots != self.slots { self.slots = d.slots; }
        }
    }

    /// Only the Auto setting adopts the first wire's type; every slot shares it.
    fn adopt(&mut self, wire_type: PortType) {
        if self.port_type != PortType::Any || wire_type == PortType::Any { return; }
        self.port_type = wire_type;
        let mut s = self.shared.lock().unwrap();
        s.pending_params.push((0, ParamValue::Choice(type_to_index(wire_type))));
    }
}

impl NodeWidget for PortHubWidget {
    fn restore_from_save(&mut self, _data: Option<&serde_json::Value>, params: &[(usize, ParamValue)]) {
        self.port_type = param_usize(params, 0).map(type_from_index).unwrap_or(PortType::Any);
        self.slots = clamp_slots(param_usize(params, 1).unwrap_or(HUB_DEFAULT_SLOTS));
    }

    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Port Hub" }
    fn description(&self) -> &'static str {
        "Pass-through bundle: N input ports paired 1:1 with N outputs of \
         the same type. Does nothing to the values, exists only to tidy \
         up long wire runs or give a grab handle for copy/paste. Type \
         and slot count are set in the inspector."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        (0..self.slots)
            .map(|i| UiPortDef::from_def(&PortDef::new(format!("in{}", i + 1), self.port_type)))
            .collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        (0..self.slots)
            .map(|i| UiPortDef::from_def(&PortDef::new(format!("out{}", i + 1), self.port_type)))
            .collect()
    }

    fn show_port_labels(&self) -> bool { true }
    fn layout(&self) -> NodeLayout { labelled_layout(&self.ui_inputs(), &self.ui_outputs()) }
    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 14.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn on_ui_connect(&mut self, _input_port: usize, source_type: PortType) {
        self.adopt(source_type);
    }
    fn on_ui_output_connect(&mut self, _output_port: usize, dest_type: PortType) {
        self.adopt(dest_type);
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.sync_from_display();
        ui.colored_label(
            theme::TEXT_DIM,
            format!("{} × {}", self.slots, MUX_TYPE_NAMES[type_to_index(self.port_type)]),
        );
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hub() -> PortHubWidget {
        PortHubWidget::new(NodeId(1), new_shared_state(1024, 1024))
    }

    #[test]
    fn auto_takes_the_type_of_the_first_wire_on_either_side() {
        let mut h = hub();
        h.on_ui_connect(0, PortType::Color);
        assert_eq!(h.port_type, PortType::Color);
        assert_eq!(h.shared.lock().unwrap().pending_params.len(), 1);

        let mut h = hub();
        h.on_ui_output_connect(2, PortType::Position);
        assert_eq!(h.port_type, PortType::Position);
    }

    #[test]
    fn a_chosen_type_is_not_replaced_by_a_wire() {
        let mut h = hub();
        h.port_type = PortType::Logic;
        h.on_ui_connect(0, PortType::Color);
        assert_eq!(h.port_type, PortType::Logic);
        assert!(h.shared.lock().unwrap().pending_params.is_empty());
    }
}
