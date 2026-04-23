use std::any::Any;
use egui::{self, Color32, Ui};

use crate::engine::nodes::meta::portal::{
    available_portal_names, PortalInDisplay, PortalOutDisplay, PortalPortDef, SharedPortalRegistry,
};
use crate::engine::nodes::meta::subgraph::PORT_TYPE_NAMES;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

// ---------------------------------------------------------------------------
// Portal In
// ---------------------------------------------------------------------------

pub struct PortalInWidget {
    id: NodeId,
    shared: SharedState,
    name: String,
    pub port_defs: Vec<PortalPortDef>,
    duplicate_name: bool,
}

impl PortalInWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id, shared,
            name: String::new(),
            port_defs: Vec::new(),
            duplicate_name: false,
        }
    }

    fn push_config(&self) {
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({
            "name": self.name,
            "ports": self.port_defs,
        }));
    }

    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
            self.name = n.to_string();
        }
        if let Some(arr) = data.get("ports").and_then(|v| v.as_array()) {
            self.port_defs = arr.iter()
                .filter_map(|v| serde_json::from_value::<PortalPortDef>(v.clone()).ok())
                .collect();
        }
    }
}

impl NodeWidget for PortalInWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Portal In" }
    fn title(&self) -> &str {
        if self.name.is_empty() { "Portal In" } else { self.name.as_str() }
    }
    fn description(&self) -> &'static str {
        "Publishes its inputs under a name. Any Portal Out bound to the same name mirrors these ports as outputs — wireless cables for tidy graphs."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.port_defs.iter()
            .map(|p| UiPortDef::from_def(&p.to_port_def()))
            .collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 140.0 }
    fn min_content_height(&self) -> f32 { 18.0 }
    // Amber accent — matches the portal-peer halo so the colour identity
    // reads "these nodes are a linked wireless pair" whether or not a
    // peer is currently selected.
    fn accent_color(&self) -> Option<Color32> { Some(theme::ACCENT_PORTAL) }
    fn portal_key(&self) -> Option<String> {
        if self.name.is_empty() { None } else { Some(self.name.clone()) }
    }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let snap = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<PortalInDisplay>())
                .map(|d| (d.name.clone(), d.duplicate_name))
        };
        if let Some((n, dup)) = snap {
            self.name = n;
            self.duplicate_name = dup;
        }

        if self.name.is_empty() {
            ui.colored_label(theme::TEXT_DIM, "(unnamed)");
        } else if self.duplicate_name {
            ui.colored_label(
                theme::STATUS_WARNING,
                format!("{} — duplicate!", self.name),
            );
        } else {
            ui.colored_label(theme::TEXT_SUBTLE, &self.name);
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Name:");
            if ui.text_edit_singleline(&mut self.name).changed() {
                changed = true;
            }
        });

        ui.separator();
        ui.label(egui::RichText::new("Ports").strong());

        let mut remove: Option<usize> = None;
        for (i, port) in self.port_defs.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui.text_edit_singleline(&mut port.name).changed() {
                    changed = true;
                }
                egui::ComboBox::from_id_salt(("portal_in_type", self.id.0, i))
                    .width(90.0)
                    .selected_text(*PORT_TYPE_NAMES.get(port.port_type_idx).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (idx, name) in PORT_TYPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut port.port_type_idx, idx, *name).changed() {
                                changed = true;
                            }
                        }
                    });
                if ui.small_button(egui_phosphor::regular::X).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.port_defs.remove(i);
            changed = true;
        }
        if ui.small_button("+ Add Port").clicked() {
            self.port_defs.push(PortalPortDef {
                name: format!("in {}", self.port_defs.len() + 1),
                port_type_idx: 2, // Untyped default
            });
            changed = true;
        }

        if changed {
            self.push_config();
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ---------------------------------------------------------------------------
// Portal Out
// ---------------------------------------------------------------------------

pub struct PortalOutWidget {
    id: NodeId,
    shared: SharedState,
    registry: SharedPortalRegistry,
    bound_name: String,
    port_defs: Vec<PortalPortDef>,
    connected: bool,
}

impl PortalOutWidget {
    pub fn new(id: NodeId, shared: SharedState, registry: SharedPortalRegistry) -> Self {
        Self {
            id, shared, registry,
            bound_name: String::new(),
            port_defs: Vec::new(),
            connected: false,
        }
    }

    fn push_config(&self) {
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({
            "bound_name": self.bound_name,
            "ports": self.port_defs,
        }));
    }

    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("bound_name").and_then(|v| v.as_str()) {
            self.bound_name = n.to_string();
        }
        if let Some(arr) = data.get("ports").and_then(|v| v.as_array()) {
            self.port_defs = arr.iter()
                .filter_map(|v| serde_json::from_value::<PortalPortDef>(v.clone()).ok())
                .collect();
        }
    }
}

