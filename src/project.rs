use std::path::PathBuf;

use egui;
use serde::{Deserialize, Serialize};

use crate::engine::nodes::meta::subgraph::{BRIDGE_IN_NODE_ID, BRIDGE_OUT_NODE_ID, SubgraphPortDef};
use crate::engine::types::{NodeId, ParamDef, ParamValue, PortDir, PortId};
use crate::widgets::nodes::{GraphLevel, NodeGraph};
use crate::widgets::nodes::meta::subgraph::SubgraphWidget;
use crate::widgets::nodes::output::bar_pattern::BarPatternWidget;
use crate::widgets::nodes::output::group::GroupWidget;

// ---------------------------------------------------------------------------
// Save format
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct ProjectFile {
    pub nodes: Vec<SavedNode>,
    pub connections: Vec<SavedConnection>,
}

#[derive(Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize)]
pub struct SavedParam {
    pub index: usize,
    pub value: SavedParamValue,
}

#[derive(Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize)]
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
fn save_level(level: &GraphLevel, graph: &NodeGraph) -> ProjectFile {
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
            graph.find_level_for_subgraph(node_id)
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
                        if let Some(inputs) = data.get("inputs").and_then(|v| v.as_array()) {
                            sub.input_defs = inputs.iter()
                                .filter_map(|v| serde_json::from_value::<SubgraphPortDef>(v.clone()).ok())
                                .collect();
                        }
                        if let Some(outputs) = data.get("outputs").and_then(|v| v.as_array()) {
                            sub.output_defs = outputs.iter()
                                .filter_map(|v| serde_json::from_value::<SubgraphPortDef>(v.clone()).ok())
                                .collect();
                        }
                        sub.push_config();
                    }
                }

                if let Some(inner_project) = &saved.inner_graph {
                    // Navigate into the subgraph to create its inner level.
                    graph.navigate_into_by_index(idx);

                    // Recursively load inner nodes (including bridge connections if saved).
                    let _inner_indices = load_graph(graph, inner_project);

                    // Navigate back to parent.
                    graph.navigate_up();
                }
            }

            // Restore Group Output widget state from save_data.
            if saved.type_name == "Group Output" {
                if let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(grp) = n.as_any_mut().downcast_mut::<GroupWidget>() {
                        if let Some(ids) = data.get("group_ids").and_then(|v| v.as_array()) {
                            grp.selected_group_ids = ids.iter()
                                .filter_map(|v| v.as_u64().map(|n| n as u32))
                                .collect();
                        }
                        grp.push_config_to_engine();
                    }
                }
            }

            // Restore Bar pattern widget state from save_data.
            if saved.type_name == "Bar" {
                if let Some(data) = &saved.data {
                    let n = graph.node_mut(idx);
                    if let Some(bar) = n.as_any_mut().downcast_mut::<BarPatternWidget>() {
                        if let Some(ids) = data.get("group_ids").and_then(|v| v.as_array()) {
                            bar.selected_group_ids = ids.iter()
                                .filter_map(|v| v.as_u64().map(|n| n as u32))
                                .collect();
                        }
                        bar.push_config_to_engine();
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
        let mut ids: Vec<u64> = indices.iter()
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
