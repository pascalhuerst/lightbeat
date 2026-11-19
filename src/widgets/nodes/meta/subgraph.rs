use egui::{self, Color32, Rect, Ui, Vec2};
use std::any::Any;

use crate::engine::nodes::meta::subgraph::{
    BRIDGE_IN_NODE_ID, BRIDGE_OUT_NODE_ID, InnerValueDisplay, PORT_TYPE_NAMES, SubgraphDisplay,
    SubgraphConfig, SubgraphPortDef,
};
use crate::engine::types::*;
use crate::widgets::setup_ui;
use crate::theme;
use crate::widgets::nodes::node::{LinkJump, LinkedNode, NodeWidget};
use crate::widgets::nodes::panel::{self, PanelRequest, PanelRow};
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
    pub macro_hash: Option<String>,
    /// Subgraphs with the same value are instances of one definition
    /// (`linked.rs`); `None` is a plain subgraph.
    pub definition: Option<u64>,
    /// How many subgraphs share the definition (set by the graph each frame).
    instances: usize,
    /// The other instances, for the inspector's jump list.
    peers: LinkJump,
    /// Whether the library file this was placed from changed or is gone,
    /// worked out by the app and shown in the inspector.
    pub library_state: LibraryState,
    /// Set by the inspector buttons for the app/graph to carry out.
    pub request: Option<SubgraphRequest>,
    /// Cached count of inner value/LED displays — used by `min_content_height`
    /// so the node grows tall enough to show every row by default. Updated
    /// each frame from the shared display.
    inner_display_count: usize,
    /// Set when a port is removed from the middle of a list, for the graph to
    /// move the outer wires after it.
    removed: Option<(PortDir, usize)>,
    /// The front panel: a cell size for each control the inner nodes put on
    /// it, in order, the width the node is drawn at (the cells wrap to it) and
    /// the list the inspector shows. All set by the graph each frame.
    panel_cells: Vec<Vec2>,
    panel_width: f32,
    panel_rows: Vec<PanelRow>,
    panel_request: Option<PanelRequest>,
    /// Where the panel was drawn last, so a double-click on a control does not
    /// open the subgraph.
    panel_screen: Option<Rect>,
    jump: Option<NodeId>,
}

/// Whether a macro instance still matches its library file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LibraryState {
    Unknown,
    Current,
    Changed,
    Missing,
}

/// What the inspector of a subgraph asks the graph or the app to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SubgraphRequest {
    MakeUnique,
    UpdateFromLibrary,
}

