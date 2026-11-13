use std::any::Any;

use egui::{self, Ui};

use crate::engine::nodes::math::multiplex::{MUX_TYPE_NAMES, clamp_slots, type_to_index};
use crate::engine::nodes::math::port_hub::{HUB_DEFAULT_SLOTS, PortHubDisplay};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeLayout, NodeWidget, labelled_layout};
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
            port_type: PortType::Untyped,
            slots: HUB_DEFAULT_SLOTS,
        }
    }

    /// Apply restored state before the first frame's stale-connection
    /// sweep — same pattern as Mux/Demux.
    pub fn set_state_from_load(&mut self, port_type: PortType, slots: usize) {
        self.port_type = port_type;
        self.slots = clamp_slots(slots);
    }

    fn sync_from_display(&mut self) {
        let shared = self.shared.lock().unwrap();
        if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<PortHubDisplay>()) {
            if d.port_type != self.port_type { self.port_type = d.port_type; }
            if d.slots != self.slots { self.slots = d.slots; }
        }
    }
}

impl NodeWidget for PortHubWidget {
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

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.sync_from_display();
        ui.colored_label(
            theme::TEXT_DIM,
            format!("{} × {}", self.slots, MUX_TYPE_NAMES[type_to_index(self.port_type)]),
        );
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
