use std::path::PathBuf;

use egui;
use serde::{Deserialize, Serialize};

use crate::widgets::nodes::{NodeGraph, NodeId, ParamValue, PortDir, PortId};

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
    pub data: Option<serde_json::Value>,
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
    fn as_f64(&self) -> f64 {
        match self {
            SavedParamValue::Float(v) => *v,
            SavedParamValue::Int(v) => *v as f64,
            SavedParamValue::Bool(v) => if *v { 1.0 } else { 0.0 },
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

pub fn save_graph(graph: &NodeGraph) -> ProjectFile {
    let mut nodes = Vec::new();

    for i in 0..graph.node_count() {
        let (node, state) = graph.node_and_state(i);

        // Save params
        let mut params = Vec::new();
        for (pi, p) in node.params().iter().enumerate() {
            let value = match p {
                crate::widgets::nodes::ParamDef::Float { value, .. } => {
                    SavedParamValue::Float(*value as f64)
                }
                crate::widgets::nodes::ParamDef::Int { value, .. } => {
                    SavedParamValue::Int(*value)
                }
                crate::widgets::nodes::ParamDef::Bool { value, .. } => {
                    SavedParamValue::Bool(*value)
                }
                crate::widgets::nodes::ParamDef::Choice { value, .. } => {
                    SavedParamValue::Choice(*value)
                }
            };
            params.push(SavedParam { index: pi, value });
        }

        nodes.push(SavedNode {
            type_name: node.type_name().to_string(),
            id: state.id.0,
            pos: [state.pos.x, state.pos.y],
            size: state.size_override.map(|s| [s.x, s.y]),
            params,
            data: node.save_data(),
        });
    }

    let connections = graph
        .connections()
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

// ---------------------------------------------------------------------------
// Load
// ---------------------------------------------------------------------------

/// Load a project file into a graph. Uses the graph's registry to create nodes.
/// Returns the indices of newly created nodes so the caller can wire them up.
pub fn load_graph(graph: &mut NodeGraph, project: &ProjectFile) -> Vec<usize> {
    let mut indices = Vec::new();

    for saved in &project.nodes {
        let id = NodeId(saved.id);
        if let Some(node) = graph.create_from_registry(&saved.type_name, id) {
            let idx = graph.add_node(
                node,
                egui::Pos2::new(saved.pos[0], saved.pos[1]),
            );

            // Apply size override.
            if let Some([w, h]) = saved.size {
                graph.set_node_size(idx, egui::Vec2::new(w, h));
            }

            // Apply params — match against the node's declared param type
            // because serde untagged deserialization may pick Float for integers.
            {
                let n = graph.node_mut(idx);
                let declared = n.params();
                for sp in &saved.params {
                    let val = if let Some(decl) = declared.get(sp.index) {
                        match decl {
                            crate::widgets::nodes::ParamDef::Float { .. } => {
                                ParamValue::Float(sp.value.as_f64() as f32)
                            }
                            crate::widgets::nodes::ParamDef::Int { .. } => {
                                ParamValue::Int(sp.value.as_f64() as i64)
                            }
                            crate::widgets::nodes::ParamDef::Bool { .. } => {
                                ParamValue::Bool(sp.value.as_f64() != 0.0)
                            }
                            crate::widgets::nodes::ParamDef::Choice { .. } => {
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
                    n.set_param(sp.index, val);
                }
                if let Some(data) = &saved.data {
                    n.load_data(data);
                }
            }

            indices.push(idx);
        } else {
            eprintln!("Unknown node type: {}", saved.type_name);
        }
    }

    // Restore connections.
    for sc in &project.connections {
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
