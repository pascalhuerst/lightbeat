use egui::{self, Color32, Ui};
use std::any::Any;

use crate::engine::nodes::meta::portal::{
    PortalPortDef, PortalReceiveDisplay, PortalSendDisplay, SharedPortalRegistry,
    available_portal_names,
};
use crate::engine::nodes::meta::subgraph::PORT_TYPE_NAMES;
use crate::engine::types::*;
use crate::widgets::setup_ui;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

/// Index into `PORT_TYPE_NAMES` of the plain number type, which a new port starts as.
const DEFAULT_PORT_TYPE: usize = 2;

// ---------------------------------------------------------------------------
// Portal Send: has inputs, publishes them under its name.
// ---------------------------------------------------------------------------

pub struct PortalSendWidget {
    id: NodeId,
    shared: SharedState,
    name: String,
    pub port_defs: Vec<PortalPortDef>,
    conflict: bool,
    /// Set when a port is removed from the middle of the list, for the graph to
    /// move the wires after it.
    removed: Option<usize>,
}

impl PortalSendWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        // A Send starts with one port so there is something to wire at once.
        let w = Self {
            id,
            shared,
            name: String::new(),
            port_defs: vec![PortalPortDef { name: "in 1".into(), port_type_idx: DEFAULT_PORT_TYPE }],
            conflict: false,
            removed: None,
        };
        w.push_config();
        w
    }

    fn push_config(&self) {
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({
            "name": self.name,
            "ports": self.port_defs,
        }));
    }

    pub fn remove_port(&mut self, i: usize) {
        self.port_defs.remove(i);
        self.removed = Some(i);
    }

    pub fn name(&self) -> &str { &self.name }

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

impl NodeWidget for PortalSendWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Portal Send" }
    fn title(&self) -> &str { &self.name }
    fn description(&self) -> &'static str {
        "Sends signals to any number of Portal Receive nodes with the same name, \
         wherever they are in the graph. Wire into the inputs; add, rename or \
         retype inputs in the inspector. The name is unique to this Send."
    }
    fn accent_icon(&self) -> Option<&'static str> { Some(egui_phosphor::regular::BROADCAST) }
    fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
        self.push_config();
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.port_defs.iter().map(|p| UiPortDef::from_def(&p.to_port_def())).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }
    fn take_removed_port(&mut self) -> Option<(PortDir, usize)> {
        self.removed.take().map(|i| (PortDir::Input, i))
    }

    fn min_width(&self) -> f32 { 160.0 }
    fn min_content_height(&self) -> f32 { 18.0 }
    fn accent_color(&self) -> Option<Color32> { Some(theme::SEM_WARNING) }
    fn link_key(&self) -> Option<String> {
        if self.name.is_empty() { None } else { Some(format!("portal:{}", self.name)) }
    }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let snap = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<PortalSendDisplay>())
                .map(|d| (d.name.clone(), d.conflict))
        };
        if let Some((n, conflict)) = snap {
            self.name = n;
            self.conflict = conflict;
        }
        if self.name.is_empty() {
            ui.colored_label(theme::SEM_WARNING, "(needs a name)");
        } else if self.conflict {
            ui.colored_label(theme::SEM_DANGER, "(name already used)");
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Name");
            if ui.text_edit_singleline(&mut self.name).changed() { changed = true; }
        });
        if self.name.is_empty() {
            ui.colored_label(theme::SEM_WARNING, "Give it a name: nothing is sent without one.");
        } else if self.conflict {
            ui.colored_label(theme::SEM_DANGER, "Another Portal Send already uses this name.");
        }
        ui.separator();
        ui.label(egui::RichText::new("Ports").strong());
        let mut remove: Option<usize> = None;
        for (i, port) in self.port_defs.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui.text_edit_singleline(&mut port.name).changed() { changed = true; }
                egui::ComboBox::from_id_salt(("portal_send_type", self.id.0, i))
                    .width(90.0)
                    .selected_text(*PORT_TYPE_NAMES.get(port.port_type_idx).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (idx, name) in PORT_TYPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut port.port_type_idx, idx, *name).changed() {
                                changed = true;
                            }
                        }
                    });
                if setup_ui::delete_button(ui, "Delete this port") { remove = Some(i); }
            });
        }
        if let Some(i) = remove {
            self.remove_port(i);
            changed = true;
        }
        if setup_ui::add_button(ui, "Add port") {
            self.port_defs.push(PortalPortDef {
                name: format!("in {}", self.port_defs.len() + 1),
                port_type_idx: DEFAULT_PORT_TYPE,
            });
            changed = true;
        }
        if changed { self.push_config(); }
    }

    fn show_port_labels(&self) -> bool { true }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ---------------------------------------------------------------------------
