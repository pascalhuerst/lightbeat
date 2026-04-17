use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::io::input_controller::InputControllerDisplay;
use crate::engine::types::*;
use crate::input_controller::SharedControllers;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct InputControllerWidget {
    id: NodeId,
    shared: SharedState,
    /// Currently selected controller id (0 = none).
    controller_id: u32,
    /// Mirror of engine display: per-output (name, type, value).
    outputs: Vec<(String, PortType, f32)>,
    /// Pointer to the live controller registry so the inspector dropdown
    /// can list available controllers.
    controllers: SharedControllers,
}

impl InputControllerWidget {
    pub fn new(id: NodeId, shared: SharedState, controllers: SharedControllers) -> Self {
        Self {
            id,
            shared,
            controller_id: 0,
            outputs: Vec::new(),
            controllers,
        }
    }

    fn push_config(&self) {
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({
            "controller_id": self.controller_id,
        }));
    }

    /// Pre-populate `controller_id` and the output-port list from save_data
    /// + the live controllers registry, so connections survive the first
    /// frame's `cleanup_stale_connections` after a project load.
    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Some(id) = data.get("controller_id").and_then(|v| v.as_u64()) {
            self.controller_id = id as u32;
        }
        if self.controller_id == 0 { return; }
        let state = self.controllers.lock().unwrap();
        if let Some(c) = state.iter().find(|c| c.id == self.controller_id) {
            // The "any change" trigger port is at index 0 in the engine
            // node's output layout, but the InputControllerProcessNode does
            // not actually expose it (only Beat / button-style triggers do).
            // Match what `InputControllerProcessNode::process` builds: one
            // port per learned input, typed by source.
            self.outputs = c.inputs.iter().map(|i| {
                let ty = if i.source.is_binary() { PortType::Logic } else { PortType::Untyped };
                (i.name.clone(), ty, 0.0)
            }).collect();
        }
    }
}

impl NodeWidget for InputControllerWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Input Controller" }
    fn title(&self) -> &str { "Input Controller" }
    fn description(&self) -> &'static str {
        "Outputs the current value of each learned input on the selected controller."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.outputs.iter().map(|(name, ty, _)| {
            UiPortDef::from_def(&PortDef::new(name.clone(), *ty))
        }).collect()
    }

    fn min_width(&self) -> f32 { 160.0 }
    fn min_content_height(&self) -> f32 { 24.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let snapshot: Option<(u32, String, Vec<(String, PortType, f32)>)> = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<InputControllerDisplay>())
                .map(|d| (d.controller_id, d.controller_name.clone(), d.outputs.clone()))
        };
        if let Some((cid, _name, outs)) = snapshot {
            self.controller_id = cid;
            self.outputs = outs;
        }

        if self.controller_id == 0 {
            ui.colored_label(Color32::from_gray(120), "No controller selected");
        } else if self.outputs.is_empty() {
            ui.colored_label(Color32::from_gray(120), "Controller has no inputs");
        } else {
            ui.colored_label(
                Color32::from_gray(140),
                format!("{} input(s)", self.outputs.len()),
            );
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let controllers: Vec<(u32, String)> = {
            let state = self.controllers.lock().unwrap();
            state.iter().map(|c| (c.id, c.name.clone())).collect()
        };

        ui.horizontal(|ui| {
            ui.label("Controller:");
            let current_label = controllers.iter()
                .find(|(id, _)| *id == self.controller_id)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| "(none)".to_string());
            egui::ComboBox::from_id_salt(("ic_pick", self.id))
                .selected_text(current_label)
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

        if !self.outputs.is_empty() {
            ui.separator();
            ui.label(egui::RichText::new("Live Values").strong());
            for (name, _ty, value) in &self.outputs {
                ui.horizontal(|ui| {
                    ui.label(name);
                    ui.colored_label(Color32::from_gray(180), format!("{:.2}", value));
                });
            }
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
