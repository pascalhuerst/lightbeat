//! The graph's side of linked subgraphs (see `crate::linked`): definition
//! identities of the nodes inside, making and breaking links, applying a sync
//! step to the levels, the instance counts and the banner.

use std::collections::{HashMap, HashSet};

use super::*;
use crate::linked::{self, ParamNames, SyncStep};
use crate::project::{self, ProjectFile, SavedNode};
use crate::widgets::nodes::meta::subgraph::{LibraryState, SubgraphRequest};

pub(super) fn is_bridge(id: NodeId) -> bool {
    id == BRIDGE_IN_NODE_ID || id == BRIDGE_OUT_NODE_ID
}

fn definition_in(data: Option<&serde_json::Value>) -> Option<u64> {
    data?.get("definition")?.as_u64()
}

impl NodeGraph {
    /// Messages for the user that came up inside the graph (a refused paste).
    pub fn take_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notices)
    }

    pub(super) fn level_of_subgraph(&self, id: NodeId) -> Option<usize> {
        self.levels.iter().position(|l| l.subgraph_id == Some(id))
    }

    /// The definition of the Subgraph node `index` on `level`, if it is linked.
    pub(super) fn definition_at(&self, level: usize, index: usize) -> Option<u64> {
        let node = &self.levels[level].nodes[index];
        if node.type_name() != "Subgraph" {
            return None;
        }
        definition_in(node.live_data().as_ref())
    }

    /// Definitions of the subgraphs the level is inside of, outermost first.
    pub(super) fn ancestor_definitions(&self, level: usize) -> Vec<u64> {
        let mut out = Vec::new();
        let mut cur = level;
        while let Some(parent) = self.levels[cur].parent_level_idx {
            if let Some(sid) = self.levels[cur].subgraph_id
                && let Some(i) = self.levels[parent].nodes.iter().position(|n| n.node_id() == sid)
                && let Some(d) = self.definition_at(parent, i)
            {
                out.push(d);
            }
            cur = parent;
        }
        out.reverse();
        out
    }

    /// Levels that are reachable from the root (a deleted subgraph's level stays in the list).
    pub(super) fn reachable_levels(&self) -> Vec<usize> {
        (0..self.levels.len()).filter(|&li| self.level_chain(li).is_some()).collect()
    }

    /// How many subgraphs are instances of each definition.
    pub fn instance_counts(&self) -> HashMap<u64, usize> {
        let mut counts = HashMap::new();
        for li in self.reachable_levels() {
            for i in 0..self.levels[li].nodes.len() {
                if let Some(d) = self.definition_at(li, i) {
                    *counts.entry(d).or_insert(0) += 1;
                }
            }
        }
        counts
    }

    /// Tell every Subgraph node how many instances it has.
    pub(super) fn refresh_instance_counts(&mut self) {
        let counts = self.instance_counts();
        for li in 0..self.levels.len() {
            for i in 0..self.levels[li].nodes.len() {
                if self.levels[li].nodes[i].type_name() != "Subgraph" {
                    continue;
                }
                let n = self.definition_at(li, i).and_then(|d| counts.get(&d).copied()).unwrap_or(1);
                self.levels[li].nodes[i].set_instance_count(n);
            }
        }
    }

    /// Carry out what the inspector of a selected Subgraph asked for.
    pub(super) fn handle_subgraph_requests(&mut self) {
        let mut asked: Vec<(NodeId, SubgraphRequest)> = Vec::new();
        let level = &mut self.levels[self.active_level];
        for (node, state) in level.nodes.iter_mut().zip(level.states.iter()) {
            if let Some(w) = node.as_any_mut().downcast_mut::<SubgraphWidget>()
                && let Some(r) = w.request.take()
            {
                asked.push((state.id, r));
            }
        }
        for (id, request) in asked {
            match request {
                SubgraphRequest::MakeUnique => self.make_unique(id),
                SubgraphRequest::UpdateFromLibrary => {
                    self.pending_macro_request = Some(MacroRequest::UpdateFromLibrary { node_id: id });
                }
            }
        }
    }

    /// The other instances of the Subgraph at `index` of `level`, for a jump.
    pub(super) fn instance_peers_of(&self, level: usize, index: usize) -> Vec<LinkedNode> {
        let Some(def) = self.definition_at(level, index) else { return Vec::new() };
        let own = self.levels[level].nodes[index].node_id();
        let mut peers = Vec::new();
        for li in self.reachable_levels() {
            let Some(titles) = self.level_titles(li) else { continue };
            let prefix: String = titles.iter().map(|t| format!("{t} / ")).collect();
            for (i, n) in self.levels[li].nodes.iter().enumerate() {
                if n.node_id() != own && self.definition_at(li, i) == Some(def) {
                    peers.push(LinkedNode { node_id: n.node_id(), path: format!("{prefix}{}", n.display_title()) });
                }
            }
        }
        peers
    }

    /// Whether any instance of `definition` exists on a reachable level.
    pub fn definition_exists(&self, definition: u64) -> bool {
        self.instance_counts().contains_key(&definition)
    }

    /// The definition of the macro placed from `path` with this `hash`, if
    /// one is in the project: placing it again makes another instance.
    pub fn macro_definition(&self, path: &str, hash: &str) -> Option<u64> {
        for li in self.reachable_levels() {
            for (i, n) in self.levels[li].nodes.iter().enumerate() {
                if n.type_name() != "Subgraph" {
                    continue;
                }
                let Some(data) = n.live_data() else { continue };
                if data.get("macro_path").and_then(|v| v.as_str()) == Some(path)
                    && data.get("macro_hash").and_then(|v| v.as_str()) == Some(hash)
                    && self.definition_at(li, i).is_some()
                {
                    return self.definition_at(li, i);
                }
            }
        }
        None
    }

    /// The one selected Subgraph that was placed from the library: its id,
    /// the file it came from and the hash it had.
    pub fn selected_macro(&self) -> Option<(NodeId, String, Option<String>)> {
        let [i] = self.selected_nodes[..] else { return None };
        let n = self.levels[self.active_level].nodes.get(i)?;
        if n.type_name() != "Subgraph" {
            return None;
        }
        let data = n.live_data()?;
        let path = data.get("macro_path")?.as_str()?.to_string();
        if path.is_empty() {
            return None;
        }
        let hash = data.get("macro_hash").and_then(|v| v.as_str()).map(str::to_string);
        Some((n.node_id(), path, hash))
    }

    pub fn set_library_state(&mut self, id: NodeId, state: LibraryState) {
        for level in &mut self.levels {
            if let Some(i) = level.states.iter().position(|s| s.id == id) {
                if let Some(w) = level.nodes[i].as_any_mut().downcast_mut::<SubgraphWidget>() {
                    w.library_state = state;
                }
                return;
            }
        }
    }

    /// The path of Subgraph ids from the root to `id`, for a sync step.
    pub fn path_to_subgraph(&self, id: NodeId) -> Option<Vec<u64>> {
        let project = project::save_graph(self);
        self.saved_path_of(&project, id)
    }

    /// The definition the open level is the inside of, with its name and how
    /// many instances use it.
    pub(super) fn open_definition(&self) -> Option<(String, usize)> {
        let level = &self.levels[self.active_level];
        let parent = level.parent_level_idx?;
        let sid = level.subgraph_id?;
        let i = self.levels[parent].nodes.iter().position(|n| n.node_id() == sid)?;
        let def = self.definition_at(parent, i)?;
        let count = self.instance_counts().get(&def).copied().unwrap_or(1);
        Some((self.levels[parent].nodes[i].display_title().to_string(), count))
    }

    /// Param names of every node, by id: a panel item names a setting and a
    /// saved node only has its index.
    pub fn param_names(&self) -> ParamNames {
        let mut out = ParamNames::new();
        for level in &self.levels {
            for n in &level.nodes {
                let shared = n.shared_state().lock();
                if !shared.current_params.is_empty() {
                    out.insert(n.node_id().0, shared.current_params.iter().map(|p| p.name().to_string()).collect());
                }
            }
        }
        out
    }

    // -----------------------------------------------------------------------
    // Identities and tags
    // -----------------------------------------------------------------------

    /// Give the nodes inside linked subgraphs the identity they have in the
    /// definition (their own id unless they have one). A node added to an
    /// instance later gets its identity here. True when something changed.
    pub fn assign_def_nodes(&mut self) -> bool {
        let mut changed = false;
        self.assign_level(0, false, &mut changed, 0);
        changed
    }

    fn assign_level(&mut self, level: usize, linked: bool, changed: &mut bool, depth: usize) {
        if depth > 64 {
            return;
        }
        for i in 0..self.levels[level].nodes.len() {
            let id = self.levels[level].states[i].id;
            if is_bridge(id) {
                continue;
            }
            if linked {
                let mut shared = self.levels[level].nodes[i].shared_state().lock();
                if shared.def_node.is_none() {
                    shared.def_node = Some(id.0);
                    *changed = true;
                }
            }
            if self.levels[level].nodes[i].type_name() == "Subgraph"
                && let Some(inner) = self.level_of_subgraph(id)
            {
                let below = linked || self.definition_at(level, i).is_some();
                self.assign_level(inner, below, changed, depth + 1);
            }
        }
    }

    /// Set the definition tag of the Subgraph `id`, wherever it is.
    fn set_tag(&mut self, id: NodeId, definition: Option<u64>) {
        for level in &mut self.levels {
            let Some(i) = level.states.iter().position(|s| s.id == id) else { continue };
            if let Some(w) = level.nodes[i].as_any_mut().downcast_mut::<SubgraphWidget>() {
                w.definition = definition;
                w.push_config();
            }
            return;
        }
    }

    /// Make the Subgraph `id` the first instance of a new definition (unless
    /// it is one already): its definition is its own id, and the nodes inside
    /// get their identities.
    pub(super) fn ensure_definition(&mut self, id: NodeId) -> Option<u64> {
        let level = self.levels.iter().position(|l| l.states.iter().any(|s| s.id == id))?;
        let index = self.levels[level].states.iter().position(|s| s.id == id)?;
        if let Some(d) = self.definition_at(level, index) {
            return Some(d);
        }
        let definition = crate::linked::new_definition_id();
        self.set_tag(id, Some(definition));
        let mut changed = false;
        if let Some(inner) = self.level_of_subgraph(id) {
            self.assign_level(inner, true, &mut changed, 0);
        }
        Some(definition)
    }

    /// Carry the tags and identities of `saved` over to the live levels below
    /// `level`, matching nodes by id.
    pub fn apply_tags(&mut self, level: usize, saved: &ProjectFile) {
        for sn in &saved.nodes {
            let id = NodeId(sn.id);
            let Some(i) = self.levels[level].states.iter().position(|s| s.id == id) else { continue };
            self.levels[level].nodes[i].shared_state().lock().def_node = sn.def_node;
            if sn.type_name != "Subgraph" {
                continue;
            }
            let def = linked::definition_of(sn);
            if self.definition_at(level, i) != def {
                self.set_tag(id, def);
            }
            if let (Some(inner), Some(inner_level)) = (&sn.inner_graph, self.level_of_subgraph(id)) {
                self.apply_tags(inner_level, inner);
            }
        }
    }

    /// An instance leaves its definition (see `linked::make_unique`).
    pub fn make_unique(&mut self, id: NodeId) {
        let mut project = project::save_graph(self);
        let Some(path) = self.saved_path_of(&project, id) else { return };
        linked::make_unique(&mut project, &path);
        self.apply_tags(0, &project);
    }

    /// Subgraph ids from the root to the node `id` in a saved project.
    fn saved_path_of(&self, project: &ProjectFile, id: NodeId) -> Option<Vec<u64>> {
        fn find(level: &ProjectFile, id: u64, prefix: &mut Vec<u64>) -> bool {
            for n in &level.nodes {
                if n.id == id {
                    prefix.push(id);
                    return true;
                }
                if let Some(inner) = &n.inner_graph {
                    prefix.push(n.id);
                    if find(inner, id, prefix) {
                        return true;
                    }
                    prefix.pop();
                }
            }
            false
        }
        let mut path = Vec::new();
        find(project, id.0, &mut path).then_some(path)
    }

    /// "Link identical subgraphs": returns how many groups and instances.
    pub fn link_identical(&mut self) -> linked::LinkReport {
        self.assign_def_nodes();
        let names = self.param_names();
        let mut project = project::save_graph(self);
        let report = linked::link_identical(&mut project, &names);
        if report.groups > 0 {
            self.apply_tags(0, &project);
        }
        report
    }

    // -----------------------------------------------------------------------
    // Applying a sync step
    // -----------------------------------------------------------------------

    /// Bring the instance `step.path` to `step.result`. Returns true when its
    /// level was built again, in which case the app has to create the engine
    /// nodes and send their data (`LightBeatApp::apply_link_steps`).
    pub fn apply_sync_step(&mut self, step: &SyncStep) -> bool {
        let Some(&id) = step.path.last() else { return false };
        let id = NodeId(id);
        let Some(level) = self.levels.iter().position(|l| l.states.iter().any(|s| s.id == id)) else { return false };
        let Some(index) = self.levels[level].states.iter().position(|s| s.id == id) else { return false };
        self.levels[level].nodes[index].restore_from_save(step.result.data.as_ref(), &[]);
        let title = self.levels[level].nodes[index].display_title().to_string();
        let Some(inner_level) = self.level_of_subgraph(id) else { return false };
        self.levels[inner_level].label = title;
        let Some(inner) = &step.result.inner_graph else { return false };
        if step.rebuild {
            self.rebuild_level(inner_level, inner);
        } else {
            self.apply_in_place(inner_level, inner);
        }
        true
    }

    /// Replace everything on `level` with `new`.
    fn rebuild_level(&mut self, level: usize, new: &ProjectFile) {
        let before = self.active_level;
        self.active_level = level;
        self.stack_sig = None;
        let gone: Vec<NodeId> = self.levels[level].states.iter().map(|s| s.id).filter(|&id| !is_bridge(id)).collect();
        for id in gone {
            self.push_engine_cmd(EngineCommand::RemoveNode(id));
        }
        let l = &mut self.levels[level];
        for i in (0..l.nodes.len()).rev() {
            if !is_bridge(l.states[i].id) {
                l.nodes.remove(i);
                l.states.remove(i);
            }
        }
        l.connections.clear();
        l.frames.clear();
        if level == before {
            self.selected_nodes.clear();
        }
        if let Some([x, y]) = new.bridge_in_pos {
            self.set_node_pos(BRIDGE_IN_NODE_ID, Pos2::new(x, y));
        }
        if let Some([x, y]) = new.bridge_out_pos {
            self.set_node_pos(BRIDGE_OUT_NODE_ID, Pos2::new(x, y));
        }
        let loaded = project::load_graph(self, new);
        // What the engine will say once it has caught up; saved forms read
        // before that would otherwise lack the data.
        for (saved, idx) in new.nodes.iter().zip(loaded.iter()) {
            if let Some(data) = &saved.data {
                self.levels[level].nodes[*idx].shared_state().lock().save_data = Some(data.clone());
            }
        }
        self.active_level = before;
    }

    /// Push the values of `new` into the nodes of `level`, which has the same
    /// nodes and wires.
    fn apply_in_place(&mut self, level: usize, new: &ProjectFile) {
        let before = self.active_level;
        self.active_level = level;
        for sn in &new.nodes {
            self.apply_node_in_place(level, sn);
        }
        self.levels[level].frames = project::frames_from_saved(&new.frames);
        if let Some([x, y]) = new.bridge_in_pos {
            self.set_node_pos(BRIDGE_IN_NODE_ID, Pos2::new(x, y));
        }
        if let Some([x, y]) = new.bridge_out_pos {
            self.set_node_pos(BRIDGE_OUT_NODE_ID, Pos2::new(x, y));
        }
        self.active_level = before;
    }

    fn apply_node_in_place(&mut self, level: usize, sn: &SavedNode) {
        let id = NodeId(sn.id);
        let Some(i) = self.levels[level].states.iter().position(|s| s.id == id) else { return };
        {
            let state = &mut self.levels[level].states[i];
            state.pos = Pos2::new(sn.pos[0], sn.pos[1]);
            state.size_override = sn.size.map(|[w, h]| Vec2::new(w, h));
        }
        let mut changed_params: Vec<(usize, ParamValue)> = Vec::new();
        {
            let mut shared = self.levels[level].nodes[i].shared_state().lock();
            shared.disabled = sn.disabled;
            shared.hide_unused_ports = sn.hide_unused_ports;
            shared.panel = sn.panel.clone();
            shared.def_node = sn.def_node;
            // Which snapshot is current is state of the instance, not of the definition.
            let mut ours = shared.snapshots.clone();
            let mut theirs = sn.snapshots.clone();
            ours.active = None;
            theirs.active = None;
            if ours != theirs {
                theirs.active = shared.snapshots.active;
                shared.snapshots = theirs.clone();
                shared.pending_snapshots = Some(theirs);
            }
            for sp in &sn.params {
                let val = project::saved_param_value(shared.current_params.get(sp.index), sp);
                let same = shared.current_params.get(sp.index).is_some_and(|p| p.as_value().as_f32() == val.as_f32());
                if !same {
                    shared.pending_params.push((sp.index, val.clone()));
                    if let Some(p) = shared.current_params.get_mut(sp.index) {
                        set_param_value(p, &val);
                    }
                    changed_params.push((sp.index, val));
                }
            }
        }
        let current = self.levels[level].nodes[i].live_data().or_else(|| self.levels[level].nodes[i].shared_state().lock().save_data.clone());
        if sn.data != current
            && let Some(data) = &sn.data
        {
            self.levels[level].nodes[i].restore_from_save(Some(data), &changed_params);
            self.levels[level].nodes[i].shared_state().lock().save_data = Some(data.clone());
            self.push_engine_cmd(EngineCommand::LoadData { node_id: id, data: data.clone() });
        }
        if let (Some(inner), Some(inner_level)) = (&sn.inner_graph, self.level_of_subgraph(id))
            && sn.type_name == "Subgraph"
        {
            self.apply_in_place(inner_level, inner);
            self.active_level = level;
        }
    }

    // -----------------------------------------------------------------------
    // Banner
    // -----------------------------------------------------------------------

    /// "Editing X: used N times" across the top of the canvas while the open
    /// level is the inside of a linked subgraph.
    pub(super) fn show_link_banner(&self, ui: &mut Ui, canvas_rect: Rect) {
        let Some((name, count)) = self.open_definition() else { return };
        if count < 2 {
            return;
        }
        let text = format!("Editing {name}: used {count} times. Changes apply to all. Front-panel values stay per instance.");
        let font = egui::FontId::proportional(13.0);
        let galley = ui.painter().layout_no_wrap(text, font, theme::TEXT);
        let size = galley.size() + Vec2::new(24.0, 10.0);
        let rect = Rect::from_center_size(
            Pos2::new(canvas_rect.center().x, canvas_rect.min.y + 8.0 + size.y / 2.0),
            size,
        );
        let painter = ui.painter();
        painter.rect_filled(rect, 6.0, theme::LINK_BANNER_FILL);
        painter.rect_stroke(rect, 6.0, Stroke::new(1.0_f32, theme::LINK_BADGE), StrokeKind::Inside);
        painter.galley(rect.min + Vec2::new(12.0, 5.0), galley, theme::TEXT);
    }

    /// Whether linking `clip` into the open level would put a definition inside itself.
    pub fn paste_would_nest(&self, definitions: &HashSet<u64>) -> bool {
        self.ancestor_definitions(self.active_level).iter().any(|d| definitions.contains(d))
    }
}

fn set_param_value(p: &mut ParamDef, v: &ParamValue) {
    match (p, v) {
        (ParamDef::Float { value, .. }, v) => *value = v.as_f32(),
        (ParamDef::Int { value, .. }, v) => *value = v.as_f32() as i64,
        (ParamDef::Bool { value, .. }, v) => *value = v.as_f32() != 0.0,
        (ParamDef::Choice { value, .. }, v) => *value = v.as_f32() as usize,
    }
}
