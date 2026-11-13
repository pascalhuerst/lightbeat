use crate::LightBeatApp;
use crate::engine;
use crate::macros;
use crate::project;
use eframe::egui;
use crate::macros::Macro;
use crate::engine::types::NodeId;
use crate::widgets::nodes::graph::MacroRequest;

/// State for the modal "Save as macro" dialog.
pub(crate) struct SaveMacroDialog {
    /// Subgraph node id whose inner graph we're saving.
    pub(crate) target_node: NodeId,
    /// Subgraph path to the target's *parent* level (i.e., the level the
    /// target node lives in). Empty = root.
    pub(crate) parent_path: Vec<NodeId>,
    pub(crate) name: String,
    pub(crate) group: String,
    pub(crate) description: String,
    /// Comma-separated tag input.
    pub(crate) tags: String,
    pub(crate) error: Option<String>,
    /// When set, "Save" overwrites this existing macro file. Name/group
    /// become read-only since the destination path is already fixed.
    pub(crate) overwrite_path: Option<std::path::PathBuf>,
}

/// Payload carried during a macro-library → canvas drag.
#[derive(Clone)]
pub(crate) struct MacroDragPayload {
    pub(crate) path: std::path::PathBuf,
}

impl LightBeatApp {
    /// Handle a macro request emitted by the right-click menu in the graph.
    pub(crate) fn handle_macro_request(&mut self, req: MacroRequest) {
        match req {
            MacroRequest::SaveAs { node_id, subgraph_path } => {
                // The selected subgraph's parent level is the current
                // subgraph_path; the inner graph we want to save belongs to
                // the target node itself.
                let _ = node_id;
                self.save_macro_dialog = Some(SaveMacroDialog {
                    target_node: node_id,
                    parent_path: subgraph_path,
                    name: String::new(),
                    group: String::new(),
                    description: String::new(),
                    tags: String::new(),
                    error: None,
                    overwrite_path: None,
                });
            }
        }
    }

    /// Validate dialog input, build the .lbm file, write it. Returns Err on
    /// failure (kept open in the dialog).
    pub(crate) fn save_pending_macro(&mut self) -> Result<(), String> {
        let dlg = self.save_macro_dialog.as_ref()
            .ok_or_else(|| "no dialog state".to_string())?;
        let target_id = dlg.target_node;
        let group = dlg.group.trim().trim_matches('/').to_string();
        let name = dlg.name.trim().to_string();
        if name.is_empty() {
            return Err("Name is required".to_string());
        }
        let tags: Vec<String> = dlg.tags.split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();

        // Find the inner graph for the target subgraph.
        let level = self.graph.find_level_for_subgraph(target_id)
            .ok_or_else(|| "subgraph has no inner level (yet)".to_string())?;
        let inner = project::save_level(level, &self.graph);

        // Capture the subgraph's external port defs so the macro can
        // re-instantiate with the same I/O shape.
        let (inputs, outputs) = self.subgraph_port_defs(target_id, &dlg.parent_path);
        let size = self.subgraph_size(target_id);

        let m = Macro {
            format_version: macros::MACRO_FORMAT_VERSION,
            name: name.clone(),
            creator: String::new(),
            date: macros::now_timestamp(),
            description: dlg.description.clone(),
            tags,
            inputs,
            outputs,
            size,
            graph: inner,
        };
        let path = match &dlg.overwrite_path {
            Some(p) => p.clone(),
            None => {
                let p = self.library.path_for(&group, &name);
                if p.exists() {
                    return Err(format!("File already exists: {}", p.display()));
                }
                p
            }
        };
        m.save_to_file(&path)?;
        self.library.rescan();
        Ok(())
    }

