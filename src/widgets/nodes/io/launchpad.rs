use crate::engine::nodes::io::status::status_port;
use std::any::Any;

use egui::Ui;

use crate::engine::nodes::io::launchpad::LaunchpadDisplay;
use crate::engine::types::*;
use crate::input_controller::{InputControllerKind, SharedControllers};
use crate::widgets::nodes::io::controller_binding::{ControllerBinding, ControllerSpec};
use crate::widgets::nodes::node::{NodeLayout, NodeWidget, labelled_layout};
use crate::widgets::nodes::types::UiPortDef;

const N_PADS: usize = 64;
const N_SIDE: usize = 8;
const N_TOP: usize = 8;
const N_CONTROLS: usize = N_PADS + N_SIDE + N_TOP;

const TOP_LABELS: [&str; 8] = [
    "Up", "Down", "Left", "Right", "Session", "User 1", "User 2", "Mixer",
];

fn pad_name(row: u8, col: u8) -> String { format!("pad r{}c{}", row + 1, col + 1) }
fn side_name(row: u8) -> String { format!("side {}", row + 1) }

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

    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Launchpad S" }
    fn description(&self) -> &'static str {
        "Novation Launchpad S dedicated node. 64 grid pads (velocity-sensitive) \
         + 8 side buttons + 8 top mode buttons each expose an output and an \
         LED-feedback input. Each LED input takes a color; the Launchpad S can only show red and green (4 levels each, so amber and orange work), blue is ignored."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        // Each pad / side / top LED accepts a Color (R, G, B). The engine
        // quantizes R and G onto the Mk1's 4-level red × 4-level green
        // palette; B is discarded because the hardware can't show blue.
        let mut v = Vec::with_capacity(N_CONTROLS);
        for row in 0..8u8 {
            for col in 0..8u8 {
                v.push(UiPortDef::from_def(&PortDef::new(
                    format!("LED {}", pad_name(row, col)), PortType::Color,
                )));
            }
        }
        for row in 0..8u8 {
            v.push(UiPortDef::from_def(&PortDef::new(
                format!("LED {}", side_name(row)), PortType::Color,
            )));
        }
        for label in TOP_LABELS {
            v.push(UiPortDef::from_def(&PortDef::new(
                format!("LED top {}", label), PortType::Color,
            )));
        }
        v
    }

    fn ui_outputs(&self) -> Vec<UiPortDef> {
        let mut v = Vec::with_capacity(2 + N_CONTROLS);
        v.push(UiPortDef::from_def(&status_port()));
        v.push(UiPortDef::from_def(&PortDef::new("any change", PortType::Logic)));
        for row in 0..8u8 {
            for col in 0..8u8 {
                v.push(UiPortDef::from_def(&PortDef::new(pad_name(row, col), PortType::Untyped)));
            }
        }
        for row in 0..8u8 {
            v.push(UiPortDef::from_def(&PortDef::new(side_name(row), PortType::Untyped)));
        }
        for label in TOP_LABELS {
            v.push(UiPortDef::from_def(&PortDef::new(format!("top {}", label), PortType::Untyped)));
        }
        v
    }

    fn show_port_labels(&self) -> bool { true }
    fn layout(&self) -> NodeLayout { labelled_layout(&self.ui_inputs(), &self.ui_outputs()) }
    fn min_width(&self) -> f32 { 200.0 }
    fn min_content_height(&self) -> f32 { 24.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

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

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
