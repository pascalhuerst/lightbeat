use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::engine::nodes::meta::subgraph::{BRIDGE_IN_NODE_ID, BRIDGE_OUT_NODE_ID};
use crate::engine::types::{NodeId, ParamDef, ParamValue, PortDir, PortId, PromotedParam, SnapshotSet};
use crate::widgets::nodes::{GraphLevel, NodeGraph};

// ---------------------------------------------------------------------------
// Save format
// ---------------------------------------------------------------------------

/// Bumped when a node's port layout changes in a way that would repoint
/// saved connections. Files without the field load as version 0.
///
/// 1: hardware nodes gained a leading "connected" output.
/// 2: Change Detect has its value first.
/// 3: Envelope and Transition have their signal first and the trigger after.
///
/// TEMPORARY: the migration exists only so the developers' test projects
/// survive one load. Once they are re-saved, delete `STATUS_PORT_NODE_TYPES`,
/// `connections_for_current_layout`, its tests and the `version` field.
pub const PROJECT_VERSION: u32 = 3;

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
/// Empty for now. Old files carry a `show_library` flag here; serde ignores
/// unknown fields, so they still load.
pub struct ProjectViewState {}

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
    /// Named sets of the node's settings (see `engine::snapshots`).
    #[serde(default, skip_serializing_if = "SnapshotSet::is_empty")]
    pub snapshots: SnapshotSet,
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
        let shared = node.shared_state().lock();
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
        let mut snapshots = shared.snapshots.clone();
        snapshots.active = shared.snapshot_active.or(snapshots.active);
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
            snapshots,
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
    if project.version < 3 {
        for c in &mut connections {
            let Some(node) = project.nodes.iter().find(|n| n.id == c.to_node) else { continue };
            // A file this old may still call the Envelope "ADSR".
            match current_type_name(&node.type_name) {
                // Was (gate, in).
                "Envelope" if c.to_port < 2 => c.to_port = 1 - c.to_port,
                // Was (trigger, phase, in); Phase out has no `in` and kept (trigger, phase).
                "Transition" => {
                    let phase_out = node.params.iter().find(|p| p.index == 0)
                        .is_some_and(|p| p.value.as_f64() == 4.0);
                    if !phase_out {
                        c.to_port = match c.to_port {
                            0 => 1,
                            1 => 2,
                            2 => 0,
                            other => other,
                        };
                    }
                }
                _ => {}
            }
        }
    }
    connections
}

/// Names nodes were saved under before they were renamed, with the name they
/// have now. Every path that makes nodes from saved data (project load, macros,
/// paste of an inner graph) goes through `load_graph`, so this is the one place.
/// Temporary: drop an entry once the test projects have been saved again.
pub(crate) const RENAMED_NODES: [(&str, &str); 8] = [
    // Renamed when the node gained several envelope kinds.
    ("ADSR", "Envelope"),
    ("Position Merge", "Pan Tilt to Position"),
    ("Position Split", "Position to Pan Tilt"),
    ("Latch", "Tick Delay"),
    ("Flip-Flop", "SR Flip-Flop"),
    ("Clock", "Link Clock"),
    ("Scaler", "Range to Unit"),
    ("Bar Loop", "Bar"),
];

/// Settings that were renamed: (node, old name, new name). Promoted inputs and
/// snapshots refer to a setting by name, so they are carried over on load.
/// Temporary, like `RENAMED_NODES`.
const RENAMED_SETTINGS: [(&str, &str, &str); 7] = [
    ("Lookup Table", "Input", "Select by"),
    ("Fill", "Intensity", "amount"),
    ("Bar", "Intensity", "amount"),
    ("Wave Loop", "Intensity", "amount"),
    ("Ripple Burst", "Intensity", "amount"),
    ("Spark Burst", "Intensity", "amount"),
    ("Ripple Burst", "Decay", "Lifetime"),
];

fn current_setting_name<'a>(node: &str, saved: &'a str) -> &'a str {
    RENAMED_SETTINGS
        .iter()
        .find(|(n, old, _)| *n == node && *old == saved)
        .map_or(saved, |(_, _, new)| new)
}

