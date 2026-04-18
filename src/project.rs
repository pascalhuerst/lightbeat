use std::path::PathBuf;

use egui;
use serde::{Deserialize, Serialize};

use crate::engine::nodes::meta::subgraph::{BRIDGE_IN_NODE_ID, SubgraphPortDef};
use crate::engine::types::{NodeId, ParamDef, ParamValue, PortDir, PortId};
use crate::widgets::nodes::display::led_display::LedDisplayWidget;
use crate::widgets::nodes::display::value_display::ValueDisplayWidget;
use crate::widgets::nodes::io::audio_input::AudioInputWidget;
use crate::widgets::nodes::math::gradient_source::GradientSourceWidget;
use crate::widgets::nodes::math::multiplex::{DemultiplexerWidget, MultiplexerWidget};
use crate::widgets::nodes::io::input_controller::InputControllerWidget;
use crate::widgets::nodes::io::push1::Push1Widget;
use crate::widgets::nodes::meta::portal::{PortalInWidget, PortalOutWidget};
use crate::widgets::nodes::meta::subgraph::SubgraphWidget;
use crate::widgets::nodes::output::effect_stack::EffectStackWidget;
use crate::widgets::nodes::output::group::GroupWidget;
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
        if node.type_name() == "GraphInput" || node.type_name() == "GraphOutput" {
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

    ProjectFile { nodes, connections }
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
        if let Some(node) = graph.create_from_registry(&saved.type_name, id) {
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
                    let _inner_indices = load_graph(graph, inner_project);
                    graph.navigate_to_level(parent_level);
                }
            }

            // Restore Group Output widget state from save_data.
            if saved.type_name == "Group Output" {
                // Sync the mode onto the widget BEFORE cleanup_stale_connections
                // runs, so Triggered-mode wires (trigger/select/width/gradient)
                // aren't dropped because the widget still reports Flood ports.
                // Note: SavedParamValue is `#[serde(untagged)]`, so Choice(1)
                // round-trips through JSON as Float(1.0) — always coerce via as_f64.
                let mode_idx = saved.params.iter()
                    .find(|p| p.index == 0)
                    .map(|p| p.value.as_f64() as usize);
                if let Some(i) = mode_idx {
                    let n = graph.node_mut(idx);
                    if let Some(grp) = n.as_any_mut().downcast_mut::<GroupWidget>() {
                        grp.set_mode_from_load(
                            crate::engine::nodes::output::group::GroupMode::from_index(i),
                        );
                    }
                }

                if let Some(data) = &saved.data {
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
            }

            // Restore Fader widget input-port state so wires don't get
            // dropped by `cleanup_stale_connections` on the first frame.
            if saved.type_name == "Fader" {
                if let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(f) = n.as_any_mut().downcast_mut::<FaderWidget>() {
                        f.restore_from_save_data(data);
                    }
                }
            }

            // Portal widgets need their port defs restored before
            // `cleanup_stale_connections` runs, otherwise wires targeting
            // the portal's input/output ports get dropped on the first frame.
            if saved.type_name == "Portal In" {
                if let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<PortalInWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            }
            if saved.type_name == "Portal Out" {
                if let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<PortalOutWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            }

            // Same for Fader Group — restore per-cell inputs/outputs enabled.
            if saved.type_name == "Fader Group" {
                if let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(fg) = n.as_any_mut().downcast_mut::<FaderGroupWidget>() {
                        fg.restore_from_save_data(data);
                    }
                }
            }

            // Input Controller / Audio Input nodes have dynamic outputs
            // sourced from the live setup; pre-populate them so wires aren't
            // dropped on the first frame's stale-connection sweep.
            if saved.type_name == "Input Controller" {
                if let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<InputControllerWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            }
            if saved.type_name == "Push 1" {
                if let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<Push1Widget>() {
                        w.restore_from_save_data(data);
                    }
                }
            }
            if saved.type_name == "Audio Input" {
                if let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<AudioInputWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            }
            if saved.type_name == "Value Display" {
                if let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<ValueDisplayWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            }
            if saved.type_name == "LED Display" {
                if let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<LedDisplayWidget>() {
                        w.restore_from_save_data(data);
                    }
                }
            }
            if saved.type_name == "Gradient Source" {
                if let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(w) = n.as_any_mut().downcast_mut::<GradientSourceWidget>() {
                        w.restore_from_save_data(data);
                    }
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
            if saved.type_name == "Effect Stack" {
                if let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(stack) = n.as_any_mut().downcast_mut::<EffectStackWidget>() {
                        if let Some(ids) = data.get("group_ids").and_then(|v| v.as_array()) {
                            stack.selected_group_ids = ids
                                .iter()
                                .filter_map(|v| v.as_u64().map(|n| n as u32))
                                .collect();
                        }
                        if let Some(layers) = data.get("layers") {
                            if let Ok(parsed) = serde_json::from_value(layers.clone()) {
                                stack.layers = parsed;
                            }
                        }
                        stack.push_config_to_engine();
                    }
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

    indices
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

pub fn default_project_path() -> PathBuf {
    PathBuf::from("project.json")
}

pub fn save_to_file(graph: &NodeGraph, path: &PathBuf) -> Result<(), String> {
    let project = save_graph(graph);
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
