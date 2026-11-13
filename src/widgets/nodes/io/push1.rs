use crate::engine::nodes::io::status::status_port;
use std::any::Any;

use egui::Ui;

use crate::engine::nodes::io::push1::Push1Display;
use crate::engine::types::*;
use crate::input_controller::{InputControllerKind, SharedControllers};
use crate::widgets::nodes::io::controller_binding::{ControllerBinding, ControllerSpec};
use crate::widgets::nodes::node::{NodeLayout, NodeWidget, labelled_layout};
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

    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Push 1" }
    fn description(&self) -> &'static str {
        "Ableton Push 1 dedicated node. Groups the device's controls into \
         (trigger, address, value) channels — e.g. one `pad trigger` pulse per \
         pad event with x/y/velocity describing which pad. Feedback inputs \
         let you light pad / button LEDs by pulsing `set pad trigger` with the \
         latched coords and color."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        // Mirror the engine port layout.
        let defs = [
            ("set pad trigger", PortType::Logic),
            ("set pad x", PortType::Untyped),
            ("set pad y", PortType::Untyped),
            ("set pad color", PortType::Untyped),
            ("set btn top trigger", PortType::Logic),
            ("set btn top col", PortType::Untyped),
            ("set btn top color", PortType::Untyped),
            ("set btn bot trigger", PortType::Logic),
            ("set btn bot col", PortType::Untyped),
            ("set btn bot color", PortType::Untyped),
        ];
        let mut v: Vec<UiPortDef> = defs.iter()
            .map(|(n, t)| UiPortDef::from_def(&PortDef::new((*n).to_string(), *t)))
            .collect();
        for name in NAMED {
            v.push(UiPortDef::from_def(&PortDef::new((*name).to_string(), PortType::Logic)));
        }
        v
    }

    fn ui_outputs(&self) -> Vec<UiPortDef> {
        let defs = [
            ("pad trigger", PortType::Logic),
            ("pad x", PortType::Untyped),
            ("pad y", PortType::Untyped),
            ("pad vel", PortType::Untyped),
            ("btn top trigger", PortType::Logic),
            ("btn top col", PortType::Untyped),
            ("btn top val", PortType::Logic),
            ("btn bot trigger", PortType::Logic),
            ("btn bot col", PortType::Untyped),
            ("btn bot val", PortType::Logic),
            ("enc trigger", PortType::Logic),
            ("enc idx", PortType::Untyped),
            ("enc val", PortType::Untyped),
            ("slider", PortType::Untyped),
        ];
        let mut v: Vec<UiPortDef> = std::iter::once(UiPortDef::from_def(&status_port()))
            .chain(defs.iter()
                .map(|(n, t)| UiPortDef::from_def(&PortDef::new((*n).to_string(), *t))))
            .collect();
        for name in NAMED {
            v.push(UiPortDef::from_def(&PortDef::new((*name).to_string(), PortType::Logic)));
        }
        v
    }

    fn show_port_labels(&self) -> bool { true }
    fn layout(&self) -> NodeLayout { labelled_layout(&self.ui_inputs(), &self.ui_outputs()) }
    fn min_width(&self) -> f32 { 180.0 }
    fn min_content_height(&self) -> f32 { 24.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let snap = {
            let shared = self.shared.lock().unwrap();
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

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

const NAMED: &[&str] = &[
    "Play", "Record", "Stop", "Shift", "Select",
    "Up", "Down", "Left", "Right", "Octave Up", "Octave Down",
];
