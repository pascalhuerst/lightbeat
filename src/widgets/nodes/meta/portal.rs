use egui::{self, Color32, Ui};
use std::any::Any;

use crate::engine::nodes::meta::portal::{
    InputPortalRxDisplay, InputPortalTxDisplay, OutputPortalRxDisplay, OutputPortalTxDisplay,
    PortalPortDef, SharedPortalRegistry, available_portal_names,
};
use crate::engine::nodes::meta::subgraph::PORT_TYPE_NAMES;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

// Portal-key namespace prefixes so names can overlap between families
// without the peer-halo highlight crossing the streams.
const OUTPUT_KEY: &str = "out:";
const INPUT_KEY: &str = "in:";

fn icon_for(glyph: &str) -> Option<&str> { Some(glyph) }

// ---------------------------------------------------------------------------
// Output Portal TX — defines inputs, broadcasts to any number of RX widgets.
// ---------------------------------------------------------------------------

pub struct OutputPortalTxWidget {
    id: NodeId,
    shared: SharedState,
    name: String,
    pub port_defs: Vec<PortalPortDef>,
}

impl OutputPortalTxWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, name: String::new(), port_defs: Vec::new() }
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

impl NodeWidget for OutputPortalTxWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Output Portal TX" }
    fn title(&self) -> &str { &self.name }
    fn description(&self) -> &'static str {
        "Output Portal — sending end. Defines input ports and a name; any \
         number of Output Portal RX nodes bound to this name mirror these \
         ports as outputs."
    }
    fn accent_icon(&self) -> Option<&'static str> { icon_for(egui_phosphor::regular::BROADCAST) }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.port_defs.iter().map(|p| UiPortDef::from_def(&p.to_port_def())).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 160.0 }
    fn min_content_height(&self) -> f32 { 18.0 }
    fn accent_color(&self) -> Option<Color32> { Some(theme::SEM_WARNING) }
    fn portal_key(&self) -> Option<String> {
        if self.name.is_empty() { None } else { Some(format!("{}{}", OUTPUT_KEY, self.name)) }
    }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let snap = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<OutputPortalTxDisplay>())
                .map(|d| d.name.clone())
        };
        if let Some(n) = snap { self.name = n; }
        // Title bar already shows the name; only surface the empty-name
        // placeholder so the user knows they still need to set it.
        if self.name.is_empty() {
            ui.colored_label(theme::TEXT_DIM, "(unnamed)");
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Name:");
            if ui.text_edit_singleline(&mut self.name).changed() { changed = true; }
        });
        ui.separator();
        ui.label(egui::RichText::new("Ports").strong());
        let mut remove: Option<usize> = None;
        for (i, port) in self.port_defs.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui.text_edit_singleline(&mut port.name).changed() { changed = true; }
                egui::ComboBox::from_id_salt(("op_tx_type", self.id.0, i))
                    .width(90.0)
                    .selected_text(*PORT_TYPE_NAMES.get(port.port_type_idx).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (idx, name) in PORT_TYPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut port.port_type_idx, idx, *name).changed() {
                                changed = true;
                            }
                        }
                    });
                if ui.small_button(egui_phosphor::regular::X).clicked() { remove = Some(i); }
            });
        }
        if let Some(i) = remove { self.port_defs.remove(i); changed = true; }
        if ui.small_button("+ Add Port").clicked() {
            self.port_defs.push(PortalPortDef {
                name: format!("in {}", self.port_defs.len() + 1),
                port_type_idx: 2,
            });
            changed = true;
        }
        if changed { self.push_config(); }
    }

    fn show_port_labels(&self) -> bool { true }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ---------------------------------------------------------------------------
// Output Portal RX — mirrors a TX's ports as outputs. Many per name.
// ---------------------------------------------------------------------------

pub struct OutputPortalRxWidget {
    id: NodeId,
    shared: SharedState,
    registry: SharedPortalRegistry,
    bound_name: String,
    port_defs: Vec<PortalPortDef>,
    connected: bool,
}