// Portal Receive: shows the ports of a Send as outputs. Any number per name.
// ---------------------------------------------------------------------------

pub struct PortalReceiveWidget {
    id: NodeId,
    shared: SharedState,
    registry: SharedPortalRegistry,
    bound_name: String,
    port_defs: Vec<PortalPortDef>,
    connected: bool,
}

impl PortalReceiveWidget {
    pub fn new(id: NodeId, shared: SharedState, registry: SharedPortalRegistry) -> Self {
        Self { id, shared, registry, bound_name: String::new(), port_defs: Vec::new(), connected: false }
    }

    pub fn bound_name(&self) -> &str { &self.bound_name }

    /// The Send this is bound to lost port `i`. The list normally follows from
    /// the engine a moment later; doing it now lets the graph move the wires
    /// and check their types against the new list straight away.
    pub fn drop_port(&mut self, i: usize) {
        if i < self.port_defs.len() {
            self.port_defs.remove(i);
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

impl NodeWidget for PortalReceiveWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Portal Receive" }
    fn title(&self) -> &str { &self.bound_name }
    fn description(&self) -> &'static str {
        "Receives what a Portal Send with the same name is sent, as outputs. \
         Place as many as you like. Pick the Send in the inspector."
    }
    fn accent_icon(&self) -> Option<&'static str> { Some(egui_phosphor::regular::WIFI_HIGH) }
    fn set_name(&mut self, name: &str) {
        self.bound_name = name.to_string();
        self.push_config();
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.port_defs.iter().map(|p| UiPortDef::from_def(&p.to_port_def())).collect()
    }

    fn min_width(&self) -> f32 { 160.0 }
    fn min_content_height(&self) -> f32 { 18.0 }
    fn accent_color(&self) -> Option<Color32> { Some(theme::SEM_WARNING) }
    fn link_key(&self) -> Option<String> {
        if self.bound_name.is_empty() { None } else { Some(format!("portal:{}", self.bound_name)) }
    }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let snap = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<PortalReceiveDisplay>())
                .map(|d| (d.bound_name.clone(), d.port_defs.clone(), d.connected))
        };
        if let Some((bound, ports, connected)) = snap {
            self.bound_name = bound;
            self.port_defs = ports.iter().map(PortalPortDef::from_port_def).collect();
            self.connected = connected;
        }
        if self.bound_name.is_empty() {
            ui.colored_label(theme::SEM_WARNING, "(pick a Portal Send)");
        } else if !self.connected {
            ui.colored_label(theme::SEM_WARNING, "(no Portal Send with this name)");
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let names = available_portal_names(&self.registry);
        ui.horizontal(|ui| {
            ui.label("Portal Send");
            let current = if self.bound_name.is_empty() { "(none)".to_string() } else { self.bound_name.clone() };
            egui::ComboBox::from_id_salt(("portal_receive_bind", self.id.0))
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for name in &names {
                        if ui.selectable_label(self.bound_name == *name, name).clicked() {
                            self.bound_name = name.clone();
                            self.push_config();
                        }
                    }
                });
        });
        if self.bound_name.is_empty() {
            ui.colored_label(theme::SEM_WARNING, "Pick the Portal Send this receives from.");
        } else if !self.connected {
            ui.colored_label(theme::SEM_WARNING, "No Portal Send publishes this name.");
        }
    }

    fn show_port_labels(&self) -> bool { true }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