/// Height of the header row (node count, open button) above the panel.
pub const PANEL_TOP: f32 = 24.0;

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
            macro_hash: None,
            definition: None,
            instances: 1,
            peers: LinkJump::default(),
            library_state: LibraryState::Unknown,
            request: None,
            inner_display_count: 0,
            removed: None,
            panel_cells: Vec::new(),
            panel_width: 0.0,
            panel_rows: Vec::new(),
            panel_request: None,
            panel_screen: None,
            jump: None,
        }
    }

    pub fn set_panel(&mut self, cells: Vec<Vec2>, rows: Vec<PanelRow>) {
        if cells.is_empty() {
            self.panel_screen = None;
        }
        self.panel_cells = cells;
        self.panel_rows = rows;
    }

    pub fn set_panel_width(&mut self, width: f32) {
        self.panel_width = width;
    }

    pub fn has_panel(&self) -> bool {
        !self.panel_cells.is_empty()
    }

    pub fn set_panel_screen(&mut self, rect: Option<Rect>) {
        self.panel_screen = rect;
    }

    pub fn panel_contains(&self, pos: egui::Pos2) -> bool {
        self.panel_screen.is_some_and(|r| r.contains(pos))
    }

    pub fn request_panel(&mut self, request: PanelRequest) {
        self.panel_request = Some(request);
    }

    pub fn take_panel_request(&mut self) -> Option<PanelRequest> {
        self.panel_request.take()
    }

    /// Width the cells may use, inside the node's side margins.
    fn panel_content_width(&self) -> f32 {
        let l = self.layout();
        self.panel_width - 2.0 * (l.side_inset + l.gutter_left.max(l.gutter_right))
    }

    fn show_panel_list(&mut self, ui: &mut Ui) {
        ui.separator();
        setup_ui::section(ui, "Front panel");
        if self.panel_rows.is_empty() {
            ui.colored_label(
                theme::TEXT_DIM,
                "Nothing yet. Open the subgraph and choose Show on parent for a control.",
            );
            return;
        }
        let rows = self.panel_rows.clone();
        // The items of one node move together, so the arrows count nodes.
        let nodes = rows.iter().map(|r| r.node_id).fold(Vec::new(), |mut v: Vec<NodeId>, id| {
            if !v.contains(&id) {
                v.push(id);
            }
            v
        });
        for row in &rows {
            ui.horizontal(|ui| {
                if !self.locked {
                    let at = nodes.iter().position(|&n| n == row.node_id).unwrap_or(0);
                    if let Some(mv) = setup_ui::move_buttons(ui, at, nodes.len()) {
                        self.request_panel(PanelRequest::Move(row.node_id, mv));
                    }
                    if setup_ui::delete_button(ui, "Take it off the panel") {
                        self.request_panel(PanelRequest::Remove(row.node_id, row.param.clone()));
                    }
                    if ui.small_button(egui_phosphor::regular::ARROW_SQUARE_OUT).on_hover_text("Go to this node").clicked() {
                        self.jump = Some(row.node_id);
                    }
                }
                ui.label(&row.label);
            });
        }
    }

    pub fn remove_input(&mut self, i: usize) {
        self.input_defs.remove(i);
        self.removed = Some((PortDir::Input, i));
    }

    pub fn remove_output(&mut self, i: usize) {
        self.output_defs.remove(i);
        self.removed = Some((PortDir::Output, i));
    }

    pub fn push_config(&self) {
        let mut shared = self.shared.lock();
        shared.pending_config = Some(SubgraphConfig {
            name: Some(self.name.clone()),
            inputs: Some(self.input_defs.clone()),
            outputs: Some(self.output_defs.clone()),
            locked: Some(self.locked),
            macro_description: Some(self.macro_description.clone()),
            macro_path: Some(self.macro_path.clone()),
            macro_hash: self.macro_hash.clone(),
            definition: self.definition,
        }.to_value());
    }

    pub fn instances(&self) -> usize {
        self.instances
    }

    /// The name, ports and macro origin as the widget holds them.
    fn config(&self) -> SubgraphConfig {
        SubgraphConfig {
            name: Some(self.name.clone()),
            inputs: Some(self.input_defs.clone()),
            outputs: Some(self.output_defs.clone()),
            locked: Some(self.locked),
            macro_description: Some(self.macro_description.clone()),
            macro_path: Some(self.macro_path.clone()),
            macro_hash: self.macro_hash.clone(),
            definition: self.definition,
        }
    }

    /// Link information shared by the locked and the editable inspector.
    fn show_link_info(&mut self, ui: &mut Ui) {
        if self.definition.is_none() || self.instances < 2 {
            return;
        }
        ui.separator();
        setup_ui::section(ui, "Linked");
        ui.label(format!(
            "Instance of {}, used {} times. Editing inside one changes all of them; front-panel values stay per instance.",
            self.display_title(),
            self.instances
        ));
        if ui.button("Make unique").on_hover_text("This instance leaves the definition and keeps its own copy").clicked() {
            self.request = Some(SubgraphRequest::MakeUnique);
        }
        self.peers.show(ui, "Other instances");
    }

    fn show_library_info(&mut self, ui: &mut Ui) {
        if self.macro_path.is_empty() {
            return;
        }
        match self.library_state {
            LibraryState::Unknown | LibraryState::Current => {}
            LibraryState::Changed => {
                ui.colored_label(theme::SEM_WARNING, "The library file changed since this was placed.");
            }
            LibraryState::Missing => {
                ui.colored_label(theme::SEM_WARNING, "The library file is missing.");
            }
        }
        if self.library_state == LibraryState::Changed
            && ui.button("Update from library").on_hover_text("Replace the definition in every instance with the library version").clicked()
        {
            self.request = Some(SubgraphRequest::UpdateFromLibrary);
        }
    }
}

impl NodeWidget for SubgraphWidget {
    /// Restore the port-affecting widget state (name, input/output port
    /// definitions, lock/macro metadata) directly from save_data. Called
    /// by the project loader and by paste, BEFORE cleanup_stale_connections
    /// or navigate_into reads `ui_inputs` / `ui_outputs` — without this
    /// early restore, a freshly duplicated subgraph would navigate into
    /// an inner level with empty bridge-node ports, and connections into
    /// those ports would be lost.
    fn restore_from_save(&mut self, data: Option<&serde_json::Value>, _params: &[(usize, crate::engine::types::ParamValue)]) {
        let Some(data) = data else { return };
        let cfg = SubgraphConfig::from_value(data);
        if let Some(name) = cfg.name {
            self.name = name;
        }
        if let Some(inputs) = cfg.inputs {
            self.input_defs = inputs;
        }
        if let Some(outputs) = cfg.outputs {
            self.output_defs = outputs;
        }
        if let Some(b) = cfg.locked {
            self.locked = b;
        }
        if let Some(s) = cfg.macro_description {
            self.macro_description = s;
        }
        if let Some(s) = cfg.macro_path {
            self.macro_path = s;
        }
        self.macro_hash = cfg.macro_hash;
        self.definition = cfg.definition;
        self.push_config();
    }

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