impl OutputPortalRxWidget {
    pub fn new(id: NodeId, shared: SharedState, registry: SharedPortalRegistry) -> Self {
        Self { id, shared, registry, bound_name: String::new(), port_defs: Vec::new(), connected: false }
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

impl NodeWidget for OutputPortalRxWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Output Portal RX" }
    fn title(&self) -> &str { &self.bound_name }
    fn description(&self) -> &'static str {
        "Output Portal — receiving end. Mirrors an Output Portal TX's inputs \
         as outputs. Select which TX to follow by name in the inspector."
    }
    fn accent_icon(&self) -> Option<&'static str> { icon_for(egui_phosphor::regular::WIFI_HIGH) }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.port_defs.iter().map(|p| UiPortDef::from_def(&p.to_port_def())).collect()
    }

    fn min_width(&self) -> f32 { 160.0 }
    fn min_content_height(&self) -> f32 { 18.0 }
    fn accent_color(&self) -> Option<Color32> { Some(theme::SEM_WARNING) }
    fn portal_key(&self) -> Option<String> {
        if self.bound_name.is_empty() { None } else { Some(format!("{}{}", OUTPUT_KEY, self.bound_name)) }
    }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let snap = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<OutputPortalRxDisplay>())
                .map(|d| (d.bound_name.clone(), d.port_defs.clone(), d.connected))
        };
        if let Some((bound, ports, connected)) = snap {
            self.bound_name = bound;
            self.port_defs = ports.iter().map(PortalPortDef::from_port_def).collect();
            self.connected = connected;
        }
        // Title bar already shows the bound name; only surface state the
        // title can't convey (unbound placeholder, disconnected warning).
        if self.bound_name.is_empty() {
            ui.colored_label(theme::TEXT_DIM, "(pick an Output Portal TX)");
        } else if !self.connected {
            ui.colored_label(theme::SEM_WARNING, "(not connected)");
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let names = available_portal_names(&self.registry);
        ui.horizontal(|ui| {
            ui.label("Portal:");
            let current = if self.bound_name.is_empty() { "(none)".to_string() } else { self.bound_name.clone() };
            egui::ComboBox::from_id_salt(("op_rx_bind", self.id.0))
                .selected_text(current)
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
        ui.horizontal(|ui| {
            ui.label("or name:");
            let mut s = self.bound_name.clone();
            if ui.text_edit_singleline(&mut s).changed() {
                self.bound_name = s;
                self.push_config();
            }
        });
        if !self.bound_name.is_empty() && !self.connected {
            ui.colored_label(theme::SEM_WARNING, "No Output Portal TX publishing this name yet.");
        }
    }

    fn show_port_labels(&self) -> bool { true }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ---------------------------------------------------------------------------
// Input Portal RX — defines outputs, fans them out locally; single paired TX
// delivers the signal from elsewhere.
// ---------------------------------------------------------------------------

pub struct InputPortalRxWidget {
    id: NodeId,
    shared: SharedState,
    name: String,
    pub port_defs: Vec<PortalPortDef>,
}

impl InputPortalRxWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, name: String::new(), port_defs: Vec::new() }
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

impl NodeWidget for InputPortalRxWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Input Portal RX" }
    fn title(&self) -> &str { &self.name }
    fn description(&self) -> &'static str {
        "Input Portal — receiving end. Defines output ports and a name; the \
         paired Input Portal TX (one per name) provides the values from \
         somewhere else in the graph."
    }
    fn accent_icon(&self) -> Option<&'static str> { icon_for(egui_phosphor::regular::DOWNLOAD_SIMPLE) }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.port_defs.iter().map(|p| UiPortDef::from_def(&p.to_port_def())).collect()
    }

    fn min_width(&self) -> f32 { 160.0 }
    fn min_content_height(&self) -> f32 { 18.0 }
    fn accent_color(&self) -> Option<Color32> { Some(theme::SEM_WARNING) }
    fn portal_key(&self) -> Option<String> {
        if self.name.is_empty() { None } else { Some(format!("{}{}", INPUT_KEY, self.name)) }
    }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let snap = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<InputPortalRxDisplay>())
                .map(|d| d.name.clone())
        };
        if let Some(n) = snap { self.name = n; }
        // Title bar shows the name; only flag when it hasn't been set yet.
        if self.name.is_empty() {
            ui.colored_label(theme::TEXT_DIM, "(unnamed)");
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Name:");
            if ui.text_edit_singleline(&mut self.name).changed() { changed = true; }
        });
        ui.separator();
        ui.label(egui::RichText::new("Ports").strong());
        let mut remove: Option<usize> = None;
        for (i, port) in self.port_defs.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui.text_edit_singleline(&mut port.name).changed() { changed = true; }
                egui::ComboBox::from_id_salt(("ip_rx_type", self.id.0, i))
                    .width(90.0)
                    .selected_text(*PORT_TYPE_NAMES.get(port.port_type_idx).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (idx, name) in PORT_TYPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut port.port_type_idx, idx, *name).changed() {
                                changed = true;
                            }
                        }
                    });
                if ui.small_button(egui_phosphor::regular::X).clicked() { remove = Some(i); }
            });
        }
        if let Some(i) = remove { self.port_defs.remove(i); changed = true; }
        if ui.small_button("+ Add Port").clicked() {
            self.port_defs.push(PortalPortDef {
                name: format!("out {}", self.port_defs.len() + 1),
                port_type_idx: 2,
            });
            changed = true;
        }
        if changed { self.push_config(); }
    }

    fn show_port_labels(&self) -> bool { true }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ---------------------------------------------------------------------------
