use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::engine::nodes::meta::subgraph::{BRIDGE_IN_NODE_ID, BRIDGE_OUT_NODE_ID, SubgraphPortDef};
use crate::engine::types::{NodeId, ParamDef, ParamValue, PortDir, PortId, PromotedParam};
use crate::widgets::nodes::display::led_display::LedDisplayWidget;
use crate::widgets::nodes::display::value_display::ValueDisplayWidget;
use crate::widgets::nodes::io::audio_input::AudioInputWidget;
use crate::widgets::nodes::math::gradient_source::GradientSourceWidget;
use crate::widgets::nodes::math::lookup::LookupWidget;
use crate::widgets::nodes::math::multiplex::{DemultiplexerWidget, MultiplexerWidget};
use crate::widgets::nodes::io::input_controller::InputControllerWidget;
use crate::widgets::nodes::io::push1::Push1Widget;
use crate::widgets::nodes::io::launchpad::LaunchpadWidget;
use crate::widgets::nodes::io::x1::X1Widget;
use crate::widgets::nodes::meta::portal::{
    InputPortalRxWidget, InputPortalTxWidget, OutputPortalRxWidget, OutputPortalTxWidget,
};
use crate::widgets::nodes::meta::subgraph::SubgraphWidget;
use crate::widgets::nodes::output::effect::EffectWidget;
use crate::widgets::nodes::output::effect_stack::EffectStackWidget;
use crate::widgets::nodes::math::toggle_bank::ToggleBankWidget;
use crate::widgets::nodes::math::trigger_bank::TriggerBankWidget;
use crate::widgets::nodes::ui::button::ButtonWidget;
use crate::widgets::nodes::ui::fader::FaderWidget;
use crate::widgets::nodes::ui::fader_group::FaderGroupWidget;
use crate::widgets::nodes::{GraphLevel, NodeGraph};

// ---------------------------------------------------------------------------
// Save format
// ---------------------------------------------------------------------------

/// Bumped when a node's port layout changes in a way that would repoint
/// saved connections. Files without the field load as version 0.
///
/// 1: hardware nodes gained a leading "connected" output.
///
/// TEMPORARY: the migration exists only so the developers' test projects
/// survive one load. Once they are re-saved, delete `STATUS_PORT_NODE_TYPES`,
/// `connections_for_current_layout`, its tests and the `version` field.
pub const PROJECT_VERSION: u32 = 2;

/// Node types whose outputs shifted by one when the "connected" port was
/// inserted at index 0 (version 1).
const STATUS_PORT_NODE_TYPES: [&str; 5] =
    ["Input Controller", "Audio Input", "Launchpad S", "Push 1", "X1"];

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectFile {
    /// Written on every level, including inner subgraphs and macros, since
    /// each is migrated on its own.
    #[serde(default)]
    pub version: u32,
    pub nodes: Vec<SavedNode>,
    pub connections: Vec<SavedConnection>,
    /// Decorative frames per level — purely visual, no engine side.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frames: Vec<SavedFrame>,
    /// App-level view toggles serialized with the project so the layout
    /// you left a project in comes back on reopen. `None` for inner
    /// (subgraph) levels — only the root level carries view state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<ProjectViewState>,
    /// Canvas position of the Graph Input bridge pseudo-node inside a
    /// subgraph level. Not persisted via the `nodes` vec — bridges are
    /// auto-reconstructed on navigate_into, but their positions are
    /// user-arrangeable so we save them here. Root level has no bridges,
    /// so this stays `None` there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge_in_pos: Option<[f32; 2]>,
    /// Canvas position of the Graph Output bridge pseudo-node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge_out_pos: Option<[f32; 2]>,
}

/// Persisted view-toggle state for the app's side panels and windows.
/// Only meaningful at the root level. Add fields here as more toggles
/// graduate from "session-only" to "remember per project".
#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProjectViewState {
    /// Whether the macro library side panel is visible.
    #[serde(default = "default_show_library")]
    pub show_library: bool,
}