    fn take_removed_port(&mut self) -> Option<(PortDir, usize)> {
        self.removed.take()
    }

    fn min_width(&self) -> f32 {
        let l = self.layout();
        let widest = self.panel_cells.iter().map(|c| c.x).fold(0.0_f32, f32::max);
        120.0_f32.max(widest + 2.0 * (l.side_inset + l.gutter_left.max(l.gutter_right)))
    }
    fn take_jump(&mut self) -> Option<NodeId> {
        self.jump.take().or_else(|| self.peers.take())
    }
    fn set_link_peers(&mut self, peers: Vec<LinkedNode>) {
        self.peers.set_peers(peers);
    }
    fn set_instance_count(&mut self, count: usize) {
        self.instances = count;
    }
    fn live_data(&self) -> Option<serde_json::Value> {
        Some(self.config().to_value())
    }
    fn min_content_height(&self) -> f32 {
        if self.has_panel() {
            return PANEL_TOP + panel::flow(&self.panel_cells, self.panel_content_width()).size.y;
        }
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
            let shared = self.shared.lock();
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
        // A panel replaces the read-only list of displays.
        let inner_displays = if self.has_panel() { Vec::new() } else { inner_displays };
        self.inner_display_count = inner_displays.len();

        ui.horizontal(|ui| {
            if self.locked {
                ui.colored_label(
                    theme::MACRO_LOCK,
                    egui_phosphor::regular::LOCK,
                )
                .on_hover_text("Macro (locked). Right-click → Embed to edit.");
            }
            ui.colored_label(theme::TEXT_DIM, format!("{} nodes", inner_count));
            if self.definition.is_some() && self.instances > 1 {
                ui.colored_label(theme::LINK_BADGE, format!("x{}", self.instances))
                    .on_hover_text(format!("Instance of {}, used {} times. Changes inside apply to all.", self.display_title(), self.instances));
            }
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
                ui.label("Name");
                ui.colored_label(theme::TEXT, &self.name);
            });
            ui.separator();
            setup_ui::section(ui, "Description");
            if self.macro_description.is_empty() {
                ui.colored_label(theme::TEXT_DIM, "(no description)");
            } else {
                ui.label(&self.macro_description);
            }
            ui.separator();
            setup_ui::section(ui, "Path");
            if self.macro_path.is_empty() {
                ui.colored_label(theme::TEXT_DIM, "(unknown)");
            } else {
                ui.colored_label(theme::TEXT, &self.macro_path);
            }
            self.show_library_info(ui);
            self.show_link_info(ui);
            self.show_panel_list(ui);
            return;
        }

        // Name.
        ui.horizontal(|ui| {
            ui.label("Name");
            if ui.text_edit_singleline(&mut self.name).changed() {
                self.push_config();
            }
        });

        ui.separator();

        // Input ports.
        setup_ui::section(ui, "Inputs");
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
                if setup_ui::delete_button(ui, "Delete this input") {
                    remove_input = Some(i);
                }
            });
        }
        if let Some(i) = remove_input {
            self.remove_input(i);
            input_changed = true;
        }
        if setup_ui::add_button(ui, "Add input") {
            self.input_defs.push(SubgraphPortDef {
                name: format!("in {}", self.input_defs.len() + 1),
                port_type_idx: 2,
            });
            input_changed = true;
        }

        ui.separator();

        // Output ports.
        setup_ui::section(ui, "Outputs");
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
                if setup_ui::delete_button(ui, "Delete this output") {
                    remove_output = Some(i);
                }
            });
        }
        if let Some(i) = remove_output {
            self.remove_output(i);
            output_changed = true;
        }
        if setup_ui::add_button(ui, "Add output") {
            self.output_defs.push(SubgraphPortDef {
                name: format!("out {}", self.output_defs.len() + 1),
                port_type_idx: 2,
            });
            output_changed = true;
        }

        if input_changed || output_changed {
            self.push_config();
        }

        self.show_library_info(ui);
        self.show_link_info(ui);
        self.show_panel_list(ui);
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

    /// The subgraph lost port `i`; keep the bridge in step without waiting
    /// for the next time the level is opened.
    pub fn remove_port(&mut self, i: usize) {
        if i < self.port_defs.len() {
            self.port_defs.remove(i);
        }
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
        ui.colored_label(theme::TEXT_MUTED, label);
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
                        theme::TEXT,
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

    /// The subgraph lost port `i`; keep the bridge in step without waiting
    /// for the next time the level is opened.
    pub fn remove_port(&mut self, i: usize) {
        if i < self.port_defs.len() {
            self.port_defs.remove(i);
        }
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
