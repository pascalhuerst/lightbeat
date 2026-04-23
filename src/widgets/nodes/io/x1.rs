use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::io::x1::X1Display;
use crate::engine::types::*;
use crate::input_controller::x1::{
    ALL_BUTTONS, ALL_ENCODERS, ALL_POTS,
    button_led_index, button_name, encoder_name, pot_name,
};
use crate::input_controller::{InputControllerKind, SharedControllers};
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct X1Widget {
    id: NodeId,
    shared: SharedState,
    controller_id: u32,
    controllers: SharedControllers,
    connected: bool,
    controller_name: String,
}

impl X1Widget {
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

impl NodeWidget for X1Widget {
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
        let mut v = Vec::with_capacity(1 + ALL_BUTTONS.len() + ALL_ENCODERS.len() + ALL_POTS.len());
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
        if let Some((cid, name, connected)) = snap {
            self.controller_id = cid;
            self.controller_name = name;
            self.connected = connected;
        }

        if self.controller_id == 0 {
            ui.colored_label(Color32::from_gray(120), "No X1 bound");
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
        // Only show X1 controllers.
        let controllers: Vec<(u32, String)> = {
            let state = self.controllers.lock().unwrap();
            state.iter()
                .filter(|c| matches!(c.kind, InputControllerKind::X1))
                .map(|c| (c.id, c.name.clone()))
                .collect()
        };

        ui.horizontal(|ui| {
            ui.label("X1:");
            let current = controllers.iter()
                .find(|(id, _)| *id == self.controller_id)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| "(none)".to_string());
            egui::ComboBox::from_id_salt(("x1_pick", self.id))
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
                "No X1 configured. Add one in the Input Controllers window.",
            );
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
