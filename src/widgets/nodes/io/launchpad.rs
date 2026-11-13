use egui::Ui;

use crate::engine::nodes::io::launchpad::{ports, LaunchpadDisplay};
use crate::engine::types::*;
use crate::input_controller::{InputControllerKind, SharedControllers};
use crate::widgets::nodes::io::controller_binding::{ControllerBinding, ControllerSpec};
use crate::widgets::nodes::node::{NodeLayout, NodeWidget, labelled_layout, node_identity};
use crate::widgets::nodes::types::UiPortDef;

static SPEC: ControllerSpec = ControllerSpec {
    label: "Launchpad",
    id_salt: "launchpad_pick",
    is_kind: |k| matches!(k, InputControllerKind::LaunchpadS { .. }),
};

pub struct LaunchpadWidget {
    id: NodeId,
    shared: SharedState,
    binding: ControllerBinding,
}

impl LaunchpadWidget {
    pub fn new(id: NodeId, shared: SharedState, controllers: SharedControllers) -> Self {
        Self {
            id, shared,
            binding: ControllerBinding::new(controllers, &SPEC),
        }
    }
}

impl NodeWidget for LaunchpadWidget {
    fn restore_from_save(&mut self, data: Option<&serde_json::Value>, _params: &[(usize, crate::engine::types::ParamValue)]) {
        self.binding.restore_from_save(data);
    }

    node_identity!("Launchpad S");
    fn description(&self) -> &'static str {
        "Novation Launchpad S dedicated node. 64 grid pads (velocity-sensitive) \
         + 8 side buttons + 8 top mode buttons each expose an output and an \
         LED-feedback input. Each LED input takes a color; the Launchpad S can only show red and green (4 levels each, so amber and orange work), blue is ignored."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&ports().1) }

    fn show_port_labels(&self) -> bool { true }
    fn layout(&self) -> NodeLayout { labelled_layout(&self.ui_inputs(), &self.ui_outputs()) }
    fn min_width(&self) -> f32 { 200.0 }
    fn min_content_height(&self) -> f32 { 24.0 }
    fn resizable(&self) -> bool { true }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let snap = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<LaunchpadDisplay>())
                .map(|d| (d.controller_id, d.controller_name.clone(), d.connected))
        };
        self.binding.sync_from(snap);
        self.binding.show_status(ui);
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        self.binding.show_picker(ui, self.id, &self.shared);
    }
}