fn current_type_name(saved: &str) -> &str {
    RENAMED_NODES.iter().find(|(old, _)| *old == saved).map_or(saved, |(_, new)| new)
}

/// Sin and Cos were removed in favour of the LFO driven by a phase wire. Their
/// ports line up with the LFO's (phase, then the promoted Amplitude), so the
/// saved wires need no change. Temporary, like `RENAMED_NODES`.
fn with_oscillators_as_lfos(project: &ProjectFile) -> Option<ProjectFile> {
    let is_oscillator = |n: &SavedNode| n.type_name == "Sin" || n.type_name == "Cos";
    if !project.nodes.iter().any(is_oscillator) {
        return None;
    }
    let mut project = project.clone();
    for node in project.nodes.iter_mut().filter(|n| is_oscillator(n)) {
        let cosine = node.type_name == "Cos";
        node.type_name = "LFO".to_string();
        node.params = vec![
            SavedParam { index: 1, value: SavedParamValue::Choice(if cosine { 6 } else { 0 }) },
            SavedParam { index: 3, value: SavedParamValue::Bool(true) },
        ];
        node.promoted = vec![PromotedParam { name: "Amplitude".to_string(), kind: crate::engine::promoted::PromotedKind::Float }];
    }
    Some(project)
}

pub fn load_graph(graph: &mut NodeGraph, project: &ProjectFile) -> Vec<usize> {
    let mut indices = Vec::new();
    let converted = with_oscillators_as_lfos(project);
    let project = converted.as_ref().unwrap_or(project);

    let project_connections = connections_for_current_layout(project);

    for saved in &project.nodes {
        let id = NodeId(saved.id);
        let type_name = current_type_name(&saved.type_name);
        if let Some(node) = graph.create_from_registry(type_name, id) {
            let idx = graph.add_node(node, egui::Pos2::new(saved.pos[0], saved.pos[1]));

            if let Some([w, h]) = saved.size {
                graph.set_node_size(idx, egui::Vec2::new(w, h));
            }

            // Apply params via shared state.
            let mut restore_params: Vec<(usize, ParamValue)> = Vec::new();
            {
                let n = graph.node_mut(idx);
                let declared = {
                    let shared = n.shared_state().lock();
                    shared.current_params.clone()
                };
                let mut shared = n.shared_state().lock();
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
                    shared.pending_params.push((sp.index, val.clone()));
                    restore_params.push((sp.index, val));
                }
                shared.disabled = saved.disabled;
                shared.hide_unused_ports = saved.hide_unused_ports;
                // Set before connections are restored so the extra ports exist.
                let mut promoted = saved.promoted.clone();
                for p in &mut promoted {
                    p.name = current_setting_name(type_name, &p.name).to_string();
                }
                shared.promoted = promoted.clone();
                shared.promoted_applied = promoted.clone();
                if !promoted.is_empty() {
                    shared.pending_promoted = Some(promoted);
                }
                if !saved.snapshots.is_empty() {
                    let mut snapshots = saved.snapshots.clone();
                    for snap in &mut snapshots.snapshots {
                        for (name, _) in &mut snap.values {
                            *name = current_setting_name(type_name, name).to_string();
                        }
                    }
                    shared.snapshots = snapshots.clone();
                    shared.pending_snapshots = Some(snapshots);
                }
            }

            // Widgets whose ports follow their own state need it before the
            // saved wires are attached.
            graph.node_mut(idx).restore_from_save(saved.data.as_ref(), &restore_params);

            if saved.type_name == "Subgraph"
                && let Some(inner_project) = &saved.inner_graph {
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
    crate::paths::data_file("project.json")
}

/// Path for the crash-recovery autosave file that shadows a given project
/// file. Convention: same directory as the project, filename is the project's
/// stem with `.autosave.json` appended and a leading `.` (hidden on unix).
/// If `project_path` is `None`, falls back to the data directory.
pub fn autosave_path_for(project_path: Option<&std::path::Path>) -> PathBuf {
    match project_path {
        Some(p) => {
            let dir = p.parent().unwrap_or(std::path::Path::new(""));
            let stem = p.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("project");
            dir.join(format!(".{}.autosave.json", stem))
        }
        None => crate::paths::data_file(".project.autosave.json"),
    }
}

pub fn save_to_file(
    graph: &NodeGraph,
    view: ProjectViewState,
    path: &std::path::Path,
) -> Result<(), String> {
    let mut project = save_graph(graph);
    project.view = Some(view);
    let json = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    crate::paths::write_atomic(path, json).map_err(|e| e.to_string())
}

pub fn load_from_file(path: &std::path::Path) -> Result<ProjectFile, String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_view_state_with_show_library_still_loads() {
        let json = r#"{"version":1,"nodes":[],"connections":[],"view":{"show_library":false}}"#;
        let p: ProjectFile = serde_json::from_str(json).expect("old project loads");
        assert!(p.view.is_some());
    }

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
            snapshots: Default::default(),
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

    fn with_wires_to(type_name: &str, version: u32, ports: &[usize], mode: Option<usize>) -> ProjectFile {
        let mut p = project(version);
        let mut n = node(3, type_name);
        if let Some(m) = mode {
            n.params.push(SavedParam { index: 0, value: SavedParamValue::Choice(m) });
        }
        p.nodes.push(n);
        p.connections = ports
            .iter()
            .map(|&to_port| SavedConnection { from_node: 2, from_port: 0, to_node: 3, to_port })
            .collect();
        p
    }

    fn to_ports(p: &ProjectFile) -> Vec<usize> {
        connections_for_current_layout(p).iter().map(|c| c.to_port).collect()
    }

    #[test]
    fn envelope_wires_follow_its_swapped_inputs() {
        // Saved (gate, in); now (in, gate).
        assert_eq!(to_ports(&with_wires_to("Envelope", 2, &[0, 1], Some(0))), vec![1, 0]);
        assert_eq!(to_ports(&with_wires_to("Envelope", 2, &[0, 1], Some(3))), vec![1, 0]);
        // No version at all is the oldest, and is remapped once.
        assert_eq!(to_ports(&with_wires_to("Envelope", 0, &[0], None)), vec![1]);
        assert_eq!(to_ports(&with_wires_to("ADSR", 0, &[0], None)), vec![1]);
        assert_eq!(to_ports(&with_wires_to("Envelope", PROJECT_VERSION, &[0, 1], Some(0))), vec![0, 1]);
    }

    #[test]
    fn transition_wires_follow_its_new_order_in_every_mode() {
        // Saved (trigger, phase, in); now (in, trigger, phase).
        for mode in [0, 1, 2, 3, 5, 6] {
            assert_eq!(to_ports(&with_wires_to("Transition", 2, &[0, 1, 2], Some(mode))), vec![1, 2, 0], "mode {mode}");
        }
        // A saved node without the setting is in its default (Auto) mode.
        assert_eq!(to_ports(&with_wires_to("Transition", 2, &[0, 1, 2], None)), vec![1, 2, 0]);
        // Phase out has no value input and keeps (trigger, phase).
        assert_eq!(to_ports(&with_wires_to("Transition", 2, &[0, 1], Some(4))), vec![0, 1]);
        assert_eq!(to_ports(&with_wires_to("Transition", PROJECT_VERSION, &[0, 1, 2], Some(1))), vec![0, 1, 2]);
        // Without a version the node is remapped once, not twice.
        let mut p = with_wires_to("Transition", 0, &[0, 1, 2], Some(1));
        assert_eq!(to_ports(&p), vec![1, 2, 0]);
        p.version = PROJECT_VERSION;
        assert_eq!(to_ports(&p), vec![0, 1, 2]);
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
            g.register_node("Effect", "Effect Stack", move |id| {
                Box::new(EffectStackWidget::new(id, crate::engine::types::new_shared_state(3, 0), ctx.clone(), stack_names.clone()))
            });
            g.register_node("Effect", "Ripple Burst", move |id| {
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
        let sent = |g: &mut NodeGraph, i: usize| g.node_mut(i).shared_state().lock().pending_config.clone();
        let (stack_data, effect_data) = (sent(&mut a, si), sent(&mut a, ei));
        for (i, d) in [(si, stack_data), (ei, effect_data)] {
            a.node_mut(i).shared_state().lock().save_data = d;
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
    fn a_bar_saved_as_bar_loop_with_promoted_inputs_loads_from_its_saved_form() {
        use crate::widgets::nodes::output::effect::{BAR_LOOP, EffectWidget, new_shared_stack_names};
        use crate::widgets::nodes::NodeGraph;

        let mut g = NodeGraph::new();
        let names = new_shared_stack_names();
        g.register_node("Effect", "Bar", move |id| {
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
        assert_eq!(ports, ["gradient", "Position", "Width"]);
        let n = g.node_mut(0);
        assert_eq!(n.as_any_mut().downcast_mut::<EffectWidget>().unwrap().stack(), "Record Shelf");
        let shared = n.shared_state().lock();
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
        g.register_node("Clock", "Phase Scaler", |id| {
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
        g.register_node("Input", "MIDI Input", move |id| {
            Box::new(MidiInputWidget::new(id, crate::engine::types::new_shared_state(0, 33), h1.clone(), l1.clone()))
        });
        g.register_node("Output", "MIDI Output", move |id| {
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
    fn wires_from_dmx_input_entries_survive_loading() {
        use crate::dmx_input::DmxInputHub;
        use crate::widgets::nodes::display::value_display::ValueDisplayWidget;
        use crate::widgets::nodes::io::dmx_input::DmxInputWidget;
        use crate::widgets::nodes::NodeGraph;

        let mut g = NodeGraph::new();
        let hub = DmxInputHub::idle();
        g.register_node("Input", "DMX Input", move |id| {
            Box::new(DmxInputWidget::new(id, crate::engine::types::new_shared_state(0, 200), hub.clone()))
        });
        g.register_node("Display", "Value Display", |id| {
            Box::new(ValueDisplayWidget::new(id, crate::engine::types::new_shared_state(1, 0)))
        });
        let project: ProjectFile = serde_json::from_str(r#"{
            "nodes": [
                {"type_name": "DMX Input", "id": 1, "pos": [0.0, 0.0], "params": [],
                 "data": {"input": 1, "outputs": [
                    {"id": 1, "name": "a", "kind": "value", "sources": [{"address": 1}]},
                    {"id": 2, "name": "b", "kind": "gate", "sources": [{"address": 2}]}]}},
                {"type_name": "Value Display", "id": 2, "pos": [200.0, 0.0], "params": []}
            ],
            "connections": [{"from_node": 1, "from_port": 2, "to_node": 2, "to_port": 0}]
        }"#).unwrap();
        load_graph(&mut g, &project);
        g.cleanup_stale_connections();
        assert_eq!(g.node_and_state(0).0.ui_outputs().len(), 3);
        assert_eq!(g.root_level().connections.len(), 1, "the wire from the second channel was kept");
    }

    fn typed_signal_graph() -> crate::widgets::nodes::NodeGraph {
        use crate::engine::types::new_shared_state;
        use crate::widgets::nodes::math::constant::ConstantWidget;
        use crate::widgets::nodes::transport::latch::LatchWidget;
        use crate::widgets::nodes::transport::sample_hold::SampleHoldWidget;
        use crate::widgets::nodes::transport::smoothing::SmoothingWidget;
        use crate::widgets::nodes::NodeGraph;

        let mut g = NodeGraph::new();
        g.register_node("Signal", "Const Value", |id| {
            Box::new(ConstantWidget::new(id, crate::engine::types::PortType::Untyped, new_shared_state(0, 1)))
        });
        g.register_node("Signal", "Sample & Hold", |id| Box::new(SampleHoldWidget::new(id, new_shared_state(41, 40))));
        g.register_node("Signal", "Smoothing", |id| Box::new(SmoothingWidget::new(id, new_shared_state(40, 40))));
        g.register_node("Trigger", "Tick Delay", |id| Box::new(LatchWidget::new(id, new_shared_state(40, 40))));
        g
    }

    fn operation_graph() -> crate::widgets::nodes::NodeGraph {
        use crate::engine::nodes::math::compare::CompareOp;
        use crate::engine::nodes::math::logic_gate::LogicOp;
        use crate::engine::nodes::math::math_op::MathOp;
        use crate::engine::types::new_shared_state;
        use crate::widgets::nodes::math::compare::CompareWidget;
        use crate::widgets::nodes::math::constant::ConstantWidget;
        use crate::widgets::nodes::math::logic_gate::LogicGateWidget;
        use crate::widgets::nodes::math::math_op::MathWidget;
        use crate::widgets::nodes::NodeGraph;

        let mut g = NodeGraph::new();
        for (label, op) in [("Add", MathOp::Add), ("Sub", MathOp::Sub), ("Mul", MathOp::Mul), ("Div", MathOp::Div)] {
            g.register_node("Math", label, move |id| Box::new(MathWidget::new(id, op, new_shared_state(16, 1))));
        }
        for (label, op) in [(">=", CompareOp::Gte), ("<=", CompareOp::Lte), ("==", CompareOp::Eq), ("!=", CompareOp::Neq)] {
            g.register_node("Compare", label, move |id| Box::new(CompareWidget::new(id, op, new_shared_state(2, 1))));
        }
        for (label, op) in [("AND", LogicOp::And), ("OR", LogicOp::Or), ("XOR", LogicOp::Xor)] {
            g.register_node("Logic", label, move |id| Box::new(LogicGateWidget::new(id, op, new_shared_state(16, 1))));
        }
        for (label, ty) in [
            ("Const Value", crate::engine::types::PortType::Untyped),
            ("Const Logic", crate::engine::types::PortType::Logic),
            ("Const Phase", crate::engine::types::PortType::Phase),
        ] {
            g.register_node("Signal", label, move |id| Box::new(ConstantWidget::new(id, ty, new_shared_state(0, 1))));
        }
        g
    }

    /// What the engine mirrors back after "Operation" (the param at `index`)
    /// was set to `choice`.
    fn mirror_operation(g: &mut crate::widgets::nodes::NodeGraph, node: usize, index: usize, choice: usize) {
        let mut params = vec![ParamDef::Float { name: "Value".into(), value: 0.0, min: 0.0, max: 1.0, step: 0.1, unit: "" }];
        params.truncate(index);
        params.push(ParamDef::Choice { name: "Operation".into(), value: choice, options: Vec::new() });
        g.node_mut(node).shared_state().lock().current_params = params;
    }

    #[test]
    fn old_sin_and_cos_nodes_load_as_phase_driven_lfos_with_amplitude_promoted() {
        let project: ProjectFile = serde_json::from_str(r#"{
            "version": 4,
            "nodes": [
                {"type_name": "Sin", "id": 1, "pos": [0.0, 0.0], "params": []},
                {"type_name": "Cos", "id": 2, "pos": [0.0, 0.0], "params": []},
                {"type_name": "Add", "id": 3, "pos": [0.0, 0.0], "params": []}
            ],
            "connections": []
        }"#).unwrap();
        let converted = with_oscillators_as_lfos(&project).unwrap();
        let types: Vec<&str> = converted.nodes.iter().map(|n| n.type_name.as_str()).collect();
        assert_eq!(types, ["LFO", "LFO", "Add"]);
        assert!(converted.nodes[0].params.contains(&SavedParam { index: 1, value: SavedParamValue::Choice(0) }));
        assert!(converted.nodes[1].params.contains(&SavedParam { index: 1, value: SavedParamValue::Choice(6) }));
        assert_eq!(converted.nodes[0].promoted[0].name, "Amplitude");
        assert!(converted.nodes[2].promoted.is_empty());
    }

    #[test]
    fn switching_the_operation_keeps_position_and_wires_and_saves_the_siblings_name() {
        let mut g = operation_graph();
        let project: ProjectFile = serde_json::from_str(&format!(r#"{{
            "version": {PROJECT_VERSION},
            "nodes": [
                {{"type_name": "Const Value", "id": 1, "pos": [0.0, 0.0], "params": []}},
                {{"type_name": "Add", "id": 2, "pos": [200.0, 40.0], "params": []}},
                {{"type_name": ">=", "id": 3, "pos": [400.0, 40.0], "params": []}},
                {{"type_name": "AND", "id": 4, "pos": [600.0, 40.0], "params": []}}
            ],
            "connections": [
                {{"from_node": 1, "from_port": 0, "to_node": 2, "to_port": 0}},
                {{"from_node": 2, "from_port": 0, "to_node": 3, "to_port": 1}},
                {{"from_node": 3, "from_port": 0, "to_node": 4, "to_port": 1}}
            ]
        }}"#)).unwrap();
        load_graph(&mut g, &project);
        g.cleanup_stale_connections();
        assert_eq!(g.root_level().connections.len(), 3);

        // Add to Div, >= to !=, AND to XOR, as the engine would report them.
        for (idx, choice) in [(1, 3), (2, 3), (3, 2)] {
            mirror_operation(&mut g, idx, 0, choice);
            g.node_mut(idx).follow_params();
        }
        g.cleanup_stale_connections();

        let saved = save_graph(&g);
        let by_id = |id: u64| saved.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(by_id(2).type_name, "Div");
        assert_eq!(by_id(3).type_name, "!=");
        assert_eq!(by_id(4).type_name, "XOR");
        assert_eq!(by_id(2).pos, [200.0, 40.0]);
        assert_eq!(saved.connections.len(), 3, "no wire was dropped");

        // The saved file loads back as the new operations, wires intact.
        let mut g2 = operation_graph();
        load_graph(&mut g2, &saved);
        g2.cleanup_stale_connections();
        assert_eq!(g2.root_level().connections.len(), 3);
        let again = save_graph(&g2);
        assert_eq!(again.nodes.iter().map(|n| n.type_name.as_str()).collect::<Vec<_>>(), ["Const Value", "Div", "!=", "XOR"]);
    }

    #[test]
    fn a_constant_switched_to_another_kind_keeps_only_the_wires_that_still_fit() {
        let mut g = operation_graph();
        let project: ProjectFile = serde_json::from_str(&format!(r#"{{
            "version": {PROJECT_VERSION},
            "nodes": [
                {{"type_name": "Const Logic", "id": 1, "pos": [0.0, 0.0], "params": []}},
                {{"type_name": "Const Value", "id": 5, "pos": [0.0, 200.0], "params": []}},
                {{"type_name": ">=", "id": 2, "pos": [200.0, 0.0], "params": []}},
                {{"type_name": "AND", "id": 3, "pos": [200.0, 100.0], "params": []}}
            ],
            "connections": [
                {{"from_node": 5, "from_port": 0, "to_node": 2, "to_port": 0}},
                {{"from_node": 1, "from_port": 0, "to_node": 3, "to_port": 0}}
            ]
        }}"#)).unwrap();
        load_graph(&mut g, &project);
        g.cleanup_stale_connections();
        assert_eq!(g.root_level().connections.len(), 2, "both wires fit to begin with");
        // The logic constant becomes a number; the other one is left alone.
        mirror_operation(&mut g, 0, 1, 0);
        g.node_mut(0).follow_params();
        g.cleanup_stale_connections();
        let saved = save_graph(&g);
        assert_eq!(saved.nodes[0].type_name, "Const Value");
        assert!(!g.root_level().connections.iter().any(|c| c.to.node == NodeId(3)), "a number no longer feeds the gate");
        assert!(g.root_level().connections.iter().any(|c| c.to.node == NodeId(2)), "the number wire that was not touched stays");
    }

    #[test]
    fn a_renamed_setting_keeps_its_promotion_and_snapshots_by_name() {
        assert_eq!(current_setting_name("Lookup Table", "Input"), "Select by");
        assert_eq!(current_setting_name("Lookup Table", "Edge"), "Edge");
        assert_eq!(current_setting_name("Smoothing", "Input"), "Input");
    }

    #[test]
    fn the_ripples_decay_is_now_lifetime() {
        assert_eq!(current_setting_name("Ripple Burst", "Decay"), "Lifetime");
        assert_eq!(current_setting_name("Envelope", "Decay"), "Decay");
    }

    #[test]
    fn the_effects_intensity_is_now_amount() {
        for effect in ["Fill", "Bar", "Wave Loop", "Ripple Burst", "Spark Burst"] {
            assert_eq!(current_setting_name(effect, "Intensity"), "amount");
        }
        assert_eq!(current_setting_name("Fill", "Width"), "Width");
        assert_eq!(current_setting_name("Smoothing", "Intensity"), "Intensity");
    }

    #[test]
    fn nodes_saved_before_they_had_a_type_come_back_as_numbers_with_their_wires() {
        let mut g = typed_signal_graph();
        // No "Type" setting in the file; every wire carried a plain number.
        let project: ProjectFile = serde_json::from_str(r#"{
            "nodes": [
                {"type_name": "Const Value", "id": 1, "pos": [0.0, 0.0], "params": []},
                {"type_name": "Sample & Hold", "id": 2, "pos": [100.0, 0.0], "params": [], "data": {"held": 0.5}},
                {"type_name": "Smoothing", "id": 3, "pos": [200.0, 0.0], "params": [{"index": 0, "value": 1}, {"index": 1, "value": 8}]},
                {"type_name": "Tick Delay", "id": 4, "pos": [300.0, 0.0], "params": []}
            ],
            "connections": [
                {"from_node": 1, "from_port": 0, "to_node": 2, "to_port": 0},
                {"from_node": 2, "from_port": 0, "to_node": 3, "to_port": 0},
                {"from_node": 3, "from_port": 0, "to_node": 4, "to_port": 0}
            ]
        }"#).unwrap();
        load_graph(&mut g, &project);
        g.cleanup_stale_connections();
        assert_eq!(g.root_level().connections.len(), 3, "no wire was dropped");
        let number = crate::engine::nodes::math::multiplex::type_to_index(crate::engine::types::PortType::Untyped);
        for (idx, type_param) in [(1, 0), (2, 2), (3, 0)] {
            let node = g.node_mut(idx);
            assert_eq!(node.ui_inputs()[0].def.port_type, crate::engine::types::PortType::Untyped, "node {idx}");
            assert_eq!(node.ui_outputs()[0].def.port_type, crate::engine::types::PortType::Untyped, "node {idx}");
            let shared = node.shared_state().lock();
            assert!(
                shared.pending_params.iter().any(|(i, v)| *i == type_param && v.as_usize() == number),
                "node {idx} was told to run as numbers"
            );
        }
    }

    #[test]
    fn a_node_with_a_saved_type_keeps_it_and_its_wires() {
        let mut g = typed_signal_graph();
        let project: ProjectFile = serde_json::from_str(r#"{
            "nodes": [
                {"type_name": "Tick Delay", "id": 1, "pos": [0.0, 0.0], "params": [{"index": 0, "value": 4}]},
                {"type_name": "Smoothing", "id": 2, "pos": [100.0, 0.0], "params": [{"index": 2, "value": 4}]}
            ],
            "connections": [{"from_node": 1, "from_port": 0, "to_node": 2, "to_port": 0}]
        }"#).unwrap();
        load_graph(&mut g, &project);
        g.cleanup_stale_connections();
        assert_eq!(g.node_mut(0).ui_outputs()[0].def.port_type, crate::engine::types::PortType::Color);
        assert_eq!(g.node_mut(1).ui_inputs()[0].def.port_type, crate::engine::types::PortType::Color);
        assert_eq!(g.root_level().connections.len(), 1);
    }

    #[test]
    fn hiding_unused_ports_is_saved_and_restored() {
        use crate::widgets::nodes::math::constant::ConstantWidget;
        use crate::widgets::nodes::NodeGraph;

        let mut g = NodeGraph::new();
        g.register_node("Signal", "Const Value", |id| {
            Box::new(ConstantWidget::new(id, crate::engine::types::PortType::Untyped, crate::engine::types::new_shared_state(0, 1)))
        });
        let project: ProjectFile = serde_json::from_str(r#"{
            "nodes": [{"type_name": "Const Value", "id": 1, "pos": [0.0, 0.0], "params": [], "hide_unused_ports": true}],
            "connections": []
        }"#).unwrap();
        load_graph(&mut g, &project);
        assert!(g.node_mut(0).shared_state().lock().hide_unused_ports);
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
