use egui::Ui;

use crate::engine::nodes::io::push1::{ports, Push1Display};
use crate::engine::types::*;
use crate::input_controller::{InputControllerKind, SharedControllers};
use crate::widgets::nodes::io::controller_binding::{ControllerBinding, ControllerSpec};
use crate::widgets::nodes::node::{NodeLayout, NodeWidget, labelled_layout, node_identity};
use crate::widgets::nodes::types::UiPortDef;

static SPEC: ControllerSpec = ControllerSpec {
    label: "Push 1",
    id_salt: "push1_pick",
    is_kind: |k| matches!(k, InputControllerKind::Push1 { .. }),
};

pub struct Push1Widget {
    id: NodeId,
    shared: SharedState,
    binding: ControllerBinding,
}

impl Push1Widget {
    pub fn new(id: NodeId, shared: SharedState, controllers: SharedControllers) -> Self {
        Self {
            id, shared,
            binding: ControllerBinding::new(controllers, &SPEC),
        }
    }
}

impl NodeWidget for Push1Widget {
    fn restore_from_save(&mut self, data: Option<&serde_json::Value>, _params: &[(usize, crate::engine::types::ParamValue)]) {
        self.binding.restore_from_save(data);
    }

    node_identity!("Push 1");
    fn description(&self) -> &'static str {
        "Ableton Push 1 dedicated node. Groups the device's controls into \
         (trigger, address, value) channels — e.g. one `pad trigger` pulse per \
         pad event with x/y/velocity describing which pad. Feedback inputs \
         let you light pad / button LEDs by pulsing `set pad trigger` with the \
         latched coords and color."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&ports().1) }

    fn show_port_labels(&self) -> bool { true }
    fn layout(&self) -> NodeLayout { labelled_layout(&self.ui_inputs(), &self.ui_outputs()) }
    fn min_width(&self) -> f32 { 180.0 }
    fn min_content_height(&self) -> f32 { 24.0 }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let snap = {
            let shared = self.shared.lock();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<Push1Display>())
                .map(|d| (d.controller_id, d.controller_name.clone(), d.connected))
        };
        self.binding.sync_from(snap);
        self.binding.show_status(ui);
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        self.binding.show_picker(ui, self.id, &self.shared);
    }
}
