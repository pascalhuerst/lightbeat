use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::io::launchpad::LaunchpadDisplay;
use crate::engine::types::*;
use crate::input_controller::{InputControllerKind, SharedControllers};
use crate::widgets::nodes::node::NodeWidget;
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

pub struct LaunchpadWidget {
    id: NodeId,
    shared: SharedState,
    controller_id: u32,
    controllers: SharedControllers,
    connected: bool,
    controller_name: String,
}

impl LaunchpadWidget {
    pub fn new(id: NodeId, shared: SharedState, controllers: SharedControllers) -> Self {
        Self {
            id, shared,
            controller_id: 0,
            controllers,
            connected: false,
            controller_name: String::new(),
        }
    }

    fn push_config(&self) {
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({
            "controller_id": self.controller_id,
        }));
    }

    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Some(id) = data.get("controller_id").and_then(|v| v.as_u64()) {
            self.controller_id = id as u32;
        }
    }
}

impl NodeWidget for LaunchpadWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Launchpad S" }
    fn description(&self) -> &'static str {
        "Novation Launchpad S dedicated node. 64 grid pads (velocity-sensitive) \
         + 8 side buttons + 8 top mode buttons each expose an output and an \
         LED-feedback input. LED inputs currently drive red brightness."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        let mut v = Vec::with_capacity(N_CONTROLS);
        for row in 0..8u8 {
            for col in 0..8u8 {
                v.push(UiPortDef::from_def(&PortDef::new(
                    format!("LED {}", pad_name(row, col)), PortType::Untyped,
                )));
            }
        }
        for row in 0..8u8 {
            v.push(UiPortDef::from_def(&PortDef::new(
                format!("LED {}", side_name(row)), PortType::Untyped,
            )));
        }
        for label in TOP_LABELS {
            v.push(UiPortDef::from_def(&PortDef::new(
                format!("LED top {}", label), PortType::Untyped,
            )));
        }
        v
    }

    fn ui_outputs(&self) -> Vec<UiPortDef> {
        let mut v = Vec::with_capacity(1 + N_CONTROLS);
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
        if let Some((cid, name, connected)) = snap {
            self.controller_id = cid;
            self.controller_name = name;
            self.connected = connected;
        }

        if self.controller_id == 0 {
            ui.colored_label(Color32::from_gray(120), "No Launchpad bound");
        } else if self.connected {
            ui.colored_label(Color32::from_rgb(80, 200, 100), &self.controller_name);
        } else {
            ui.colored_label(
                Color32::from_rgb(220, 180, 60),
                format!("{} (not connected)", self.controller_name),
            );
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let controllers: Vec<(u32, String)> = {
            let state = self.controllers.lock().unwrap();
            state.iter()
                .filter(|c| matches!(c.kind, InputControllerKind::LaunchpadS { .. }))
                .map(|c| (c.id, c.name.clone()))
                .collect()
        };

        ui.horizontal(|ui| {
            ui.label("Launchpad:");
            let current = controllers.iter()
                .find(|(id, _)| *id == self.controller_id)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| "(none)".to_string());
            egui::ComboBox::from_id_salt(("launchpad_pick", self.id))
                .selected_text(current)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.controller_id == 0, "(none)").clicked() {
                        self.controller_id = 0;
                        self.push_config();
                    }
                    for (id, name) in &controllers {
                        if ui.selectable_label(self.controller_id == *id, name).clicked() {
                            self.controller_id = *id;
                            self.push_config();
                        }
                    }
                });
        });

        if controllers.is_empty() {
            ui.colored_label(
                Color32::from_gray(140),
                "No Launchpad configured. Add one in the Input Controllers window.",
            );
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
