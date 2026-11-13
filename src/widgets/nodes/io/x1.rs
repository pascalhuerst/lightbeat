use crate::engine::nodes::io::status::status_port;
use std::any::Any;

use egui::Ui;

use crate::engine::nodes::io::x1::X1Display;
use crate::engine::types::*;
use crate::input_controller::x1::{
    ALL_BUTTONS, ALL_ENCODERS, ALL_POTS,
    button_led_index, button_name, encoder_name, pot_name,
};
use crate::input_controller::{InputControllerKind, SharedControllers};
use crate::widgets::nodes::io::controller_binding::{ControllerBinding, ControllerSpec};
use crate::widgets::nodes::node::{NodeLayout, NodeWidget, labelled_layout};
use crate::widgets::nodes::types::UiPortDef;

static SPEC: ControllerSpec = ControllerSpec {
    label: "X1",
    id_salt: "x1_pick",
    is_kind: |k| matches!(k, InputControllerKind::X1),
};

pub struct X1Widget {
    id: NodeId,
    shared: SharedState,
    binding: ControllerBinding,
}

impl X1Widget {
    pub fn new(id: NodeId, shared: SharedState, controllers: SharedControllers) -> Self {
        Self {
            id, shared,
            binding: ControllerBinding::new(controllers, &SPEC),
        }
    }
}

impl NodeWidget for X1Widget {
    fn restore_from_save(&mut self, data: Option<&serde_json::Value>, _params: &[(usize, crate::engine::types::ParamValue)]) {
        self.binding.restore_from_save(data);
    }

    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "X1" }
    fn description(&self) -> &'static str {
        "Native Instruments Kontrol X1 dedicated node. Each physical button, \
         encoder, and pot gets its own output port; each button also has an \
         LED input port, so wiring any 0..1 signal into it sets that LED's \
         brightness."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        ALL_BUTTONS
            .iter()
            .copied()
            .filter(|&b| button_led_index(b).is_some())
            .map(|b| {
                UiPortDef::from_def(&PortDef::new(
                    format!("LED {}", button_name(b)),
                    PortType::Untyped,
                ))
            })
            .collect()
    }

    fn ui_outputs(&self) -> Vec<UiPortDef> {
        let mut v = Vec::with_capacity(2 + ALL_BUTTONS.len() + ALL_ENCODERS.len() + ALL_POTS.len());
        v.push(UiPortDef::from_def(&status_port()));
        v.push(UiPortDef::from_def(&PortDef::new("any change", PortType::Logic)));
        for &b in ALL_BUTTONS {
            v.push(UiPortDef::from_def(&PortDef::new(button_name(b), PortType::Untyped)));
        }
        for &e in ALL_ENCODERS {
            v.push(UiPortDef::from_def(&PortDef::new(encoder_name(e), PortType::Untyped)));
        }
        for &p in ALL_POTS {
            v.push(UiPortDef::from_def(&PortDef::new(pot_name(p), PortType::Untyped)));
        }
        v
    }

    fn show_port_labels(&self) -> bool { true }
    fn layout(&self) -> NodeLayout { labelled_layout(&self.ui_inputs(), &self.ui_outputs()) }
    fn min_width(&self) -> f32 { 180.0 }
    fn min_content_height(&self) -> f32 { 24.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let snap = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<X1Display>())
                .map(|d| (d.controller_id, d.controller_name.clone(), d.connected))
        };
        self.binding.sync_from(snap);
        self.binding.show_status(ui);
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        self.binding.show_picker(ui, self.id, &self.shared);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