    /// Read the subgraph's external port defs out of its widget at the
    /// given parent path. Returns `(inputs, outputs)` — empty if not found.
    fn subgraph_port_defs(
        &self,
        target_id: NodeId,
        _parent_path: &[NodeId],
    ) -> (
        Vec<engine::nodes::meta::subgraph::SubgraphPortDef>,
        Vec<engine::nodes::meta::subgraph::SubgraphPortDef>,
    ) {
        // The macro's port defs come from the `find_level_for_subgraph` we
        // already used for the inner graph: the bridge nodes inside that
        // level mirror the subgraph's external ports.
        // But simpler: walk up to the parent level and read SubgraphWidget
        // directly. We rely on the SubgraphWidget being in the active
        // graph somewhere with the matching node id.
        for level in self.graph.all_levels() {
            for (i, n) in level.nodes.iter().enumerate() {
                if level.states[i].id != target_id { continue; }
                // Can't downcast through &dyn; need &mut. Use a different
                // route: serialize the subgraph's pending shared.save_data
                // which contains the port defs JSON.
                let shared = n.shared_state().lock();
                if let Some(data) = &shared.save_data {
                    use engine::nodes::meta::subgraph::SubgraphPortDef;
                    let inputs = data.get("inputs")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter()
                            .filter_map(|v| serde_json::from_value::<SubgraphPortDef>(v.clone()).ok())
                            .collect())
                        .unwrap_or_default();
                    let outputs = data.get("outputs")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter()
                            .filter_map(|v| serde_json::from_value::<SubgraphPortDef>(v.clone()).ok())
                            .collect())
                        .unwrap_or_default();
                    return (inputs, outputs);
                }
            }
        }
        (Vec::new(), Vec::new())
    }

    /// Walk every level looking for a Subgraph node with the given id and
    /// return its `size_override` (the user-resized size). Returns None when
    /// the user has never resized it.
    fn subgraph_size(&self, target_id: NodeId) -> Option<[f32; 2]> {
        for level in self.graph.all_levels() {
            for (i, _n) in level.nodes.iter().enumerate() {
                if level.states[i].id != target_id { continue; }
                return level.states[i].size_override.map(|s| [s.x, s.y]);
            }
        }
        None
    }

    /// Load a macro file and add it as a locked Subgraph at the given
    /// world-space position. All NodeIds in the inner graph are remapped to
    /// fresh ones to avoid collisions with existing project nodes.
    pub(crate) fn instantiate_macro_from_path(&mut self, path: &std::path::Path, canvas_pos: egui::Pos2) {
        let m = match Macro::load_from_file(path) {
            Ok(m) => m,
            Err(e) => {
                self.report_error(format!("Could not load the macro {}: {e}", path.display()));
                return;
            }
        };
        if m.format_version != macros::MACRO_FORMAT_VERSION {
            self.report_error(format!("Macro '{}' has an unsupported format version {}.", m.name, m.format_version));
        }

        // Remap all inner-graph NodeIds (recursively into nested subgraphs)
        // to fresh ones from the graph's allocator.
        let mut inner = m.graph;
        remap_project_ids(&mut inner, || self.graph.alloc_id());

        let new_id = self.graph.alloc_id();
        let data = serde_json::json!({
            "name": m.name,
            "inputs": m.inputs,
            "outputs": m.outputs,
            "locked": true,
            "macro_description": m.description,
            "macro_path": path.display().to_string(),
        });
        let saved = project::SavedNode {
            type_name: "Subgraph".to_string(),
            id: new_id.0,
            pos: [canvas_pos.x, canvas_pos.y],
            size: m.size,
            params: Vec::new(),
            data: Some(data),
            inner_graph: Some(inner),
            disabled: false,
            promoted: Vec::new(),
            hide_unused_ports: false,
            snapshots: Default::default(),
        };

        // Load this single node into the active level via the existing
        // project loader machinery (handles inner-graph descent + bridges).
        let pf = project::ProjectFile {
            version: project::PROJECT_VERSION,
            nodes: vec![saved.clone()],
            connections: Vec::new(),
            frames: Vec::new(),
            view: None,
            bridge_in_pos: None,
            bridge_out_pos: None,
        };
        let _indices = project::load_graph(&mut self.graph, &pf);

        // Spawn engine nodes + send queued commands + load_data, same as
        // load_project_from / undo apply_project does.
        self.wire_new_nodes();
        for cmd in self.graph.drain_engine_commands() {
            self.engine.send(cmd);
        }
        // The macro's nodes need their per-node data restored too (inner
        // subgraphs have data fields, etc.).
        self.send_load_data_recursive(&pf.nodes, &_indices, vec![]);
    }
}

/// Walk a `ProjectFile` (recursively into nested subgraphs) and rewrite
/// every node id + connection endpoint to a fresh id allocated via `alloc`.
/// Bridge sentinel ids (`u64::MAX` / `u64::MAX-1`) are left intact — they're
/// position-based, not identity-based.
fn remap_project_ids<F: FnMut() -> NodeId>(pf: &mut project::ProjectFile, mut alloc: F) {
    use std::collections::HashMap;
    use engine::nodes::meta::subgraph::{BRIDGE_IN_NODE_ID, BRIDGE_OUT_NODE_ID};

    fn is_bridge(id: u64) -> bool {
        id == BRIDGE_IN_NODE_ID.0 || id == BRIDGE_OUT_NODE_ID.0
    }

    fn walk<F: FnMut() -> NodeId>(pf: &mut project::ProjectFile, alloc: &mut F) {
        let mut map: HashMap<u64, u64> = HashMap::new();
        for n in &mut pf.nodes {
            if is_bridge(n.id) { continue; }
            let new = alloc().0;
            map.insert(n.id, new);
            n.id = new;
        }
        for c in &mut pf.connections {
            if let Some(&n) = map.get(&c.from_node) { c.from_node = n; }
            if let Some(&n) = map.get(&c.to_node) { c.to_node = n; }
        }
        for n in &mut pf.nodes {
            if let Some(inner) = n.inner_graph.as_mut() {
                walk(inner, alloc);
            }
        }
    }
    walk(pf, &mut alloc);
}
