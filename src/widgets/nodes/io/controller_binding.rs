use egui::{self, Ui};

use crate::engine::types::{NodeId, SharedState};
use crate::input_controller::{InputControllerKind, SharedControllers};
use crate::theme;

/// What distinguishes one hardware node from another in the binding UI.
pub struct ControllerSpec {
    /// Inspector label and the name used in the "No ... bound" texts.
    pub label: &'static str,
    /// Salt for the picker's combo box id.
    pub id_salt: &'static str,
    pub is_kind: fn(&InputControllerKind) -> bool,
}

/// The controller a hardware node widget is bound to, as the UI sees it.
pub struct ControllerBinding {
    pub controller_id: u32,
    controllers: SharedControllers,
    connected: bool,
    controller_name: String,
    spec: &'static ControllerSpec,
}

impl ControllerBinding {
    pub fn new(controllers: SharedControllers, spec: &'static ControllerSpec) -> Self {
        Self { controller_id: 0, controllers, connected: false, controller_name: String::new(), spec }
    }

    /// Takes the engine's view (id, name, connected) when it has published one.
    pub fn sync_from(&mut self, snap: Option<(u32, String, bool)>) {
        if let Some((id, name, connected)) = snap {
            self.controller_id = id;
            self.controller_name = name;
            self.connected = connected;
        }
    }

    pub fn restore_from_save(&mut self, data: Option<&serde_json::Value>) {
        let Some(data) = data else { return };
        if let Some(id) = data.get("controller_id").and_then(|v| v.as_u64()) {
            self.controller_id = id as u32;
        }
    }

    fn push_config(&self, shared: &SharedState) {
        shared.lock().pending_config = Some(serde_json::json!({
            "controller_id": self.controller_id,
        }));
    }

    pub fn show_status(&self, ui: &mut Ui) {
        if self.controller_id == 0 {
            ui.colored_label(theme::TEXT_DIM, format!("No {} bound", self.spec.label));
        } else if self.connected {
            ui.colored_label(theme::SEM_SUCCESS, &self.controller_name);
        } else {
            ui.colored_label(
                theme::SEM_WARNING,
                format!("{} (not connected)", self.controller_name),
            );
        }
    }

    /// Lists only controllers of this node's kind; any other would be useless here.
    pub fn show_picker(&mut self, ui: &mut Ui, node_id: NodeId, shared: &SharedState) {
        let controllers: Vec<(u32, String)> = {
            let state = self.controllers.lock();
            state.iter()
                .filter(|c| (self.spec.is_kind)(&c.kind))
                .map(|c| (c.id, c.name.clone()))
                .collect()
        };

        ui.horizontal(|ui| {
            ui.label(self.spec.label);
            let current = controllers.iter()
                .find(|(id, _)| *id == self.controller_id)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| "(none)".to_string());
            egui::ComboBox::from_id_salt((self.spec.id_salt, node_id))
                .selected_text(current)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.controller_id == 0, "(none)").clicked() {
                        self.controller_id = 0;
                        self.push_config(shared);
                    }
                    for (id, name) in &controllers {
                        if ui.selectable_label(self.controller_id == *id, name).clicked() {
                            self.controller_id = *id;
                            self.push_config(shared);
                        }
                    }
                });
        });

        if controllers.is_empty() {
            ui.colored_label(
                theme::TEXT_DIM,
                format!("No {} configured. Add one in the Input Controllers window.", self.spec.label),
            );
        }
    }
}
