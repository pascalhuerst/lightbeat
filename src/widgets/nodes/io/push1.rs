use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::io::push1::Push1Display;
use crate::engine::types::*;
use crate::input_controller::{InputControllerKind, SharedControllers};
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct Push1Widget {
    id: NodeId,
    shared: SharedState,
    controller_id: u32,
    controllers: SharedControllers,
    connected: bool,
    controller_name: String,
}

impl Push1Widget {
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

impl NodeWidget for Push1Widget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Push 1" }
    fn title(&self) -> &str { "Push 1" }
    fn description(&self) -> &'static str {
        "Ableton Push 1 dedicated node. Groups the device's controls into \
         (trigger, address, value) channels — e.g. one `pad trig` pulse per \
         pad event with x/y/velocity describing which pad. Feedback inputs \
         let you light pad / button LEDs by pulsing `set pad trig` with the \
         latched coords and color."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        // Mirror the engine port layout.
        let defs = [
            ("set pad trig", PortType::Logic),
            ("set pad x", PortType::Untyped),
            ("set pad y", PortType::Untyped),
            ("set pad color", PortType::Untyped),
            ("set btn top trig", PortType::Logic),
            ("set btn top col", PortType::Untyped),
            ("set btn top color", PortType::Untyped),
            ("set btn bot trig", PortType::Logic),
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
            ("pad trig", PortType::Logic),
            ("pad x", PortType::Untyped),
            ("pad y", PortType::Untyped),
            ("pad vel", PortType::Untyped),
            ("btn top trig", PortType::Logic),
            ("btn top col", PortType::Untyped),
            ("btn top val", PortType::Logic),
            ("btn bot trig", PortType::Logic),
            ("btn bot col", PortType::Untyped),
            ("btn bot val", PortType::Logic),
            ("enc trig", PortType::Logic),
            ("enc idx", PortType::Untyped),
            ("enc val", PortType::Untyped),
            ("slider", PortType::Untyped),
        ];
        let mut v: Vec<UiPortDef> = defs.iter()
            .map(|(n, t)| UiPortDef::from_def(&PortDef::new((*n).to_string(), *t)))
            .collect();
        for name in NAMED {
            v.push(UiPortDef::from_def(&PortDef::new((*name).to_string(), PortType::Logic)));
        }
        v
    }

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
        if let Some((cid, name, connected)) = snap {
            self.controller_id = cid;
            self.controller_name = name;
            self.connected = connected;
        }

        if self.controller_id == 0 {
            ui.colored_label(Color32::from_gray(120), "No Push 1 bound");
        } else if self.connected {
            ui.colored_label(Color32::from_rgb(80, 200, 100), &self.controller_name);
        } else {
            ui.colored_label(Color32::from_rgb(220, 180, 60), format!("{} (not connected)", self.controller_name));
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        // Only show Push 1 controllers — this node is useless with any other.
        let controllers: Vec<(u32, String)> = {
            let state = self.controllers.lock().unwrap();
            state.iter()
                .filter(|c| matches!(c.kind, InputControllerKind::Push1 { .. }))
                .map(|c| (c.id, c.name.clone()))
                .collect()
        };

        ui.horizontal(|ui| {
            ui.label("Push 1:");
            let current = controllers.iter()
                .find(|(id, _)| *id == self.controller_id)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| "(none)".to_string());
            egui::ComboBox::from_id_salt(("push1_pick", self.id))
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
                "No Push 1 configured. Add one in the Input Controllers window.",
            );
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

const NAMED: &[&str] = &[
    "Play", "Record", "Stop", "Shift", "Select",
    "Up", "Down", "Left", "Right", "Octave Up", "Octave Down",
];