impl NodeWidget for PortalOutWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Portal Out" }
    fn title(&self) -> &str {
        if self.bound_name.is_empty() { "Portal Out" } else { self.bound_name.as_str() }
    }
    fn description(&self) -> &'static str {
        "Mirrors a Portal In's inputs as outputs. Select which Portal In to follow by name in the inspector."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.port_defs.iter()
            .map(|p| UiPortDef::from_def(&p.to_port_def()))
            .collect()
    }

    fn min_width(&self) -> f32 { 140.0 }
    fn min_content_height(&self) -> f32 { 18.0 }
    // Amber accent — matches the portal-peer halo so the colour identity
    // reads "these nodes are a linked wireless pair" whether or not a
    // peer is currently selected.
    fn accent_color(&self) -> Option<Color32> { Some(theme::ACCENT_PORTAL) }
    fn portal_key(&self) -> Option<String> {
        if self.bound_name.is_empty() { None } else { Some(self.bound_name.clone()) }
    }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let snap = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<PortalOutDisplay>())
                .map(|d| (d.bound_name.clone(), d.port_defs.clone(), d.connected))
        };
        if let Some((bound, ports, connected)) = snap {
            self.bound_name = bound;
            // Mirror port defs from the engine so the widget's ui_outputs
            // matches the Portal In's current layout.
            self.port_defs = ports.iter().map(PortalPortDef::from_port_def).collect();
            self.connected = connected;
        }

        if self.bound_name.is_empty() {
            ui.colored_label(theme::TEXT_DIM, "(pick a Portal In)");
        } else if self.connected {
            ui.colored_label(theme::TEXT_SUBTLE, &self.bound_name);
        } else {
            ui.colored_label(
                theme::STATUS_WARNING,
                format!("{} (not connected)", self.bound_name),
            );
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let names = available_portal_names(&self.registry);

        ui.horizontal(|ui| {
            ui.label("Portal:");
            let current_label = if self.bound_name.is_empty() {
                "(none)".to_string()
            } else {
                self.bound_name.clone()
            };
            egui::ComboBox::from_id_salt(("portal_out_bind", self.id.0))
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.bound_name.is_empty(), "(none)").clicked() {
                        self.bound_name.clear();
                        self.push_config();
                    }
                    for name in &names {
                        if ui.selectable_label(self.bound_name == *name, name).clicked() {
                            self.bound_name = name.clone();
                            self.push_config();
                        }
                    }
                });
        });

        // Allow typing a name that doesn't exist yet — handy for preconfiguring
        // a Portal Out before its Portal In is created (or after it was
        // deleted and will be recreated).
        ui.horizontal(|ui| {
            ui.label("or name:");
            let mut s = self.bound_name.clone();
            if ui.text_edit_singleline(&mut s).changed() {
                self.bound_name = s;
                self.push_config();
            }
        });

        if !self.bound_name.is_empty() && !self.connected {
            ui.colored_label(
                theme::STATUS_WARNING,
                "No Portal In publishing this name yet.",
            );
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
