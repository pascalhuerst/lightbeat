use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::engine::nodes::meta::subgraph::{BRIDGE_IN_NODE_ID, BRIDGE_OUT_NODE_ID, SubgraphPortDef};
use crate::engine::types::{NodeId, ParamDef, ParamValue, PortDir, PortId};
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
use crate::widgets::nodes::output::effect_stack::EffectStackWidget;
use crate::widgets::nodes::output::group::GroupWidget;
use crate::widgets::nodes::math::toggle_bank::ToggleBankWidget;
use crate::widgets::nodes::math::trigger_bank::TriggerBankWidget;
use crate::widgets::nodes::ui::button::ButtonWidget;
use crate::widgets::nodes::ui::fader::FaderWidget;
use crate::widgets::nodes::ui::fader_group::FaderGroupWidget;
use crate::widgets::nodes::{GraphLevel, NodeGraph};

// ---------------------------------------------------------------------------
// Save format
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectFile {
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

#[allow(dead_code)]
pub fn load_graph(graph: &mut NodeGraph, project: &ProjectFile) -> Vec<usize> {
    let mut indices = Vec::new();

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

            // Restore Group Output widget state from save_data.
            if saved.type_name == "Group Output"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(grp) = n.as_any_mut().downcast_mut::<GroupWidget>() {
                        if let Some(ids) = data.get("group_ids").and_then(|v| v.as_array()) {
                            grp.selected_group_ids = ids
                                .iter()
                                .filter_map(|v| v.as_u64().map(|n| n as u32))
                                .collect();
                        }
                        grp.push_config_to_engine();
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

            // Mux/Demux need their port type + slot count synced before the
            // first frame's stale-connection sweep, or wires on slots >= 8 or
            // on non-default types get dropped.
            // SavedParamValue is untagged; always coerce via as_f64.
            if saved.type_name == "Multiplexer" || saved.type_name == "Demultiplexer" {
                let type_idx = saved.params.iter().find(|p| p.index == 0)
                    .map(|p| p.value.as_f64() as usize);
                let slots = saved.params.iter().find(|p| p.index == 1)
                    .map(|p| p.value.as_f64() as usize);
                let pt = type_idx.map(crate::engine::nodes::math::multiplex::type_from_index)
                    .unwrap_or(crate::engine::types::PortType::Any);
                let s = slots.unwrap_or(crate::engine::nodes::math::multiplex::MUX_DEFAULT_SLOTS);
                let n = graph.node_mut(idx);
                if let Some(w) = n.as_any_mut().downcast_mut::<MultiplexerWidget>() {
                    w.set_state_from_load(pt, s);
                } else if let Some(w) = n.as_any_mut().downcast_mut::<DemultiplexerWidget>() {
                    w.set_state_from_load(pt, s);
                }
            }

            // Restore Effect Stack widget state from save_data.
            if saved.type_name == "Effect Stack"
                && let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(stack) = n.as_any_mut().downcast_mut::<EffectStackWidget>() {
                        if let Some(ids) = data.get("group_ids").and_then(|v| v.as_array()) {
                            stack.selected_group_ids = ids
                                .iter()
                                .filter_map(|v| v.as_u64().map(|n| n as u32))
                                .collect();
                        }
                        if let Some(layers) = data.get("layers")
                            && let Ok(parsed) = serde_json::from_value(layers.clone()) {
                                stack.layers = parsed;
                            }
                        stack.push_config_to_engine();
                    }
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
    for sc in &project.connections {
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