// Input Portal TX — single sender that feeds the paired RX.
// ---------------------------------------------------------------------------

pub struct InputPortalTxWidget {
    id: NodeId,
    shared: SharedState,
    registry: SharedPortalRegistry,
    bound_name: String,
    port_defs: Vec<PortalPortDef>,
    connected: bool,
}

impl InputPortalTxWidget {
    pub fn new(id: NodeId, shared: SharedState, registry: SharedPortalRegistry) -> Self {
        Self { id, shared, registry, bound_name: String::new(), port_defs: Vec::new(), connected: false }
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

impl NodeWidget for InputPortalTxWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Input Portal TX" }
    fn title(&self) -> &str { &self.bound_name }
    fn description(&self) -> &'static str {
        "Input Portal — sending end. Mirrors the paired Input Portal RX's \
         output ports as inputs, and feeds whatever you wire into them \
         into that RX."
    }
    fn accent_icon(&self) -> Option<&'static str> { icon_for(egui_phosphor::regular::UPLOAD_SIMPLE) }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.port_defs.iter().map(|p| UiPortDef::from_def(&p.to_port_def())).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 160.0 }
    fn min_content_height(&self) -> f32 { 18.0 }
    fn accent_color(&self) -> Option<Color32> { Some(theme::SEM_WARNING) }
    fn portal_key(&self) -> Option<String> {
        if self.bound_name.is_empty() { None } else { Some(format!("{}{}", INPUT_KEY, self.bound_name)) }
    }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let snap = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<InputPortalTxDisplay>())
                .map(|d| (d.bound_name.clone(), d.port_defs.clone(), d.connected))
        };
        if let Some((bound, ports, connected)) = snap {
            self.bound_name = bound;
            self.port_defs = ports.iter().map(PortalPortDef::from_port_def).collect();
            self.connected = connected;
        }
        // Title bar shows the bound name; only flag edge states here.
        if self.bound_name.is_empty() {
            ui.colored_label(theme::TEXT_DIM, "(pick an Input Portal RX)");
        } else if !self.connected {
            ui.colored_label(theme::SEM_WARNING, "(not connected)");
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let names = available_portal_names(&self.registry);
        ui.horizontal(|ui| {
            ui.label("Portal:");
            let current = if self.bound_name.is_empty() { "(none)".to_string() } else { self.bound_name.clone() };
            egui::ComboBox::from_id_salt(("ip_tx_bind", self.id.0))
                .selected_text(current)
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
        ui.horizontal(|ui| {
            ui.label("or name:");
            let mut s = self.bound_name.clone();
            if ui.text_edit_singleline(&mut s).changed() {
                self.bound_name = s;
                self.push_config();
            }
        });
        if !self.bound_name.is_empty() && !self.connected {
            ui.colored_label(theme::SEM_WARNING, "No Input Portal RX defining this name yet.");
        }
    }

    fn show_port_labels(&self) -> bool { true }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