fn default_show_library() -> bool { true }

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedFrame {
    pub id: u64,
    pub title: String,
    /// RGBA (alpha for the border tint; body fill is rendered at low opacity).
    pub color: [u8; 4],
    pub notes: String,
    pub pos: [f32; 2],
    pub size: [f32; 2],
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedNode {
    pub type_name: String,
    pub id: u64,
    pub pos: [f32; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<[f32; 2]>,
    pub params: Vec<SavedParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[allow(dead_code)]
    pub data: Option<serde_json::Value>,
    /// Inner graph for Subgraph nodes (recursive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inner_graph: Option<ProjectFile>,
    /// Whether this node is currently disabled (engine skips its tick).
    /// Defaults to false; the `skip_serializing_if` keeps disabled-less
    /// nodes from writing the field.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    /// Params the user exposed as input ports, in port order. Wires in
    /// `connections` refer to these ports by index after the node's own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub promoted: Vec<PromotedParam>,
    /// Show only the wired ports.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_unused_ports: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedParam {
    pub index: usize,
    pub value: SavedParamValue,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SavedParamValue {
    Float(f64),
    Int(i64),
    Bool(bool),
    Choice(usize),
}

impl SavedParamValue {
    #[allow(dead_code)]
    fn as_f64(&self) -> f64 {
        match self {
            SavedParamValue::Float(v) => *v,
            SavedParamValue::Int(v) => *v as f64,
            SavedParamValue::Bool(v) => {
                if *v {
                    1.0
                } else {
                    0.0
                }
            }
            SavedParamValue::Choice(v) => *v as f64,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedConnection {
    pub from_node: u64,
    pub from_port: usize,
    pub to_node: u64,
    pub to_port: usize,
}

/// Rewrite every `NodeId` inside `project` to a freshly allocated id.
/// Used when deep-cloning a subtree (duplicating a Subgraph) — the
/// original's inner node ids are still live in the running graph, so the
/// duplicate needs its own namespace. Each level is remapped in its own
/// scope (so connections that reference its nodes are rewritten using
/// that level's local map), and `inner_graph` children are recursed
/// through. Bridge pseudo-ids (`u64::MAX - 1`, `u64::MAX`) are never in
/// the map and pass through unchanged.
pub fn remap_ids_inplace<F: FnMut() -> u64>(project: &mut ProjectFile, alloc: &mut F) {
    let mut id_map: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    for node in &mut project.nodes {
        let new_id = alloc();
        id_map.insert(node.id, new_id);
        node.id = new_id;
    }
    for conn in &mut project.connections {
        if let Some(&new) = id_map.get(&conn.from_node) { conn.from_node = new; }
        if let Some(&new) = id_map.get(&conn.to_node) { conn.to_node = new; }
    }
    for node in &mut project.nodes {
        if let Some(ref mut inner) = node.inner_graph {
            remap_ids_inplace(inner, alloc);
        }
    }
}

// ---------------------------------------------------------------------------
// Save
// ---------------------------------------------------------------------------

/// Save a single graph level to a ProjectFile.
pub fn save_level(level: &GraphLevel, graph: &NodeGraph) -> ProjectFile {
    let mut nodes = Vec::new();

    for i in 0..level.nodes.len() {
        let node = level.nodes[i].as_ref();
        let state = &level.states[i];
        let node_id = state.id;

        // Skip bridge pseudo-nodes — they're reconstructed on load.
        if node.type_name() == "Graph Input" || node.type_name() == "Graph Output" {
            continue;
        }

        // Read params from shared state.
        let shared = node.shared_state().lock().unwrap();
        let mut params = Vec::new();
        for (pi, p) in shared.current_params.iter().enumerate() {
            let value = match p {
                ParamDef::Float { value, .. } => SavedParamValue::Float(*value as f64),
                ParamDef::Int { value, .. } => SavedParamValue::Int(*value),
                ParamDef::Bool { value, .. } => SavedParamValue::Bool(*value),
                ParamDef::Choice { value, .. } => SavedParamValue::Choice(*value),
            };
            params.push(SavedParam { index: pi, value });
        }

        let data = shared.save_data.clone();
        let disabled = shared.disabled;
        let promoted = shared.promoted.clone();
        let hide_unused_ports = shared.hide_unused_ports;
        drop(shared);

        // For Subgraph nodes, recursively save the inner level.
        let inner_graph = if node.type_name() == "Subgraph" {
            graph
                .find_level_for_subgraph(node_id)
                .map(|inner_level| save_level(inner_level, graph))
        } else {
            None
        };

        nodes.push(SavedNode {
            type_name: node.type_name().to_string(),
            id: node_id.0,
            pos: [state.pos.x, state.pos.y],
            size: state.size_override.map(|s| [s.x, s.y]),
            params,
            data,
            inner_graph,
            disabled,
            promoted,
            hide_unused_ports,
        });
    }

    let connections = level
        .connections
        .iter()
        .map(|c| SavedConnection {
            from_node: c.from.node.0,
            from_port: c.from.index,
            to_node: c.to.node.0,
            to_port: c.to.index,
        })
        .collect();

    let frames = level.frames.iter().map(|f| SavedFrame {
        id: f.id,
        title: f.title.clone(),
        color: [f.color.r(), f.color.g(), f.color.b(), f.color.a()],
        notes: f.notes.clone(),
        pos: [f.pos.x, f.pos.y],
        size: [f.size.x, f.size.y],
    }).collect();

    // Bridge pseudo-node positions. The bridges themselves are
    // reconstructed on navigate_into, but their user-placed positions
    // round-trip separately so subgraph layouts survive reopen.
    let mut bridge_in_pos = None;
    let mut bridge_out_pos = None;
    for (i, node) in level.nodes.iter().enumerate() {
        let id = node.node_id();
        if id == BRIDGE_IN_NODE_ID {
            let p = level.states[i].pos;
            bridge_in_pos = Some([p.x, p.y]);
        } else if id == BRIDGE_OUT_NODE_ID {
            let p = level.states[i].pos;
            bridge_out_pos = Some([p.x, p.y]);
        }
    }

    // View state is plumbed in by the caller (save_to_file) at the root
    // level only; nested subgraph saves leave it as None.
    ProjectFile {
        version: PROJECT_VERSION,
        nodes,
        connections,
        frames,
        view: None,
        bridge_in_pos,
        bridge_out_pos,
    }
}

pub fn save_graph(graph: &NodeGraph) -> ProjectFile {
    // Always save the root level, regardless of which level is active.
    save_level(graph.root_level(), graph)
}

// ---------------------------------------------------------------------------
// Load
// ---------------------------------------------------------------------------

/// The project's connections rewritten for the port layout this build
/// expects, so wires saved before a layout change keep their endpoints.
fn connections_for_current_layout(project: &ProjectFile) -> Vec<SavedConnection> {
    let mut connections = project.connections.clone();
    if project.version < 1 {
        for c in &mut connections {
            let from_type = project
                .nodes
                .iter()
                .find(|n| n.id == c.from_node)
                .map(|n| n.type_name.as_str());
            if from_type.is_some_and(|t| STATUS_PORT_NODE_TYPES.contains(&t)) {
                c.from_port += 1;
            }
        }
    }
    if project.version < 2 {
        // Change Detect had its trigger first; the value is now port 0.
        for c in &mut connections {
            let to_type = project.nodes.iter().find(|n| n.id == c.to_node).map(|n| n.type_name.as_str());
            if to_type == Some("Change Detect") && c.to_port < 2 {
                c.to_port = 1 - c.to_port;
            }
        }
    }
    connections
}

#[allow(dead_code)]
pub fn load_graph(graph: &mut NodeGraph, project: &ProjectFile) -> Vec<usize> {
    let mut indices = Vec::new();

    let project_connections = connections_for_current_layout(project);

    for saved in &project.nodes {
        let id = NodeId(saved.id);
        // Legacy project files saved the portal node types under their
        // pre-rename names. Remap so `create_from_registry` still finds a
        // factory. The rest of this function continues using `type_name`
        // instead of `saved.type_name` so downstream string matches pick up
        // the new names too.
        let type_name: &str = match saved.type_name.as_str() {
            "Portal In" => "Output Portal TX",
            "Portal Out" => "Output Portal RX",
            // ADSR was renamed to "Envelope" when the node gained
            // multiple envelope kinds. Old projects still save under
            // the former type_name.
            "ADSR" => "Envelope",
            other => other,
        };
        if let Some(node) = graph.create_from_registry(type_name, id) {
            let idx = graph.add_node(node, egui::Pos2::new(saved.pos[0], saved.pos[1]));

            if let Some([w, h]) = saved.size {
                graph.set_node_size(idx, egui::Vec2::new(w, h));
            }

            // Apply params via shared state.
            {
                let n = graph.node_mut(idx);
                let declared = {
                    let shared = n.shared_state().lock().unwrap();
                    shared.current_params.clone()
                };
                let mut shared = n.shared_state().lock().unwrap();
                for sp in &saved.params {
                    let val = if let Some(decl) = declared.get(sp.index) {
                        match decl {
                            ParamDef::Float { .. } => ParamValue::Float(sp.value.as_f64() as f32),
                            ParamDef::Int { .. } => ParamValue::Int(sp.value.as_f64() as i64),
                            ParamDef::Bool { .. } => ParamValue::Bool(sp.value.as_f64() != 0.0),
                            ParamDef::Choice { .. } => {
                                ParamValue::Choice(sp.value.as_f64() as usize)
                            }
                        }
                    } else {
                        match &sp.value {
                            SavedParamValue::Float(v) => ParamValue::Float(*v as f32),
                            SavedParamValue::Int(v) => ParamValue::Int(*v),
                            SavedParamValue::Bool(v) => ParamValue::Bool(*v),
                            SavedParamValue::Choice(v) => ParamValue::Choice(*v),
                        }
                    };
                    shared.pending_params.push((sp.index, val));
                }
                shared.disabled = saved.disabled;
                shared.hide_unused_ports = saved.hide_unused_ports;
                // Set before connections are restored so the extra ports exist.
                shared.promoted = saved.promoted.clone();
                shared.promoted_applied = saved.promoted.clone();
                if !saved.promoted.is_empty() {
                    shared.pending_promoted = Some(saved.promoted.clone());
                }
            }

            // For Subgraph nodes, restore port definitions on the widget
            // BEFORE connections are loaded (so ports exist for wiring).
            if saved.type_name == "Subgraph" {
                if let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(sub) = n.as_any_mut().downcast_mut::<SubgraphWidget>() {
                        if let Some(name) = data.get("name").and_then(|v| v.as_str()) {
                            sub.set_name(name.to_string());
                        }
                        if let Some(inputs) = data.get("inputs").and_then(|v| v.as_array()) {
                            sub.input_defs = inputs
                                .iter()
                                .filter_map(|v| {
                                    serde_json::from_value::<SubgraphPortDef>(v.clone()).ok()
                                })
                                .collect();
                        }
                        if let Some(outputs) = data.get("outputs").and_then(|v| v.as_array()) {
                            sub.output_defs = outputs
                                .iter()
                                .filter_map(|v| {
                                    serde_json::from_value::<SubgraphPortDef>(v.clone()).ok()
                                })
                                .collect();
                        }
                        if let Some(b) = data.get("locked").and_then(|v| v.as_bool()) {
                            sub.locked = b;
                        }
                        if let Some(s) = data.get("macro_description").and_then(|v| v.as_str()) {
                            sub.macro_description = s.to_string();
                        }
                        if let Some(s) = data.get("macro_path").and_then(|v| v.as_str()) {
                            sub.macro_path = s.to_string();
                        }
                        sub.push_config();
                    }
                }

                if let Some(inner_project) = &saved.inner_graph {
                    // Save the active level explicitly — `navigate_up` just
                    // decrements by one, which picks the wrong level when
                    // sibling subgraphs have pushed levels above us in the
                    // navigation Vec (macros with multiple inner subgraphs).
                    let parent_level = graph.active_level_index();

                    graph.navigate_into_by_index(idx);

                    // Restore bridge pseudo-node positions BEFORE the
                    // recursive load, so nested subgraphs that re-enter
                    // this level (or the user navigating in) find the
                    // bridges where they were left.
                    if let Some([x, y]) = inner_project.bridge_in_pos {
                        graph.set_node_pos(BRIDGE_IN_NODE_ID, egui::Pos2::new(x, y));
                    }
                    if let Some([x, y]) = inner_project.bridge_out_pos {
                        graph.set_node_pos(BRIDGE_OUT_NODE_ID, egui::Pos2::new(x, y));
                    }

                    let _inner_indices = load_graph(graph, inner_project);
                    graph.navigate_to_level(parent_level);
                }
            }

            // Restore Fader widget input-port state so wires don't get
            // dropped by `cleanup_stale_connections` on the first frame.
            if saved.type_name == "Fader"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(f) = n.as_any_mut().downcast_mut::<FaderWidget>() {
                        f.restore_from_save_data(data);
                    }
                }

            // Same for Button — its input port only exists when
            // `inputs_enabled` is true, and that flag's default is false.
            // Without restoring early, any incoming wire dies on the first
            // frame before the engine has applied save_data.
            if saved.type_name == "Button"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(b) = n.as_any_mut().downcast_mut::<ButtonWidget>() {
                        b.restore_from_save_data(data);
                    }
                }

            // Toggle Bank's port count is variable — restore `n` before
            // cleanup_stale_connections so wires to channels > default
            // aren't dropped.
            if saved.type_name == "Toggle Bank"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(tb) = n.as_any_mut().downcast_mut::<ToggleBankWidget>() {
                        tb.restore_from_save_data(data);
                    }
                }
            if saved.type_name == "Trigger Bank"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(tb) = n.as_any_mut().downcast_mut::<TriggerBankWidget>() {
                        tb.restore_from_save_data(data);
                    }
                }

            // Portal widgets need their port defs restored before
            // `cleanup_stale_connections` runs, otherwise wires targeting
            // the portal's input/output ports get dropped on the first frame.
            // Each branch matches both the current type name and the
            // pre-rename legacy name ("Portal In"/"Portal Out") so older
            // project files keep loading.
            if matches!(type_name, "Output Portal TX" | "Portal In")
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<OutputPortalTxWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            if matches!(type_name, "Output Portal RX" | "Portal Out")
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<OutputPortalRxWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            if type_name == "Input Portal RX"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<InputPortalRxWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            if type_name == "Input Portal TX"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<InputPortalTxWidget>() {
                        w.restore_from_save_data(data);
                    }
                }

            // Same for Fader Group — restore per-cell inputs/outputs enabled.
            if saved.type_name == "Fader Group"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(fg) = n.as_any_mut().downcast_mut::<FaderGroupWidget>() {
                        fg.restore_from_save_data(data);
                    }
                }

            // Input Controller / Audio Input nodes have dynamic outputs
            // sourced from the live setup; pre-populate them so wires aren't
            // dropped on the first frame's stale-connection sweep.
            if saved.type_name == "Input Controller"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<InputControllerWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            if saved.type_name == "Push 1"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<Push1Widget>() {
                        w.restore_from_save_data(data);
                    }
                }
            if saved.type_name == "X1"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<X1Widget>() {
                        w.restore_from_save_data(data);
                    }
                }
            if saved.type_name == "Launchpad S"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<LaunchpadWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            if saved.type_name == "Audio Input"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<AudioInputWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            if saved.type_name == "Value Display"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<ValueDisplayWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            if saved.type_name == "LED Display"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<LedDisplayWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            // The extra outputs of a Phase Scaler are ports: restore them
            // before saved wires are attached.
            // MIDI nodes have a port per entry of their list: restore it
            // before saved wires are attached.
            if saved.type_name == "MIDI Input"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<crate::widgets::nodes::io::midi_input::MidiInputWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            if saved.type_name == "MIDI Output"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<crate::widgets::nodes::io::midi_output::MidiOutputWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            if saved.type_name == "Clock Divider"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<crate::widgets::nodes::transport::clock_divider::ClockDividerWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            if saved.type_name == "Phase Scaler"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<crate::widgets::nodes::transport::phase_scaler::PhaseScalerWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            if saved.type_name == "Gradient Source"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<GradientSourceWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            // Lookup's widget owns the column list, so it must be restored
            // before cleanup_stale_connections or wires to columns past the
            // default single-column layout get dropped on the first frame.
            if saved.type_name == "Lookup"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<LookupWidget>() {
                        w.restore_from_save_data(data);
                    }
                }

            // Gradient to/from Colors: the stop count decides how many ports
            // there are, so it has to be known before saved wires are attached.
            if saved.type_name == "Gradient to Colors" || saved.type_name == "Colors to Gradient" {
                if let Some(stops) = saved.params.iter().find(|p| p.index == 0).map(|p| p.value.as_f64() as usize)
                    && let Some(w) = graph.node_mut(idx).as_any_mut()
                        .downcast_mut::<crate::widgets::nodes::math::color_reshape::ColorReshapeWidget>()
                {
                    w.set_state_from_load(stops);
                }
            }

            // Mux/Demux need their port type + slot count synced before the
            // first frame's stale-connection sweep, or wires on slots >= 8 or
            // on non-default types get dropped.
            // SavedParamValue is untagged; always coerce via as_f64.
            if saved.type_name == "Multiplexer" || saved.type_name == "Demultiplexer" {
                let type_idx = saved.params.iter().find(|p| p.index == 0)
                    .map(|p| p.value.as_f64() as usize);
                let slots = saved.params.iter().find(|p| p.index == 1)
                    .map(|p| p.value.as_f64() as usize);
                let mode_idx = saved.params.iter().find(|p| p.index == 2)
                    .map(|p| p.value.as_f64() as usize);
                let pt = type_idx.map(crate::engine::nodes::math::multiplex::type_from_index)
                    .unwrap_or(crate::engine::types::PortType::Any);
                let s = slots.unwrap_or(crate::engine::nodes::math::multiplex::MUX_DEFAULT_SLOTS);
                let m = mode_idx
                    .map(crate::engine::nodes::math::multiplex::SelectMode::from_index)
                    .unwrap_or(crate::engine::nodes::math::multiplex::SelectMode::Index);
                let n = graph.node_mut(idx);
                if let Some(w) = n.as_any_mut().downcast_mut::<MultiplexerWidget>() {
                    w.set_state_from_load(pt, s, m);
                } else if let Some(w) = n.as_any_mut().downcast_mut::<DemultiplexerWidget>() {
                    w.set_state_from_load(pt, s, m);
                }
            }
            // Port Hub shares the Mux/Demux pattern: its Type and Slots
            // are stored as params 0 and 1. Restore early so paired
            // in/out wires survive the first-frame stale-connection sweep.
            if saved.type_name == "Port Hub" {
                let type_idx = saved.params.iter().find(|p| p.index == 0)
                    .map(|p| p.value.as_f64() as usize);
                let slots = saved.params.iter().find(|p| p.index == 1)
                    .map(|p| p.value.as_f64() as usize);
                let pt = type_idx
                    .map(crate::engine::nodes::math::multiplex::type_from_index)
                    .unwrap_or(crate::engine::types::PortType::Untyped);
                let s = slots.unwrap_or(
                    crate::engine::nodes::math::port_hub::HUB_DEFAULT_SLOTS,
                );
                let n = graph.node_mut(idx);
                if let Some(w) = n.as_any_mut()
                    .downcast_mut::<crate::widgets::nodes::math::port_hub::PortHubWidget>()
                {
                    w.set_state_from_load(pt, s);
                }
            }

            // Counter: `Select Mode` lives at param index 2. Restore
            // early so a Phase-typed wire into the `phase` port survives
            // the first frame when the saved mode is Phase.
            if saved.type_name == "Counter" {
                let mode_idx = saved.params.iter().find(|p| p.index == 2)
                    .map(|p| p.value.as_f64() as usize);
                let m = mode_idx
                    .map(crate::engine::nodes::math::multiplex::SelectMode::from_index)
                    .unwrap_or(crate::engine::nodes::math::multiplex::SelectMode::Index);
                let n = graph.node_mut(idx);
                if let Some(w) = n.as_any_mut()
                    .downcast_mut::<crate::widgets::nodes::math::counter::CounterWidget>()
                {
                    w.set_state_from_load(m);
                }
            }

            // Envelope: `Mode` is at param index 4. Restore before the
            // first-frame sweep so port labels (gate vs. unused) are
            // already in sync with the saved kind.
            if type_name == "Envelope" {
                let mode_idx = saved.params.iter().find(|p| p.index == 4)
                    .map(|p| p.value.as_f64() as usize);
                let k = mode_idx
                    .map(crate::engine::nodes::transport::envelope::EnvelopeKind::from_index)
                    .unwrap_or(crate::engine::nodes::transport::envelope::EnvelopeKind::Adsr);
                let n = graph.node_mut(idx);
                if let Some(w) = n.as_any_mut()
                    .downcast_mut::<crate::widgets::nodes::transport::envelope::EnvelopeWidget>()
                {
                    w.set_state_from_load(k);
                }
            }

            // Palette Select: `Select Mode` is at param index 0. Same
            // early-restore reasoning as Counter.
            if saved.type_name == "Palette Select" {
                let mode_idx = saved.params.iter().find(|p| p.index == 0)
                    .map(|p| p.value.as_f64() as usize);
                let m = mode_idx
                    .map(crate::engine::nodes::math::multiplex::SelectMode::from_index)
                    .unwrap_or(crate::engine::nodes::math::multiplex::SelectMode::Index);
                let n = graph.node_mut(idx);
                if let Some(w) = n.as_any_mut()
                    .downcast_mut::<crate::widgets::nodes::math::palette_select::PaletteSelectWidget>()
                {
                    w.set_state_from_load(m);
                }
            }

            // Restore Effect Stack and effect widget state from save_data.
            if saved.type_name == "Effect Stack"
                && let Some(data) = &saved.data
                && let Ok(config) = serde_json::from_value::<crate::engine::effects::stack::StackConfig>(data.clone())
                && let Some(stack) = graph.node_mut(idx).as_any_mut().downcast_mut::<EffectStackWidget>()
            {
                stack.set_config(config);
            }
            if let Some(data) = &saved.data
                && let Some(name) = data.get("stack").and_then(|v| v.as_str())
                && let Some(effect) = graph.node_mut(idx).as_any_mut().downcast_mut::<EffectWidget>()
            {
                effect.set_stack(name);
            }

            indices.push(idx);
        }
        // Unknown types are silently skipped.
    }

    // Collect loaded node IDs for filtering connections.
    // Include bridge node IDs so bridge connections are preserved.
    let loaded_ids: Vec<u64> = {
        let mut ids: Vec<u64> = indices
            .iter()
            .map(|&idx| {
                let (node, _) = graph.node_and_state(idx);
                node.node_id().0
            })
            .collect();
        ids.push(BRIDGE_IN_NODE_ID.0);
        ids.push(crate::engine::nodes::meta::subgraph::BRIDGE_OUT_NODE_ID.0);
        ids
    };

    // Restore connections (only between nodes that were actually loaded).
    for sc in &project_connections {
        if !loaded_ids.contains(&sc.from_node) || !loaded_ids.contains(&sc.to_node) {
            continue;
        }
        let from = PortId {
            node: NodeId(sc.from_node),
            index: sc.from_port,
            dir: PortDir::Output,
        };
        let to = PortId {
            node: NodeId(sc.to_node),
            index: sc.to_port,
            dir: PortDir::Input,
        };
        graph.add_connection(from, to);
    }

    // Restore decorative frames into the active level. Only touch the
    // active level's frames when the project actually carries some — this
    // is the same function that's used to graft a macro (single Subgraph
    // node with empty frames) into an existing level, and wiping the
    // user's frames there would silently discard them on every macro drop.
    if !project.frames.is_empty() {
        let target = graph.frames_mut();
        target.clear();
        for sf in &project.frames {
            target.push(crate::widgets::nodes::graph::GraphFrame {
                id: sf.id,
                title: sf.title.clone(),
                color: egui::Color32::from_rgba_unmultiplied(
                    sf.color[0], sf.color[1], sf.color[2], sf.color[3],
                ),
                notes: sf.notes.clone(),
                pos: egui::pos2(sf.pos[0], sf.pos[1]),
                size: egui::vec2(sf.size[0], sf.size[1]),
            });
        }
    }
    // Bump the id counter past any restored frame ids so freshly added
    // frames don't collide.
    if let Some(max) = project.frames.iter().map(|f| f.id).max() {
        graph.bump_next_id_above(max);
    }

    indices
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

