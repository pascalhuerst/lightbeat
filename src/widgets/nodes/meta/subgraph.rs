use egui::{self, Color32, Ui};
use std::any::Any;

use crate::engine::nodes::meta::subgraph::{
    BRIDGE_IN_NODE_ID, BRIDGE_OUT_NODE_ID, InnerValueDisplay, PORT_TYPE_NAMES, SubgraphDisplay,
    SubgraphPortDef,
};
use crate::engine::types::*;
use crate::theme;
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
    /// When true, the subgraph is treated as an opaque "macro instance":
    /// users can't navigate into it, the Open button is hidden, and the
    /// title bar shows a lock glyph. Set when instantiating from the macro
    /// library; cleared via the right-click "Embed" action.
    pub locked: bool,
    /// Macro metadata surfaced in the inspector when `locked` is true.
    /// Round-tripped through save_data so the info survives project save/load.
    pub macro_description: String,
    pub macro_path: String,
    /// Cached count of inner value/LED displays — used by `min_content_height`
    /// so the node grows tall enough to show every row by default. Updated
    /// each frame from the shared display.
    inner_display_count: usize,
}

impl SubgraphWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            name: String::new(),
            input_defs: Vec::new(),
            output_defs: Vec::new(),
            wants_open: false,
            locked: false,
            macro_description: String::new(),
            macro_path: String::new(),
            inner_display_count: 0,
        }
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn push_config(&self) {
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({
            "name": self.name,
            "inputs": self.input_defs,
            "outputs": self.output_defs,
            "locked": self.locked,
            "macro_description": self.macro_description,
            "macro_path": self.macro_path,
        }));
    }
}

