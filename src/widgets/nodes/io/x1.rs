use egui::Ui;

use crate::engine::nodes::io::x1::{ports, X1Display};
use crate::engine::types::*;
use crate::input_controller::{InputControllerKind, SharedControllers};
use crate::widgets::nodes::io::controller_binding::{ControllerBinding, ControllerSpec};
use crate::widgets::nodes::node::{NodeLayout, NodeWidget, labelled_layout, node_identity};
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

    node_identity!("X1");
    fn description(&self) -> &'static str {
        "Native Instruments Kontrol X1 dedicated node. Each physical button, \
         encoder, and pot gets its own output port; each button also has an \
         LED input port, so wiring any 0..1 signal into it sets that LED's \
         brightness."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&ports().1) }

    fn show_port_labels(&self) -> bool { true }
    fn layout(&self) -> NodeLayout { labelled_layout(&self.ui_inputs(), &self.ui_outputs()) }
    fn min_width(&self) -> f32 { 180.0 }
    fn min_content_height(&self) -> f32 { 24.0 }
    fn resizable(&self) -> bool { true }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let snap = {
            let shared = self.shared.lock();
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
}