pub fn default_project_path() -> PathBuf {
    PathBuf::from("project.json")
}

/// Path for the crash-recovery autosave file that shadows a given project
/// file. Convention: same directory as the project, filename is the project's
/// stem with `.autosave.json` appended and a leading `.` (hidden on unix).
/// If `project_path` is `None`, falls back to `./.project.autosave.json`.
pub fn autosave_path_for(project_path: Option<&std::path::Path>) -> PathBuf {
    match project_path {
        Some(p) => {
            let dir = p.parent().unwrap_or(std::path::Path::new(""));
            let stem = p.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("project");
            dir.join(format!(".{}.autosave.json", stem))
        }
        None => PathBuf::from(".project.autosave.json"),
    }
}

pub fn save_to_file(
    graph: &NodeGraph,
    view: ProjectViewState,
    path: &PathBuf,
) -> Result<(), String> {
    let mut project = save_graph(graph);
    project.view = Some(view);
    let json = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, json).map_err(|e| e.to_string())
}

pub fn load_from_file(path: &PathBuf) -> Result<ProjectFile, String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u64, type_name: &str) -> SavedNode {
        SavedNode {
            type_name: type_name.to_string(),
            id,
            pos: [0.0, 0.0],
            size: None,
            params: Vec::new(),
            data: None,
            inner_graph: None,
            disabled: false,
            promoted: Vec::new(),
            hide_unused_ports: false,
        }
    }

    fn conn(from_node: u64, from_port: usize, to_node: u64) -> SavedConnection {
        SavedConnection { from_node, from_port, to_node, to_port: 0 }
    }

    fn project(version: u32) -> ProjectFile {
        ProjectFile {
            version,
            nodes: vec![node(1, "X1"), node(2, "LFO")],
            connections: vec![conn(1, 3, 2), conn(2, 0, 1)],
            frames: Vec::new(),
            view: None,
            bridge_in_pos: None,
            bridge_out_pos: None,
        }
    }

    #[test]
    fn old_hardware_outputs_shift_past_the_status_port() {
        let out = connections_for_current_layout(&project(0));
        assert_eq!(out[0].from_port, 4);
        // Only outputs of hardware nodes move; this one comes from an LFO.
        assert_eq!(out[1].from_port, 0);
    }

    #[test]
    fn change_detect_wires_follow_its_swapped_inputs() {
        let mut p = project(1);
        p.nodes.push(node(3, "Change Detect"));
        p.connections = vec![conn(2, 0, 3), conn(2, 1, 3)];
        // Wired into (trigger, value) before; now (value, sample on).
        p.connections[0].to_port = 0;
        p.connections[1].to_port = 1;
        let out = connections_for_current_layout(&p);
        assert_eq!((out[0].to_port, out[1].to_port), (1, 0));
        p.version = PROJECT_VERSION;
        let out = connections_for_current_layout(&p);
        assert_eq!((out[0].to_port, out[1].to_port), (0, 1));
    }

    #[test]
    fn current_projects_are_left_alone() {
        let out = connections_for_current_layout(&project(PROJECT_VERSION));
        assert_eq!(out[0].from_port, 3);
    }

    #[test]
    fn effect_stack_and_effects_survive_a_save_and_load() {
        use crate::engine::effects::stack::LayerConfig;
        use crate::widgets::nodes::output::effect::{
            EffectWidget, RIPPLE_BURST, new_shared_stack_names,
        };
        use crate::widgets::nodes::output::effect_stack::EffectStackWidget;
        use crate::widgets::group_context::new_shared_group_context;
        use crate::widgets::nodes::NodeGraph;

        fn graph() -> NodeGraph {
            let mut g = NodeGraph::new();
            let names = new_shared_stack_names();
            let stack_names = names.clone();
            let ctx = new_shared_group_context();
            g.register_node("Effects", "Effect Stack", move |id| {
                Box::new(EffectStackWidget::new(id, crate::engine::types::new_shared_state(3, 0), ctx.clone(), stack_names.clone()))
            });
            g.register_node("Effects", "Ripple Burst", move |id| {
                Box::new(EffectWidget::new(id, crate::engine::types::new_shared_state(48, 0), RIPPLE_BURST, names.clone()))
            });
            g
        }

        let mut a = graph();
        let stack = a.create_from_registry("Effect Stack", NodeId(1)).unwrap();
        let effect = a.create_from_registry("Ripple Burst", NodeId(2)).unwrap();
        let si = a.add_node(stack, egui::pos2(0.0, 0.0));
        let ei = a.add_node(effect, egui::pos2(300.0, 0.0));
        {
            let w = a.node_mut(si).as_any_mut().downcast_mut::<EffectStackWidget>().unwrap();
            let mut config = w.config().clone();
            config.name = "Shelf".into();
            config.parent = "Master".into();
            config.folded = true;
            let mut layer = LayerConfig::new(2);
            layer.opacity = 0.4;
            layer.muted = true;
            config.layers = vec![layer];
            w.set_config(config);
        }
        a.node_mut(ei).as_any_mut().downcast_mut::<EffectWidget>().unwrap().set_stack("Shelf");

        // What the engine would hand back for saving: the config it was sent.
        let sent = |g: &mut NodeGraph, i: usize| g.node_mut(i).shared_state().lock().unwrap().pending_config.clone();
        let (stack_data, effect_data) = (sent(&mut a, si), sent(&mut a, ei));
        for (i, d) in [(si, stack_data), (ei, effect_data)] {
            a.node_mut(i).shared_state().lock().unwrap().save_data = d;
        }
        let saved = save_graph(&a);

        let mut b = graph();
        load_graph(&mut b, &saved);
        let w = b.node_mut(0).as_any_mut().downcast_mut::<EffectStackWidget>().unwrap();
        assert_eq!(w.config().name, "Shelf");
        assert_eq!(w.config().parent, "Master");
        assert!(w.config().folded);
        assert_eq!(w.config().layers.len(), 1);
        assert_eq!((w.config().layers[0].opacity, w.config().layers[0].muted), (0.4, true));
        let e = b.node_mut(1).as_any_mut().downcast_mut::<EffectWidget>().unwrap();
        assert_eq!(e.stack(), "Shelf");
    }

    #[test]
    fn a_bar_loop_with_promoted_inputs_loads_from_its_saved_form() {
        use crate::widgets::nodes::output::effect::{BAR_LOOP, EffectWidget, new_shared_stack_names};
        use crate::widgets::nodes::NodeGraph;

        let mut g = NodeGraph::new();
        let names = new_shared_stack_names();
        g.register_node("Effects", "Bar Loop", move |id| {
            Box::new(EffectWidget::new(id, crate::engine::types::new_shared_state(48, 0), BAR_LOOP, names.clone()))
        });
        // Exactly what the Group Output conversion writes.
        let project: ProjectFile = serde_json::from_str(r#"{
            "nodes": [{
                "type_name": "Bar Loop", "id": 704, "pos": [10.0, 20.0],
                "params": [{"index": 2, "value": 0}],
                "data": {"stack": "Record Shelf"},
                "promoted": [{"name": "Position", "kind": "Float"}, {"name": "Width", "kind": "Float"}]
            }],
            "connections": []
        }"#).unwrap();
        load_graph(&mut g, &project);

        let ports: Vec<String> = g.node_and_state(0).0.ui_inputs_full().into_iter().map(|p| p.def.name).collect();
        assert_eq!(ports, ["color", "Position", "Width"]);
        let n = g.node_mut(0);
        assert_eq!(n.as_any_mut().downcast_mut::<EffectWidget>().unwrap().stack(), "Record Shelf");
        let shared = n.shared_state().lock().unwrap();
        assert_eq!(shared.promoted.len(), 2);
        assert_eq!(shared.pending_promoted.as_ref().map(Vec::len), Some(2));
        assert!(shared.pending_params.iter().any(|(i, _)| *i == 2), "the anchor was restored");
    }

    #[test]
    fn a_wire_from_an_extra_phase_scaler_output_survives_loading() {
        use crate::widgets::nodes::display::value_display::ValueDisplayWidget;
        use crate::widgets::nodes::transport::phase_scaler::PhaseScalerWidget;
        use crate::widgets::nodes::NodeGraph;

        let mut g = NodeGraph::new();
        g.register_node("Transport", "Phase Scaler", |id| {
            Box::new(PhaseScalerWidget::new(id, crate::engine::types::new_shared_state(4, 16)))
        });
        g.register_node("Display", "Value Display", |id| {
            Box::new(ValueDisplayWidget::new(id, crate::engine::types::new_shared_state(1, 0)))
        });
        let project: ProjectFile = serde_json::from_str(r#"{
            "nodes": [
                {"type_name": "Phase Scaler", "id": 1, "pos": [0.0, 0.0], "params": [],
                 "data": {"extra": [{"exponent": -2, "offset": 0.0}, {"exponent": 3, "offset": 0.0}]}},
                {"type_name": "Value Display", "id": 2, "pos": [200.0, 0.0], "params": []}
            ],
            "connections": [{"from_node": 1, "from_port": 2, "to_node": 2, "to_port": 0}]
        }"#).unwrap();
        load_graph(&mut g, &project);
        g.cleanup_stale_connections();
        assert_eq!(g.node_and_state(0).0.ui_outputs().len(), 3);
        assert_eq!(g.root_level().connections.len(), 1, "the wire to the third output was kept");
    }

    #[test]
    fn wires_to_midi_node_entries_survive_loading() {
        use crate::midi_io::MidiHub;
        use crate::widgets::nodes::display::value_display::ValueDisplayWidget;
        use crate::widgets::nodes::io::midi_common::new_shared_learned;
        use crate::widgets::nodes::io::midi_input::MidiInputWidget;
        use crate::widgets::nodes::io::midi_output::MidiOutputWidget;
        use crate::widgets::nodes::NodeGraph;

        let mut g = NodeGraph::new();
        let (hub, learned) = (MidiHub::idle(), new_shared_learned());
        let (h1, l1, h2, l2) = (hub.clone(), learned.clone(), hub, learned);
        g.register_node("IO", "MIDI Input", move |id| {
            Box::new(MidiInputWidget::new(id, crate::engine::types::new_shared_state(0, 33), h1.clone(), l1.clone()))
        });
        g.register_node("IO", "MIDI Output", move |id| {
            Box::new(MidiOutputWidget::new(id, crate::engine::types::new_shared_state(32, 1), h2.clone(), l2.clone()))
        });
        g.register_node("Display", "Value Display", |id| {
            Box::new(ValueDisplayWidget::new(id, crate::engine::types::new_shared_state(1, 0)))
        });
        let project: ProjectFile = serde_json::from_str(r#"{
            "nodes": [
                {"type_name": "MIDI Input", "id": 1, "pos": [0.0, 0.0], "params": [],
                 "data": {"port": "dev", "entries": [
                    {"id": 1, "name": "a", "binding": {"kind": "cc", "channel": 1, "number": 1}, "mode": "value"},
                    {"id": 2, "name": "b", "binding": {"kind": "note", "channel": 1, "number": 36}, "mode": "gate"}]}},
                {"type_name": "MIDI Output", "id": 2, "pos": [200.0, 0.0], "params": [],
                 "data": {"port": "dev", "entries": [
                    {"id": 1, "binding": {"kind": "cc", "channel": 1, "number": 1}, "mode": "value"},
                    {"id": 2, "binding": {"kind": "cc", "channel": 1, "number": 2}, "mode": "value"}]}},
                {"type_name": "Value Display", "id": 3, "pos": [400.0, 0.0], "params": []}
            ],
            "connections": [
                {"from_node": 1, "from_port": 2, "to_node": 3, "to_port": 0},
                {"from_node": 1, "from_port": 1, "to_node": 2, "to_port": 1}
            ]
        }"#).unwrap();
        load_graph(&mut g, &project);
        g.cleanup_stale_connections();
        assert_eq!(g.node_and_state(0).0.ui_outputs().len(), 3);
        assert_eq!(g.node_and_state(1).0.ui_inputs().len(), 2);
        assert_eq!(g.root_level().connections.len(), 2, "both wires kept, to the second port of each side");
    }

    #[test]
    fn hiding_unused_ports_is_saved_and_restored() {
        use crate::widgets::nodes::math::constant::ConstantWidget;
        use crate::widgets::nodes::NodeGraph;

        let mut g = NodeGraph::new();
        g.register_node("Math", "Const Value", |id| {
            Box::new(ConstantWidget::new(id, crate::engine::types::PortType::Untyped, crate::engine::types::new_shared_state(0, 1)))
        });
        let project: ProjectFile = serde_json::from_str(r#"{
            "nodes": [{"type_name": "Const Value", "id": 1, "pos": [0.0, 0.0], "params": [], "hide_unused_ports": true}],
            "connections": []
        }"#).unwrap();
        load_graph(&mut g, &project);
        assert!(g.node_mut(0).shared_state().lock().unwrap().hide_unused_ports);
        let saved = save_graph(&g);
        assert!(saved.nodes[0].hide_unused_ports);
        let json = serde_json::to_string(&saved).unwrap();
        assert!(json.contains("hide_unused_ports"));
    }

    #[test]
    fn gradient_nodes_get_their_stop_count_before_saved_wires_are_attached() {
        use crate::engine::nodes::math::color_reshape::ReshapeKind;
        use crate::widgets::nodes::math::color_reshape::ColorReshapeWidget;
        use crate::widgets::nodes::NodeGraph;

        let mut g = NodeGraph::new();
        for kind in [ReshapeKind::GradientToColors, ReshapeKind::ColorsToGradient] {
            g.register_node("Color", kind.type_name(), move |id| {
                Box::new(ColorReshapeWidget::new(id, crate::engine::types::new_shared_state(48, 48), kind))
            });
        }
        // Colour 3 of the splitter into colour 3 of the builder: only ports
        // that exist with six stops can carry it.
        let project: ProjectFile = serde_json::from_str(r#"{
            "nodes": [
                {"type_name": "Gradient to Colors", "id": 1, "pos": [0.0, 0.0], "params": [{"index": 0, "value": 6}]},
                {"type_name": "Colors to Gradient", "id": 2, "pos": [200.0, 0.0], "params": [{"index": 0, "value": 6}]}
            ],
            "connections": [{"from_node": 1, "from_port": 15, "to_node": 2, "to_port": 15}]
        }"#).unwrap();
        load_graph(&mut g, &project);

        assert_eq!(g.node_and_state(0).0.ui_outputs().len(), 18);
        assert_eq!(g.node_and_state(1).0.ui_inputs().len(), 18);
        assert_eq!(g.root_level().connections.len(), 1);
    }
}