impl NodeWidget for SubgraphWidget {
    fn node_id(&self) -> NodeId {
        self.id
    }
    fn type_name(&self) -> &'static str {
        "Subgraph"
    }
    fn title(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &'static str {
        "Encapsulates an inner graph with custom input/output ports; double-click to enter."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.input_defs
            .iter()
            .map(|p| UiPortDef::from_def(&p.to_port_def()))
            .collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.output_defs
            .iter()
            .map(|p| UiPortDef::from_def(&p.to_port_def()))
            .collect()
    }

    fn min_width(&self) -> f32 {
        120.0
    }
    fn min_content_height(&self) -> f32 {
        // Header row + one row per inner value/LED display. This is a
        // *minimum* — `resizable` stays true, so the user can still grow
        // the node further. Since `size_override` is clamped to this min,
        // macros and freshly-added subgraphs always show their LEDs.
        16.0 + self.inner_display_count as f32 * 18.0
    }
    fn resizable(&self) -> bool {
        true
    }
    fn shared_state(&self) -> &SharedState {
        &self.shared
    }

    fn accent_color(&self) -> Option<Color32> {
        Some(if self.locked { theme::ACCENT_MACRO } else { theme::ACCENT_SUBGRAPH })
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let (inner_count, locked, inner_displays) = {
            let shared = self.shared.lock().unwrap();
            let display = shared
                .display
                .as_ref()
                .and_then(|d| d.downcast_ref::<SubgraphDisplay>());
            if let Some(d) = display {
                (d.inner_node_count, d.locked, d.inner_value_displays.clone())
            } else {
                (0, self.locked, Vec::new())
            }
        };
        self.locked = locked;
        self.inner_display_count = inner_displays.len();

        ui.horizontal(|ui| {
            if self.locked {
                ui.colored_label(
                    Color32::from_rgb(180, 160, 220),
                    egui_phosphor::regular::LOCK,
                )
                .on_hover_text("Macro (locked). Right-click → Embed to edit.");
            }
            ui.colored_label(Color32::from_gray(100), format!("{} nodes", inner_count));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Open button is hidden when locked — match navigation gating.
                if !self.locked
                    && ui
                        .small_button(egui_phosphor::regular::ARROW_SQUARE_OUT)
                        .on_hover_text("Open subgraph")
                        .clicked()
                {
                    self.wants_open = true;
                }
            });
        });

        for d in &inner_displays {
            draw_inner_display_row(ui, d);
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        // Locked macro: show read-only metadata and skip the port/structure
        // editors (they'd imply you can change the macro's interface, which
        // you can't without unlocking via the context menu's "Embed" action).
        if self.locked {
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.colored_label(egui::Color32::from_gray(220), &self.name);
            });
            ui.separator();
            ui.label(egui::RichText::new("Description").strong());
            if self.macro_description.is_empty() {
                ui.colored_label(theme::TEXT_DIM, "(no description)");
            } else {
                ui.label(&self.macro_description);
            }
            ui.separator();
            ui.label(egui::RichText::new("Path").strong());
            if self.macro_path.is_empty() {
                ui.colored_label(theme::TEXT_DIM, "(unknown)");
            } else {
                ui.colored_label(theme::TEXT, &self.macro_path);
            }
            return;
        }

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
                if ui.text_edit_singleline(&mut port.name).changed() {
                    input_changed = true;
                }
                egui::ComboBox::from_id_salt(("sub_in_type", self.id.0, i))
                    .width(80.0)
                    .selected_text(*PORT_TYPE_NAMES.get(port.port_type_idx).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (idx, name) in PORT_TYPE_NAMES.iter().enumerate() {
                            if ui
                                .selectable_value(&mut port.port_type_idx, idx, *name)
                                .changed()
                            {
                                input_changed = true;
                            }
                        }
                    });
                if ui.small_button(egui_phosphor::regular::X).clicked() {
                    remove_input = Some(i);
                }
            });
        }
        if let Some(i) = remove_input {
            self.input_defs.remove(i);
            input_changed = true;
        }
        if ui.small_button("+ Add Input").clicked() {
            self.input_defs.push(SubgraphPortDef {
                name: format!("in {}", self.input_defs.len()),
                port_type_idx: 2,
            });
            input_changed = true;
        }

        ui.separator();

        // Output ports.
        ui.label(egui::RichText::new("Outputs").strong());
        let mut output_changed = false;
        let mut remove_output = None;
        for (i, port) in self.output_defs.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui.text_edit_singleline(&mut port.name).changed() {
                    output_changed = true;
                }
                egui::ComboBox::from_id_salt(("sub_out_type", self.id.0, i))
                    .width(80.0)
                    .selected_text(*PORT_TYPE_NAMES.get(port.port_type_idx).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (idx, name) in PORT_TYPE_NAMES.iter().enumerate() {
                            if ui
                                .selectable_value(&mut port.port_type_idx, idx, *name)
                                .changed()
                            {
                                output_changed = true;
                            }
                        }
                    });
                if ui.small_button(egui_phosphor::regular::X).clicked() {
                    remove_output = Some(i);
                }
            });
        }
        if let Some(i) = remove_output {
            self.output_defs.remove(i);
            output_changed = true;
        }
        if ui.small_button("+ Add Output").clicked() {
            self.output_defs.push(SubgraphPortDef {
                name: format!("out {}", self.output_defs.len()),
                port_type_idx: 2,
            });
            output_changed = true;
        }

        if input_changed || output_changed {
            self.push_config();
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
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
        let out_channels = port_defs
            .iter()
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
    fn node_id(&self) -> NodeId {
        BRIDGE_IN_NODE_ID
    }
    fn type_name(&self) -> &'static str {
        "Graph Input"
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.port_defs
            .iter()
            .map(|p| UiPortDef::from_def(&p.to_port_def()))
            .collect()
    }

    fn min_width(&self) -> f32 {
        110.0
    }
    fn min_content_height(&self) -> f32 {
        0.0
    }
    fn shared_state(&self) -> &SharedState {
        &self.shared
    }

    fn show_content(&mut self, _ui: &mut Ui, _zoom: f32) {}

    fn show_port_labels(&self) -> bool { true }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn draw_inner_display_row(ui: &mut Ui, d: &InnerValueDisplay) {
    ui.horizontal(|ui| {
        let label = if d.name.is_empty() {
            "(unnamed)"
        } else {
            d.name.as_str()
        };
        ui.colored_label(Color32::from_gray(160), label);
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| match d.mode {
                1 => {
                    let brightness = d.value.clamp(0.0, 1.0);
                    let size = egui::Vec2::splat(14.0);
                    let (resp, painter) = ui.allocate_painter(size, egui::Sense::hover());
                    let center = resp.rect.center();
                    let radius = 5.0;
                    let led_color = Color32::from_rgb(
                        (40.0 + 215.0 * brightness) as u8,
                        (10.0 + 20.0 * brightness) as u8,
                        (10.0 + 10.0 * brightness) as u8,
                    );
                    painter.circle_filled(center, radius, led_color);
                }
                _ => {
                    ui.colored_label(
                        Color32::from_gray(220),
                        egui::RichText::new(format!("{:.3}", d.value)).monospace(),
                    );
                }
            },
        );
    });
}

/// Pseudo-node representing the subgraph's output ports inside the inner graph.
/// Inputs correspond to the subgraph's external outputs (data flows out).
pub struct GraphOutputWidget {
    shared: SharedState,
    port_defs: Vec<SubgraphPortDef>,
}

impl GraphOutputWidget {
    pub fn new(port_defs: Vec<SubgraphPortDef>) -> Self {
        let in_channels = port_defs
            .iter()
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
    fn node_id(&self) -> NodeId {
        BRIDGE_OUT_NODE_ID
    }
    fn type_name(&self) -> &'static str {
        "Graph Output"
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.port_defs
            .iter()
            .map(|p| UiPortDef::from_def(&p.to_port_def()))
            .collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![]
    }

    fn min_width(&self) -> f32 {
        110.0
    }
    fn min_content_height(&self) -> f32 {
        0.0
    }
    fn shared_state(&self) -> &SharedState {
        &self.shared
    }

    fn show_content(&mut self, _ui: &mut Ui, _zoom: f32) {}

    fn show_port_labels(&self) -> bool { true }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
