use std::any::Any;
use egui::{self, Color32, Ui};

use crate::engine::nodes::meta::subgraph::{SubgraphDisplay, SubgraphPortDef, port_type_to_idx, BRIDGE_IN_NODE_ID, BRIDGE_OUT_NODE_ID, PORT_TYPE_NAMES};
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct SubgraphWidget {
    id: NodeId,
    shared: SharedState,
    name: String,
    pub input_defs: Vec<SubgraphPortDef>,
    pub output_defs: Vec<SubgraphPortDef>,
    /// Set to true to signal the app to open this subgraph for editing.
    pub wants_open: bool,
}

impl SubgraphWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id, shared,
            name: "Subgraph".to_string(),
            input_defs: Vec::new(),
            output_defs: Vec::new(),
            wants_open: false,
        }
    }

    pub fn push_config(&self) {
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({
            "name": self.name,
            "inputs": self.input_defs,
            "outputs": self.output_defs,
        }));
    }
}

impl NodeWidget for SubgraphWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Subgraph" }
    fn title(&self) -> &str { &self.name }
    fn description(&self) -> &'static str { "Encapsulates an inner graph with custom input/output ports; double-click to enter." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.input_defs.iter().map(|p| UiPortDef::from_def(&p.to_port_def())).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.output_defs.iter().map(|p| UiPortDef::from_def(&p.to_port_def())).collect()
    }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 16.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn title_color(&self) -> Option<Color32> {
        Some(Color32::from_rgb(18, 18, 28))
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<SubgraphDisplay>());

        let inner_count = if let Some(d) = display {
            d.inner_node_count
        } else { 0 };
        drop(shared);

        ui.horizontal(|ui| {
            ui.colored_label(Color32::from_gray(100), format!("{} nodes", inner_count));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button(egui_phosphor::regular::ARROW_SQUARE_OUT).on_hover_text("Open subgraph").clicked() {
                    self.wants_open = true;
                }
            });
        });
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        // Name.
        ui.horizontal(|ui| {
            ui.label("Name:");
            if ui.text_edit_singleline(&mut self.name).changed() {
                self.push_config();
            }
        });

        ui.separator();

        // Input ports.
        ui.label(egui::RichText::new("Inputs").strong());
        let mut input_changed = false;
        let mut remove_input = None;
        for (i, port) in self.input_defs.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui.text_edit_singleline(&mut port.name).changed() { input_changed = true; }
                egui::ComboBox::from_id_salt(("sub_in_type", self.id.0, i))
                    .width(80.0)
                    .selected_text(*PORT_TYPE_NAMES.get(port.port_type_idx).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (idx, name) in PORT_TYPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut port.port_type_idx, idx, *name).changed() {
                                input_changed = true;
                            }
                        }
                    });
                if ui.small_button(egui_phosphor::regular::X).clicked() { remove_input = Some(i); }
            });
        }
        if let Some(i) = remove_input { self.input_defs.remove(i); input_changed = true; }
        if ui.small_button("+ Add Input").clicked() {
            self.input_defs.push(SubgraphPortDef { name: format!("in {}", self.input_defs.len()), port_type_idx: 2 });
            input_changed = true;
        }

        ui.separator();

        // Output ports.
        ui.label(egui::RichText::new("Outputs").strong());
        let mut output_changed = false;
        let mut remove_output = None;
        for (i, port) in self.output_defs.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui.text_edit_singleline(&mut port.name).changed() { output_changed = true; }
                egui::ComboBox::from_id_salt(("sub_out_type", self.id.0, i))
                    .width(80.0)
                    .selected_text(*PORT_TYPE_NAMES.get(port.port_type_idx).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (idx, name) in PORT_TYPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut port.port_type_idx, idx, *name).changed() {
                                output_changed = true;
                            }
                        }
                    });
                if ui.small_button(egui_phosphor::regular::X).clicked() { remove_output = Some(i); }
            });
        }
        if let Some(i) = remove_output { self.output_defs.remove(i); output_changed = true; }
        if ui.small_button("+ Add Output").clicked() {
            self.output_defs.push(SubgraphPortDef { name: format!("out {}", self.output_defs.len()), port_type_idx: 2 });
            output_changed = true;
        }

        if input_changed || output_changed {
            self.push_config();
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ---------------------------------------------------------------------------
// Bridge pseudo-nodes: visible inside subgraph as "Graph Input" / "Graph Output"
// ---------------------------------------------------------------------------

/// Pseudo-node representing the subgraph's input ports inside the inner graph.
/// Outputs correspond to the subgraph's external inputs (data flows in).
pub struct GraphInputWidget {
    shared: SharedState,
    port_defs: Vec<SubgraphPortDef>,
}

impl GraphInputWidget {
    pub fn new(port_defs: Vec<SubgraphPortDef>) -> Self {
        let out_channels = port_defs.iter()
            .map(|p| p.to_port_def().port_type.channel_count())
            .sum();
        Self {
            shared: new_shared_state(0, out_channels),
            port_defs,
        }
    }

    pub fn update_ports(&mut self, port_defs: Vec<SubgraphPortDef>) {
        self.port_defs = port_defs;
    }
}

impl NodeWidget for GraphInputWidget {
    fn node_id(&self) -> NodeId { BRIDGE_IN_NODE_ID }
    fn type_name(&self) -> &'static str { "GraphInput" }
    fn title(&self) -> &str { "Graph Input" }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.port_defs.iter().map(|p| UiPortDef::from_def(&p.to_port_def())).collect()
    }

    fn min_width(&self) -> f32 { 110.0 }
    fn min_content_height(&self) -> f32 { 0.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, _ui: &mut Ui, _zoom: f32) {}

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

/// Pseudo-node representing the subgraph's output ports inside the inner graph.
/// Inputs correspond to the subgraph's external outputs (data flows out).
pub struct GraphOutputWidget {
    shared: SharedState,
    port_defs: Vec<SubgraphPortDef>,
}

impl GraphOutputWidget {
    pub fn new(port_defs: Vec<SubgraphPortDef>) -> Self {
        let in_channels = port_defs.iter()
            .map(|p| p.to_port_def().port_type.channel_count())
            .sum();
        Self {
            shared: new_shared_state(in_channels, 0),
            port_defs,
        }
    }

    pub fn update_ports(&mut self, port_defs: Vec<SubgraphPortDef>) {
        self.port_defs = port_defs;
    }
}

impl NodeWidget for GraphOutputWidget {
    fn node_id(&self) -> NodeId { BRIDGE_OUT_NODE_ID }
    fn type_name(&self) -> &'static str { "GraphOutput" }
    fn title(&self) -> &str { "Graph Output" }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.port_defs.iter().map(|p| UiPortDef::from_def(&p.to_port_def())).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 110.0 }
    fn min_content_height(&self) -> f32 { 0.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, _ui: &mut Ui, _zoom: f32) {}

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
