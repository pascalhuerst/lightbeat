//! Linked subgraphs: several Subgraph nodes that are instances of one
//! definition. Each instance keeps its full inner graph, its own level and its
//! own engine graph; what is added is a tag (`SubgraphConfig::definition`), a
//! definition-local identity for the nodes inside (`SavedNode::def_node`) and
//! the functions here, which bring the instances to the same content when one
//! of them is edited. Everything works on saved forms (`ProjectFile`) so it
//! needs no window and no engine; the app applies the result to the live graph.
//!
//! The rule for the user: what is on a front panel belongs to the instance,
//! everything else to the definition.

use std::collections::{HashMap, HashSet};

use crate::engine::nodes::meta::subgraph::{BRIDGE_IN_NODE_ID, BRIDGE_OUT_NODE_ID};
use crate::project::{ProjectFile, SavedConnection, SavedNode, SavedParam, remap_ids_inplace};

/// Param names of every live node by node id. A saved node only knows its
/// params by index, a panel item names them.
pub type ParamNames = HashMap<u64, Vec<String>>;

/// The identity of a node inside its definition. A node that has none yet
/// stands for itself.
pub fn def_key(n: &SavedNode) -> u64 {
    n.def_node.unwrap_or(n.id)
}

/// The definition a Subgraph node is an instance of.
pub fn definition_of(n: &SavedNode) -> Option<u64> {
    if n.type_name != "Subgraph" {
        return None;
    }
    n.data.as_ref()?.get("definition")?.as_u64()
}

pub fn set_definition(n: &mut SavedNode, definition: Option<u64>) {
    let mut data = match n.data.take() {
        Some(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    match definition {
        Some(d) => {
            data.insert("definition".into(), d.into());
        }
        None => {
            data.remove("definition");
        }
    }
    n.data = Some(serde_json::Value::Object(data));
}

/// Name, ports and macro origin the instances share, other than the tag.
fn data_without_definition(n: &SavedNode) -> Option<serde_json::Value> {
    let mut data = n.data.clone()?;
    if let Some(map) = data.as_object_mut() {
        map.remove("definition");
    }
    Some(data)
}

/// A fresh definition id. Projects are edited separately and their pieces are
/// copied between them (clipboard, macros), so an id must not be a counter
/// that two projects share; time and a process counter, hashed, keep them apart.
/// Below 2^53 so that tools that read JSON numbers as doubles keep it exact.
pub fn new_definition_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut h: u64 = 0xcbf29ce484222325;
    for b in nanos.to_le_bytes().into_iter().chain(COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    (h & ((1 << 53) - 1)).max(1)
}

/// Take the definition tags off every subgraph at and below `level`: what is
/// kept in a macro file says nothing about a project's definitions.
pub fn strip_definitions(level: &mut ProjectFile) {
    for n in &mut level.nodes {
        if n.type_name == "Subgraph" {
            set_definition(n, None);
        }
        if let Some(inner) = n.inner_graph.as_mut() {
            strip_definitions(inner);
        }
    }
}

fn is_bridge(id: u64) -> bool {
    id == BRIDGE_IN_NODE_ID.0 || id == BRIDGE_OUT_NODE_ID.0
}

/// Give every node below `level` (nested levels too) the identity it has in
/// the definition being made, which is its own id unless it has one.
pub fn assign_def_nodes(level: &mut ProjectFile) {
    for n in &mut level.nodes {
        n.def_node.get_or_insert(n.id);
        if let Some(inner) = n.inner_graph.as_mut() {
            assign_def_nodes(inner);
        }
    }
}

// ---------------------------------------------------------------------------
// Finding instances
// ---------------------------------------------------------------------------

pub struct Instance<'a> {
    /// Subgraph node ids from the root down to this node, itself last.
    pub path: Vec<u64>,
    pub definition: u64,
    pub node: &'a SavedNode,
}

/// Every linked Subgraph node, outer before inner, in load order.
pub fn instances(project: &ProjectFile) -> Vec<Instance<'_>> {
    fn walk<'a>(level: &'a ProjectFile, prefix: &mut Vec<u64>, out: &mut Vec<Instance<'a>>) {
        for n in &level.nodes {
            let Some(inner) = n.inner_graph.as_ref() else { continue };
            prefix.push(n.id);
            if let Some(definition) = definition_of(n) {
                out.push(Instance { path: prefix.clone(), definition, node: n });
            }
            walk(inner, prefix, out);
            prefix.pop();
        }
    }
    let mut out = Vec::new();
    walk(project, &mut Vec::new(), &mut out);
    out
}

pub fn node_at<'a>(project: &'a ProjectFile, path: &[u64]) -> Option<&'a SavedNode> {
    let (last, parents) = path.split_last()?;
    let mut level = project;
    for id in parents {
        level = level.nodes.iter().find(|n| n.id == *id)?.inner_graph.as_ref()?;
    }
    level.nodes.iter().find(|n| n.id == *last)
}

fn node_at_mut<'a>(project: &'a mut ProjectFile, path: &[u64]) -> Option<&'a mut SavedNode> {
    let (last, parents) = path.split_last()?;
    let mut level = project;
    for id in parents {
        level = level.nodes.iter_mut().find(|n| n.id == *id)?.inner_graph.as_mut()?;
    }
    level.nodes.iter_mut().find(|n| n.id == *last)
}

/// Definitions of the Subgraphs at and below a node, given as its data and
/// inner graph (a clipboard entry has no `SavedNode`).
pub fn definitions_in(data: Option<&serde_json::Value>, inner: Option<&ProjectFile>, out: &mut HashSet<u64>) {
    if let Some(d) = data.and_then(|d| d.get("definition")).and_then(|v| v.as_u64()) {
        out.insert(d);
    }
    if let Some(inner) = inner {
        for n in &inner.nodes {
            definitions_in(n.data.as_ref().filter(|_| n.type_name == "Subgraph"), n.inner_graph.as_ref(), out);
        }
    }
}

/// Whether any instance sits inside an instance of the same definition.
pub fn has_self_nesting(project: &ProjectFile) -> bool {
    fn walk(level: &ProjectFile, ancestors: &mut Vec<u64>) -> bool {
        for n in &level.nodes {
            let Some(inner) = &n.inner_graph else { continue };
            let d = definition_of(n);
            if let Some(d) = d {
                if ancestors.contains(&d) {
                    return true;
                }
                ancestors.push(d);
            }
            let found = walk(inner, ancestors);
            if d.is_some() {
                ancestors.pop();
            }
            if found {
                return true;
            }
        }
        false
    }
    walk(project, &mut Vec::new())
}

// ---------------------------------------------------------------------------
// Sync
// ---------------------------------------------------------------------------

/// A copy of `s` as a new node of the target: fresh ids all the way down, the
/// same definition identity.
fn fresh_copy(s: &SavedNode, mut alloc: &mut dyn FnMut() -> u64) -> SavedNode {
    let mut n = s.clone();
    n.id = alloc();
    n.def_node = Some(def_key(s));
    if let Some(inner) = n.inner_graph.as_mut() {
        assign_def_nodes(inner);
        remap_ids_inplace(inner, &mut alloc);
    }
    n
}

/// `s` (the source's node) as it stands in the target, whose own counterpart
/// is `d`: everything from the source except the values of what is on the
/// front panel, which stay the target's.
fn merge_node(s: &SavedNode, d: &SavedNode, names: &ParamNames, alloc: &mut dyn FnMut() -> u64) -> SavedNode {
    let mut n = s.clone();
    n.id = d.id;
    n.def_node = d.def_node;
    if let Some(panel) = &s.panel {
        if panel.whole {
            n.params = d.params.clone();
            if n.type_name != "Subgraph" {
                n.data = d.data.clone();
            }
        } else if let Some(wanted) = names.get(&d.id) {
            for name in &panel.params {
                let Some(index) = wanted.iter().position(|p| p == name) else { continue };
                let theirs = d.params.iter().find(|p| p.index == index);
                n.params.retain(|p| p.index != index);
                if let Some(p) = theirs {
                    n.params.push(SavedParam { index, value: p.value.clone() });
                }
            }
            n.params.sort_by_key(|p| p.index);
        } else if !panel.params.is_empty() {
            // The setting cannot be told apart yet, so none of them is touched
            // rather than overwriting what the instance has pinned.
            n.params = d.params.clone();
        }
    }
    // Which snapshot is current is state, not definition.
    n.snapshots.active = d.snapshots.active;

    if let Some(s_inner) = &s.inner_graph {
        let same_definition = definition_of(s).is_some() && definition_of(s) == definition_of(d);
        n.inner_graph = match &d.inner_graph {
            // A nested linked subgraph is kept up to date by its own definition.
            Some(d_inner) if same_definition => Some(d_inner.clone()),
            Some(d_inner) => Some(sync_level(s_inner, d_inner, names, alloc)),
            None => Some(sync_level(s_inner, &empty_level(s_inner), names, alloc)),
        };
    }
    n
}

fn empty_level(like: &ProjectFile) -> ProjectFile {
    ProjectFile {
        version: like.version,
        nodes: Vec::new(),
        connections: Vec::new(),
        frames: Vec::new(),
        view: None,
        bridge_in_pos: None,
        bridge_out_pos: None,
    }
}

fn connection_set(cs: &[SavedConnection]) -> HashSet<(u64, usize, u64, usize)> {
    cs.iter().map(|c| (c.from_node, c.from_port, c.to_node, c.to_port)).collect()
}

/// `dst`'s level brought to `src`'s content. Nodes are matched by their
/// definition identity: ones of the source that the target lacks are made
/// with fresh ids, ones the target has and the source does not are dropped,
/// the rest keep the target's ids and order.
pub fn sync_level(src: &ProjectFile, dst: &ProjectFile, names: &ParamNames, alloc: &mut dyn FnMut() -> u64) -> ProjectFile {
    let theirs: HashMap<u64, &SavedNode> = dst.nodes.iter().map(|n| (def_key(n), n)).collect();
    let mut id_map: HashMap<u64, u64> = HashMap::new();
    let mut kept: HashMap<u64, SavedNode> = HashMap::new();
    let mut added: Vec<SavedNode> = Vec::new();
    for s in &src.nodes {
        match theirs.get(&def_key(s)) {
            Some(d) if d.type_name == s.type_name => {
                id_map.insert(s.id, d.id);
                kept.insert(d.id, merge_node(s, d, names, alloc));
            }
            _ => {
                let n = fresh_copy(s, alloc);
                id_map.insert(s.id, n.id);
                added.push(n);
            }
        }
    }
    let mut nodes: Vec<SavedNode> = dst.nodes.iter().filter_map(|d| kept.remove(&d.id)).collect();
    nodes.extend(added);

    let map = |id: u64| if is_bridge(id) { Some(id) } else { id_map.get(&id).copied() };
    let mapped: Vec<SavedConnection> = src
        .connections
        .iter()
        .filter_map(|c| {
            Some(SavedConnection {
                from_node: map(c.from_node)?,
                from_port: c.from_port,
                to_node: map(c.to_node)?,
                to_port: c.to_port,
            })
        })
        .collect();
    let connections = if connection_set(&mapped) == connection_set(&dst.connections) {
        dst.connections.clone()
    } else {
        mapped
    };

    ProjectFile {
        version: dst.version,
        nodes,
        connections,
        frames: src.frames.clone(),
        view: None,
        bridge_in_pos: src.bridge_in_pos,
        bridge_out_pos: src.bridge_out_pos,
    }
}

/// The instance `dst` after it follows `src`: its name, ports and inner graph.
/// Its own place, size, wires, promoted and snapshot state are not part of a
/// definition and stay.
pub fn sync_instance(src: &SavedNode, dst: &SavedNode, names: &ParamNames, alloc: &mut dyn FnMut() -> u64) -> SavedNode {
    let mut out = dst.clone();
    out.data = src.data.clone();
    if let Some(s_inner) = &src.inner_graph {
        let empty = empty_level(s_inner);
        let d_inner = dst.inner_graph.as_ref().unwrap_or(&empty);
        out.inner_graph = Some(sync_level(s_inner, d_inner, names, alloc));
    }
    out
}

fn ports_of(n: &SavedNode) -> (Option<&serde_json::Value>, Option<&serde_json::Value>) {
    let data = n.data.as_ref();
    (data.and_then(|d| d.get("inputs")), data.and_then(|d| d.get("outputs")))
}

/// Whether `new` differs from `old` only in values that can be pushed into the
/// running nodes: the same nodes, wires, ports and promoted settings. Anything
/// else needs the level built again.
pub fn in_place_ok(old: &ProjectFile, new: &ProjectFile) -> bool {
    if old.nodes.len() != new.nodes.len() || connection_set(&old.connections) != connection_set(&new.connections) {
        return false;
    }
    let theirs: HashMap<u64, &SavedNode> = old.nodes.iter().map(|n| (n.id, n)).collect();
    new.nodes.iter().all(|n| {
        let Some(o) = theirs.get(&n.id) else { return false };
        if o.type_name != n.type_name || o.promoted != n.promoted {
            return false;
        }
        match (&o.inner_graph, &n.inner_graph) {
            (Some(oi), Some(ni)) => ports_of(o) == ports_of(n) && in_place_ok(oi, ni),
            (None, None) => true,
            _ => false,
        }
    })
}

#[derive(Debug, PartialEq)]
pub struct SyncStep {
    /// The instance to change.
    pub path: Vec<u64>,
    /// The instance it follows.
    pub source: Vec<u64>,
    /// The instance as it is to be after the step.
    pub result: SavedNode,
    /// The level has to be built again; otherwise values are pushed in place.
    pub rebuild: bool,
}

impl std::fmt::Debug for SavedNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SavedNode({} #{})", self.type_name, self.id)
    }
}

/// What the definition looks like in one instance.
fn definition_view(n: &SavedNode) -> (Option<&serde_json::Value>, Option<&ProjectFile>) {
    (n.data.as_ref(), n.inner_graph.as_ref())
}

/// The steps that bring every definition's instances to one content. The
/// source is the first instance that differs from `baseline` (the project as
/// it was when it last settled), or, with none or when nothing was edited
/// (a project just loaded), the first in load order.
pub fn plan_sync(
    baseline: Option<&ProjectFile>,
    current: &ProjectFile,
    names: &ParamNames,
    alloc: &mut dyn FnMut() -> u64,
) -> Vec<SyncStep> {
    let all = instances(current);
    let mut order: Vec<u64> = Vec::new();
    for i in &all {
        if !order.contains(&i.definition) {
            order.push(i.definition);
        }
    }
    let mut steps = Vec::new();
    for def in order {
        let group: Vec<&Instance> = all.iter().filter(|i| i.definition == def).collect();
        if group.len() < 2 {
            continue;
        }
        let changed = |i: &Instance| {
            baseline
                .and_then(|b| node_at(b, &i.path))
                .is_some_and(|before| definition_view(before) != definition_view(i.node))
        };
        let source = group.iter().find(|i| changed(i)).unwrap_or(&group[0]);
        for target in group.iter().filter(|i| i.path != source.path) {
            let result = sync_instance(source.node, target.node, names, alloc);
            if result.data == target.node.data && result.inner_graph == target.node.inner_graph {
                continue;
            }
            let rebuild = match (&target.node.inner_graph, &result.inner_graph) {
                (Some(o), Some(n)) => ports_of(target.node) != ports_of(&result) || !in_place_ok(o, n),
                _ => true,
            };
            steps.push(SyncStep { path: target.path.clone(), source: source.path.clone(), result, rebuild });
        }
    }
    // A rebuilt level takes its nested instances with it; they are planned
    // again on the next round.
    let rebuilt: Vec<Vec<u64>> = steps.iter().filter(|s| s.rebuild).map(|s| s.path.clone()).collect();
    steps.retain(|s| !rebuilt.iter().any(|r| s.path.len() > r.len() && s.path.starts_with(r)));
    steps.sort_by_key(|s| s.path.len());
    steps
}

/// The steps that make every instance of `definition` follow `source` (the
/// library's version of a macro), whatever the instances look like now.
pub fn plan_update(
    project: &ProjectFile,
    definition: u64,
    source: &SavedNode,
    names: &ParamNames,
    alloc: &mut dyn FnMut() -> u64,
) -> Vec<SyncStep> {
    let mut steps = Vec::new();
    for target in instances(project).into_iter().filter(|i| i.definition == definition) {
        let result = sync_instance(source, target.node, names, alloc);
        if result.data == target.node.data && result.inner_graph == target.node.inner_graph {
            continue;
        }
        let rebuild = match (&target.node.inner_graph, &result.inner_graph) {
            (Some(o), Some(n)) => ports_of(target.node) != ports_of(&result) || !in_place_ok(o, n),
            _ => true,
        };
        steps.push(SyncStep { path: target.path.clone(), source: Vec::new(), result, rebuild });
    }
    let rebuilt: Vec<Vec<u64>> = steps.iter().filter(|s| s.rebuild).map(|s| s.path.clone()).collect();
    steps.retain(|s| !rebuilt.iter().any(|r| s.path.len() > r.len() && s.path.starts_with(r)));
    steps.sort_by_key(|s| s.path.len());
    steps
}

/// Put the results of `steps` into a saved project, so that it shows what the
/// live graph is about to look like.
pub fn apply_steps(project: &mut ProjectFile, steps: &[SyncStep]) {
    for s in steps {
        if let Some(node) = node_at_mut(project, &s.path) {
            *node = s.result.clone();
        }
    }
}

/// Make the nodes of `project` (the first level only) and the levels below
/// carry definition identities, a hash-free way to tell them apart later.
pub fn make_definition(node: &mut SavedNode, definition: u64) {
    set_definition(node, Some(definition));
    if let Some(inner) = node.inner_graph.as_mut() {
        assign_def_nodes(inner);
    }
}

/// An instance leaves its definition. Returns the definition when exactly one
/// other instance is left, which then is plain again.
pub fn make_unique(project: &mut ProjectFile, path: &[u64]) -> Option<u64> {
    let definition = node_at(project, path).and_then(definition_of)?;
    if let Some(n) = node_at_mut(project, path) {
        set_definition(n, None);
    }
    let left: Vec<Vec<u64>> = instances(project).into_iter().filter(|i| i.definition == definition).map(|i| i.path).collect();
    if let [only] = &left[..] {
        if let Some(n) = node_at_mut(project, only) {
            set_definition(n, None);
        }
        return Some(definition);
    }
    None
}

// ---------------------------------------------------------------------------
// Link identical
// ---------------------------------------------------------------------------

/// A level with ids replaced by positions and per-instance values blanked, as
/// text, so that two subgraphs can be compared for being the same.
fn canonical_level(level: &ProjectFile, names: &ParamNames) -> serde_json::Value {
    let index: HashMap<u64, i64> = level.nodes.iter().enumerate().map(|(i, n)| (n.id, i as i64)).collect();
    let at = |id: u64| {
        if id == BRIDGE_IN_NODE_ID.0 {
            -1
        } else if id == BRIDGE_OUT_NODE_ID.0 {
            -2
        } else {
            index.get(&id).copied().unwrap_or(-3)
        }
    };
    let nodes: Vec<serde_json::Value> = level.nodes.iter().map(|n| canonical_node(n, names)).collect();
    let mut conns: Vec<(i64, usize, i64, usize)> =
        level.connections.iter().map(|c| (at(c.from_node), c.from_port, at(c.to_node), c.to_port)).collect();
    conns.sort_unstable();
    let frames: Vec<serde_json::Value> = level
        .frames
        .iter()
        .map(|f| serde_json::json!([f.title, f.color, f.notes, f.pos, f.size]))
        .collect();
    serde_json::json!({
        "nodes": nodes,
        "connections": conns,
        "frames": frames,
        "bridges": [level.bridge_in_pos, level.bridge_out_pos],
    })
}

fn canonical_node(n: &SavedNode, names: &ParamNames) -> serde_json::Value {
    let mut params: Vec<(usize, &crate::project::SavedParamValue)> = n.params.iter().map(|p| (p.index, &p.value)).collect();
    let mut data = if n.type_name == "Subgraph" { data_without_definition(n) } else { n.data.clone() };
    if let Some(panel) = &n.panel {
        if panel.whole {
            params.clear();
            if n.type_name != "Subgraph" {
                data = None;
            }
        } else if let Some(wanted) = names.get(&n.id) {
            let blank: Vec<usize> = panel.params.iter().filter_map(|name| wanted.iter().position(|p| p == name)).collect();
            params.retain(|(i, _)| !blank.contains(i));
        }
    }
    params.sort_by_key(|(i, _)| *i);
    serde_json::json!({
        "type": n.type_name,
        "pos": n.pos,
        "size": n.size,
        "params": params.iter().map(|(i, v)| serde_json::json!([i, v])).collect::<Vec<_>>(),
        "data": data,
        "inner": n.inner_graph.as_ref().map(|g| canonical_level(g, names)),
        "disabled": n.disabled,
        "promoted": n.promoted,
        "hide": n.hide_unused_ports,
        "panel": n.panel,
    })
}

/// What `link_identical` did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LinkReport {
    pub groups: usize,
    pub instances: usize,
}

/// Give `other` the definition identities `reference` has, node by node.
fn align_def_nodes(reference: &mut ProjectFile, other: &mut ProjectFile) {
    for (r, o) in reference.nodes.iter_mut().zip(other.nodes.iter_mut()) {
        let key = *r.def_node.get_or_insert(r.id);
        o.def_node = Some(key);
        if let (Some(ri), Some(oi)) = (r.inner_graph.as_mut(), o.inner_graph.as_mut()) {
            align_def_nodes(ri, oi);
        }
    }
}

/// Link the subgraphs anywhere in `project` whose inner graphs are the same
/// apart from node ids and front-panel values, plain or linked already.
/// Subgraphs that sit inside a member of their own group (cannot happen for
/// equal graphs) or would make a definition contain itself are left alone.
pub fn link_identical(project: &mut ProjectFile, names: &ParamNames) -> LinkReport {
    fn collect(level: &ProjectFile, prefix: &mut Vec<u64>, names: &ParamNames, groups: &mut Vec<(String, Vec<Vec<u64>>)>) {
        for n in &level.nodes {
            let Some(inner) = &n.inner_graph else { continue };
            prefix.push(n.id);
            let mut key = canonical_level(inner, names);
            key["data"] = data_without_definition(n).unwrap_or(serde_json::Value::Null);
            let key = key.to_string();
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, members)) => members.push(prefix.clone()),
                None => groups.push((key, vec![prefix.clone()])),
            }
            collect(inner, prefix, names, groups);
            prefix.pop();
        }
    }
    let mut groups = Vec::new();
    collect(project, &mut Vec::new(), names, &mut groups);

    let mut report = LinkReport::default();
    for (_, members) in groups {
        if members.len() < 2 {
            continue;
        }
        // Several nodes that are already one definition and nothing else.
        let defs: Vec<Option<u64>> = members.iter().map(|p| node_at(project, p).and_then(definition_of)).collect();
        if defs[0].is_some() && defs.iter().all(|d| *d == defs[0]) {
            continue;
        }
        let definition = defs.iter().flatten().next().copied().unwrap_or_else(new_definition_id);
        let Some(mut reference) = node_at(project, &members[0]).cloned() else { continue };
        let ref_inner = reference.inner_graph.get_or_insert_with(|| empty_level(project));
        assign_def_nodes(ref_inner);
        let before = project.clone();
        make_definition(&mut reference, definition);
        let mut staged: Vec<(Vec<u64>, SavedNode)> = vec![(members[0].clone(), reference.clone())];
        for path in &members[1..] {
            let Some(mut n) = node_at(project, path).cloned() else { continue };
            if let (Some(ri), Some(oi)) = (reference.inner_graph.as_mut(), n.inner_graph.as_mut()) {
                align_def_nodes(ri, oi);
            }
            set_definition(&mut n, Some(definition));
            staged.push((path.clone(), n));
        }
        for (path, n) in &staged {
            if let Some(slot) = node_at_mut(project, path) {
                *slot = n.clone();
            }
        }
        if has_self_nesting(project) {
            *project = before;
            continue;
        }
        report.groups += 1;
        report.instances += staged.len();
    }
    report
}

// ---------------------------------------------------------------------------
// Library origin
// ---------------------------------------------------------------------------

/// A stable hash of a macro's graph (FNV-1a over its JSON), kept with the
/// instances to tell whether the library file changed since.
pub fn graph_hash(graph: &ProjectFile) -> String {
    let json = serde_json::to_string(graph).unwrap_or_default();
    let mut h: u64 = 0xcbf29ce484222325;
    for b in json.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::PanelItem;
    use crate::project::SavedParamValue;

    fn node(type_name: &str, id: u64) -> SavedNode {
        SavedNode {
            type_name: type_name.into(),
            id,
            pos: [0.0, 0.0],
            size: None,
            params: vec![SavedParam { index: 0, value: SavedParamValue::Float(0.5) }],
            data: None,
            inner_graph: None,
            disabled: false,
            promoted: Vec::new(),
            hide_unused_ports: false,
            panel: None,
            def_node: None,
            snapshots: Default::default(),
        }
    }

    fn level(nodes: Vec<SavedNode>, conns: &[(u64, usize, u64, usize)]) -> ProjectFile {
        ProjectFile {
            version: crate::project::PROJECT_VERSION,
            nodes,
            connections: conns
                .iter()
                .map(|&(a, ap, b, bp)| SavedConnection { from_node: a, from_port: ap, to_node: b, to_port: bp })
                .collect(),
            frames: Vec::new(),
            view: None,
            bridge_in_pos: None,
            bridge_out_pos: None,
        }
    }

    fn sub(id: u64, def: Option<u64>, inner: ProjectFile) -> SavedNode {
        let mut n = node("Subgraph", id);
        n.params.clear();
        n.data = Some(serde_json::json!({"name": "S", "inputs": [], "outputs": []}));
        n.inner_graph = Some(inner);
        set_definition(&mut n, def);
        n
    }

    fn counter(start: u64) -> impl FnMut() -> u64 {
        let mut next = start;
        move || {
            next += 1;
            next - 1
        }
    }

    fn param(n: &SavedNode, index: usize) -> f64 {
        match n.params.iter().find(|p| p.index == index).map(|p| &p.value) {
            Some(SavedParamValue::Float(v)) => *v,
            other => panic!("no float param {index}: {}", other.is_some()),
        }
    }

    fn set_param(n: &mut SavedNode, index: usize, v: f64) {
        n.params.retain(|p| p.index != index);
        n.params.push(SavedParam { index, value: SavedParamValue::Float(v) });
    }

    /// Two instances (ids 1 and 2) of definition 100 with the same two wired
    /// nodes; the second one's inner nodes have their own ids.
    fn pair() -> ProjectFile {
        let mut a = level(vec![node("Gain", 10), node("Const Value", 11)], &[(11, 0, 10, 0), (BRIDGE_IN_NODE_ID.0, 0, 10, 1)]);
        let mut b = level(vec![node("Gain", 20), node("Const Value", 21)], &[(21, 0, 20, 0), (BRIDGE_IN_NODE_ID.0, 0, 20, 1)]);
        for (l, base) in [(&mut a, 10), (&mut b, 10)] {
            l.nodes[0].def_node = Some(base);
            l.nodes[1].def_node = Some(base + 1);
        }
        level(vec![sub(1, Some(100), a), sub(2, Some(100), b)], &[])
    }

    fn inner(p: &ProjectFile, i: usize) -> &ProjectFile {
        p.nodes[i].inner_graph.as_ref().unwrap()
    }

    fn plan(base: Option<&ProjectFile>, cur: &ProjectFile) -> Vec<SyncStep> {
        plan_sync(base, cur, &ParamNames::new(), &mut counter(1000))
    }

    #[test]
    fn identical_instances_need_no_step() {
        let p = pair();
        assert!(plan(Some(&p), &p).is_empty());
        assert!(plan(None, &p).is_empty());
    }

    #[test]
    fn an_edited_value_is_pushed_in_place_and_ids_stay() {
        let base = pair();
        let mut cur = base.clone();
        set_param(&mut cur.nodes[0].inner_graph.as_mut().unwrap().nodes[0], 0, 0.9);
        let steps = plan(Some(&base), &cur);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].path, vec![2]);
        assert!(!steps[0].rebuild);
        let r = steps[0].result.inner_graph.as_ref().unwrap();
        assert_eq!(r.nodes[0].id, 20);
        assert_eq!(param(&r.nodes[0], 0), 0.9);
        assert_eq!(r.connections, inner(&cur, 1).connections);
    }

    #[test]
    fn a_node_added_in_one_instance_is_made_in_the_others_with_a_fresh_id() {
        let base = pair();
        let mut cur = base.clone();
        let g = cur.nodes[1].inner_graph.as_mut().unwrap();
        g.nodes.push(node("Lfo", 77));
        g.connections.push(SavedConnection { from_node: 77, from_port: 0, to_node: 20, to_port: 1 });
        let steps = plan(Some(&base), &cur);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].path, vec![1]);
        assert!(steps[0].rebuild);
        let r = steps[0].result.inner_graph.as_ref().unwrap();
        assert_eq!(r.nodes.len(), 3);
        assert_eq!(&r.nodes[..2].iter().map(|n| n.id).collect::<Vec<_>>(), &[10, 11]);
        let added = &r.nodes[2];
        assert_eq!(added.type_name, "Lfo");
        assert!(added.id >= 1000, "a fresh id, not 77");
        assert_eq!(added.def_node, Some(77), "it keeps the identity it has in the source");
        assert!(r.connections.contains(&SavedConnection { from_node: added.id, from_port: 0, to_node: 10, to_port: 1 }));
    }

    #[test]
    fn a_node_removed_in_one_instance_goes_from_the_others() {
        let base = pair();
        let mut cur = base.clone();
        let g = cur.nodes[0].inner_graph.as_mut().unwrap();
        g.nodes.remove(1);
        g.connections.retain(|c| c.from_node != 11);
        let steps = plan(Some(&base), &cur);
        assert_eq!(steps.len(), 1);
        let r = steps[0].result.inner_graph.as_ref().unwrap();
        assert_eq!(r.nodes.iter().map(|n| n.id).collect::<Vec<_>>(), vec![20]);
        assert_eq!(r.connections.len(), 1);
        assert!(steps[0].rebuild);
    }

    #[test]
    fn a_new_wire_between_existing_nodes_rebuilds() {
        let base = pair();
        let mut cur = base.clone();
        cur.nodes[0].inner_graph.as_mut().unwrap().connections.push(SavedConnection { from_node: 10, from_port: 0, to_node: BRIDGE_OUT_NODE_ID.0, to_port: 0 });
        let steps = plan(Some(&base), &cur);
        assert!(steps[0].rebuild);
        let r = steps[0].result.inner_graph.as_ref().unwrap();
        assert!(r.connections.contains(&SavedConnection { from_node: 20, from_port: 0, to_node: BRIDGE_OUT_NODE_ID.0, to_port: 0 }));
    }

    #[test]
    fn positions_name_ports_and_bridges_follow_the_source() {
        let base = pair();
        let mut cur = base.clone();
        cur.nodes[1].inner_graph.as_mut().unwrap().nodes[0].pos = [500.0, 5.0];
        cur.nodes[1].inner_graph.as_mut().unwrap().bridge_in_pos = Some([1.0, 2.0]);
        let steps = plan(Some(&base), &cur);
        let r = steps[0].result.inner_graph.as_ref().unwrap();
        assert_eq!(r.nodes[0].pos, [500.0, 5.0]);
        assert_eq!(r.bridge_in_pos, Some([1.0, 2.0]));
        assert!(!steps[0].rebuild);

        let mut cur = base.clone();
        cur.nodes[0].data = Some(serde_json::json!({"name": "Renamed", "inputs": [], "outputs": [], "definition": 100}));
        let steps = plan(Some(&base), &cur);
        assert_eq!(steps[0].result.data, cur.nodes[0].data);
        assert!(!steps[0].rebuild);

        let mut cur = base.clone();
        cur.nodes[0].data = Some(serde_json::json!({"name": "S", "inputs": [{"name": "a", "port_type_idx": 2}], "outputs": [], "definition": 100}));
        assert!(plan(Some(&base), &cur)[0].rebuild, "a port change reshapes the node");
    }

    #[test]
    fn the_nodes_own_place_wires_and_state_on_the_parent_level_stay() {
        let base = pair();
        let mut cur = base.clone();
        set_param(&mut cur.nodes[0].inner_graph.as_mut().unwrap().nodes[0], 0, 0.1);
        cur.nodes[1].pos = [900.0, 900.0];
        cur.nodes[1].size = Some([300.0, 200.0]);
        cur.nodes[1].disabled = true;
        let steps = plan(Some(&base), &cur);
        let r = &steps[0].result;
        assert_eq!(r.pos, [900.0, 900.0]);
        assert_eq!(r.size, Some([300.0, 200.0]));
        assert!(r.disabled);
        assert_eq!(r.id, 2);
    }

    #[test]
    fn promoted_lists_follow_the_definition_and_rebuild() {
        let base = pair();
        let mut cur = base.clone();
        cur.nodes[0].inner_graph.as_mut().unwrap().nodes[0].promoted = vec![crate::engine::types::PromotedParam {
            name: "Gain".into(),
            kind: crate::engine::types::PromotedKind::Float,
        }];
        let steps = plan(Some(&base), &cur);
        assert!(steps[0].rebuild);
        assert_eq!(steps[0].result.inner_graph.as_ref().unwrap().nodes[0].promoted.len(), 1);
    }

    #[test]
    fn values_on_the_front_panel_stay_per_instance() {
        let mut base = pair();
        for i in 0..2 {
            let g = base.nodes[i].inner_graph.as_mut().unwrap();
            g.nodes[0].panel = Some(PanelItem { order: 0, whole: true, params: vec![] });
            g.nodes[0].data = Some(serde_json::json!({"v": i}));
            set_param(&mut g.nodes[0], 0, i as f64);
            g.nodes[1].panel = Some(PanelItem { order: 1, whole: false, params: vec!["Level".into()] });
            set_param(&mut g.nodes[1], 0, 0.25 + i as f64);
            set_param(&mut g.nodes[1], 1, 0.0);
        }
        let mut names = ParamNames::new();
        names.insert(21, vec!["Level".into(), "Other".into()]);
        names.insert(11, vec!["Level".into(), "Other".into()]);
        let mut cur = base.clone();
        // An edit of the definition in instance 1: a value off the panel and a position.
        set_param(&mut cur.nodes[0].inner_graph.as_mut().unwrap().nodes[1], 1, 7.0);
        cur.nodes[0].inner_graph.as_mut().unwrap().nodes[0].pos = [9.0, 9.0];
        let steps = plan_sync(Some(&base), &cur, &names, &mut counter(1000));
        assert_eq!(steps.len(), 1);
        let r = steps[0].result.inner_graph.as_ref().unwrap();
        assert_eq!(param(&r.nodes[0], 0), 1.0, "whole node on the panel keeps its params");
        assert_eq!(r.nodes[0].data, Some(serde_json::json!({"v": 1})), "and its data");
        assert_eq!(r.nodes[0].pos, [9.0, 9.0], "but not its place");
        assert_eq!(param(&r.nodes[1], 0), 1.25, "a pinned setting stays");
        assert_eq!(param(&r.nodes[1], 1), 7.0, "an unpinned one follows");
    }

    #[test]
    fn turning_a_panel_value_in_one_instance_does_nothing_to_the_others() {
        let mut base = pair();
        for i in 0..2 {
            base.nodes[i].inner_graph.as_mut().unwrap().nodes[0].panel = Some(PanelItem { order: 0, whole: true, params: vec![] });
        }
        let mut cur = base.clone();
        set_param(&mut cur.nodes[0].inner_graph.as_mut().unwrap().nodes[0], 0, 0.99);
        assert!(plan(Some(&base), &cur).is_empty());
    }

    #[test]
    fn whether_a_node_is_on_the_panel_follows_the_definition() {
        let base = pair();
        let mut cur = base.clone();
        cur.nodes[0].inner_graph.as_mut().unwrap().nodes[0].panel = Some(PanelItem { order: 0, whole: true, params: vec![] });
        let steps = plan(Some(&base), &cur);
        assert!(steps[0].result.inner_graph.as_ref().unwrap().nodes[0].panel.is_some());
        assert!(!steps[0].rebuild);
    }

    #[test]
    fn the_first_edited_instance_is_the_source_and_without_an_edit_the_first_in_load_order() {
        let base = pair();
        let mut cur = base.clone();
        set_param(&mut cur.nodes[1].inner_graph.as_mut().unwrap().nodes[0], 0, 0.8);
        let steps = plan(Some(&base), &cur);
        assert_eq!((steps[0].source.clone(), steps[0].path.clone()), (vec![2], vec![1]));
        // Loaded: no baseline, they differ, the first wins.
        let steps = plan(None, &cur);
        assert_eq!((steps[0].source.clone(), steps[0].path.clone()), (vec![1], vec![2]));
    }

    #[test]
    fn syncing_twice_changes_nothing_the_second_time() {
        let base = pair();
        let mut cur = base.clone();
        let g = cur.nodes[0].inner_graph.as_mut().unwrap();
        g.nodes.push(node("Lfo", 77));
        g.nodes.remove(0);
        g.connections.clear();
        let steps = plan(Some(&base), &cur);
        apply_steps(&mut cur, &steps);
        assert!(plan(Some(&base), &cur).is_empty());
        assert!(plan(None, &cur).is_empty());
    }

    #[test]
    fn a_plain_subgraph_and_one_other_definition_are_left_alone() {
        let mut p = pair();
        p.nodes.push(sub(3, None, level(vec![node("Gain", 30)], &[])));
        p.nodes.push(sub(4, Some(200), level(vec![node("Gain", 40)], &[])));
        let base = p.clone();
        let mut cur = p.clone();
        set_param(&mut cur.nodes[3].inner_graph.as_mut().unwrap().nodes[0], 0, 0.1);
        assert!(plan(Some(&base), &cur).is_empty());
    }

    /// Definition 100 with a plain subgraph (id 50) inside, in two instances.
    fn nested_plain() -> ProjectFile {
        let mk = |outer: u64, inner_sub: u64, gain: u64| {
            let mut g = node("Gain", gain);
            g.def_node = Some(1);
            let mut s = sub(inner_sub, None, level(vec![g], &[]));
            s.def_node = Some(2);
            s.inner_graph.as_mut().unwrap().nodes[0].def_node = Some(3);
            sub(outer, Some(100), level(vec![s], &[]))
        };
        level(vec![mk(1, 50, 51), mk(2, 60, 61)], &[])
    }

    #[test]
    fn a_nested_plain_subgraph_is_followed_by_correspondence() {
        let base = nested_plain();
        let mut cur = base.clone();
        let deep = cur.nodes[0].inner_graph.as_mut().unwrap().nodes[0].inner_graph.as_mut().unwrap();
        set_param(&mut deep.nodes[0], 0, 0.7);
        let steps = plan(Some(&base), &cur);
        assert_eq!(steps.len(), 1);
        assert!(!steps[0].rebuild, "the same shape all the way down is pushed in place");
        let r = steps[0].result.inner_graph.as_ref().unwrap();
        assert_eq!(r.nodes[0].id, 60);
        let deep = r.nodes[0].inner_graph.as_ref().unwrap();
        assert_eq!(deep.nodes[0].id, 61);
        assert_eq!(param(&deep.nodes[0], 0), 0.7);

        let mut cur = base.clone();
        cur.nodes[0].inner_graph.as_mut().unwrap().nodes[0].inner_graph.as_mut().unwrap().nodes.push(node("Lfo", 99));
        assert!(plan(Some(&base), &cur)[0].rebuild);
    }

    /// Definition 100 holding an instance of definition 200 (two outer
    /// instances, so two inner ones; a third inner one sits at the root).
    fn nested_linked() -> ProjectFile {
        let inner_def = |id: u64, gain: u64| {
            let mut g = node("Gain", gain);
            g.def_node = Some(1);
            sub(id, Some(200), level(vec![g], &[]))
        };
        let outer = |id: u64, inner_id: u64, gain: u64| {
            let mut n = inner_def(inner_id, gain);
            n.def_node = Some(5);
            sub(id, Some(100), level(vec![n], &[]))
        };
        level(vec![outer(1, 11, 111), outer(2, 12, 112), inner_def(3, 113)], &[])
    }

    #[test]
    fn an_edit_in_a_nested_definition_goes_to_all_its_instances_and_not_by_the_outer_one() {
        let base = nested_linked();
        let mut cur = base.clone();
        set_param(&mut cur.nodes[0].inner_graph.as_mut().unwrap().nodes[0].inner_graph.as_mut().unwrap().nodes[0], 0, 0.1);
        let steps = plan(Some(&base), &cur);
        // Definition 100: the outer instances only differ below a nested
        // linked node, which belongs to its own definition, so no step for it.
        assert!(steps.iter().all(|s| s.result.data.as_ref().unwrap()["definition"] == 200), "{steps:?}");
        let mut paths: Vec<_> = steps.iter().map(|s| s.path.clone()).collect();
        paths.sort();
        assert_eq!(paths, vec![vec![2, 12], vec![3]]);
        apply_steps(&mut cur, &steps);
        assert!(plan(Some(&base), &cur).is_empty());
        assert_eq!(param(&cur.nodes[2].inner_graph.as_ref().unwrap().nodes[0], 0), 0.1);
    }

    #[test]
    fn an_instance_inside_a_subgraph_inside_another_instance_syncs_after_the_outer_one_is_rebuilt() {
        let base = nested_linked();
        let mut cur = base.clone();
        // New node in the outer definition (instance 1) and an edit of the inner one at the root.
        cur.nodes[0].inner_graph.as_mut().unwrap().nodes.push(node("Lfo", 88));
        set_param(&mut cur.nodes[2].inner_graph.as_mut().unwrap().nodes[0], 0, 0.3);
        let mut alloc = counter(1000);
        let names = ParamNames::new();
        let steps = plan_sync(Some(&base), &cur, &names, &mut alloc);
        // Outer: instance 2 gets the Lfo (rebuild); inner: instances 11 and 12 follow the one at the root.
        assert!(steps.iter().any(|s| s.path == vec![2] && s.rebuild));
        apply_steps(&mut cur, &steps);
        let again = plan_sync(Some(&base), &cur, &names, &mut alloc);
        apply_steps(&mut cur, &again);
        assert!(plan_sync(Some(&base), &cur, &names, &mut alloc).is_empty());
        let outer2 = cur.nodes[1].inner_graph.as_ref().unwrap();
        assert_eq!(outer2.nodes.len(), 2);
        assert_eq!(param(&outer2.nodes[0].inner_graph.as_ref().unwrap().nodes[0], 0), 0.3);
        assert_eq!(param(&cur.nodes[0].inner_graph.as_ref().unwrap().nodes[0].inner_graph.as_ref().unwrap().nodes[0], 0), 0.3);
    }

    #[test]
    fn a_rebuild_takes_its_nested_targets_with_it() {
        let base = nested_linked();
        let mut cur = base.clone();
        cur.nodes[0].inner_graph.as_mut().unwrap().nodes.push(node("Lfo", 88));
        set_param(&mut cur.nodes[0].inner_graph.as_mut().unwrap().nodes[0].inner_graph.as_mut().unwrap().nodes[0], 0, 0.1);
        let steps = plan(Some(&base), &cur);
        assert!(steps.iter().all(|s| !(s.path.len() > 1 && s.path[0] == 2)), "{steps:?}");
    }

    #[test]
    fn counting_listing_and_unlinking() {
        let mut p = nested_linked();
        let count = |p: &ProjectFile, d: u64| instances(p).iter().filter(|i| i.definition == d).count();
        assert_eq!((count(&p, 100), count(&p, 200)), (2, 3));
        assert_eq!(node_at(&p, &[1, 11]).map(|n| n.id), Some(11));
        assert_eq!(make_unique(&mut p, &[3]), None);
        assert_eq!(count(&p, 200), 2);
        assert_eq!(definition_of(&p.nodes[2]), None);
        // The one that is left alone is plain again.
        assert_eq!(make_unique(&mut p, &[1]), Some(100));
        assert_eq!(count(&p, 100), 0);
        assert_eq!(definition_of(&p.nodes[1]), None);
    }

    #[test]
    fn a_definition_cannot_contain_itself() {
        let nests = |ancestors: &[u64], n: &SavedNode| {
            let mut inside = HashSet::new();
            definitions_in(n.data.as_ref(), n.inner_graph.as_ref(), &mut inside);
            ancestors.iter().any(|d| inside.contains(d))
        };
        let inner = sub(5, Some(100), level(vec![], &[]));
        assert!(nests(&[100], &inner));
        assert!(!nests(&[200], &inner));
        let wrapper = sub(6, None, level(vec![inner.clone()], &[]));
        assert!(nests(&[100], &wrapper), "also through nesting");
        let p = level(vec![sub(1, Some(100), level(vec![inner], &[]))], &[]);
        assert!(has_self_nesting(&p));
        assert!(!has_self_nesting(&pair()));
        assert!(!has_self_nesting(&nested_linked()));
    }

    fn plain(id: u64, gain_id: u64, v: f64) -> SavedNode {
        let mut g = node("Gain", gain_id);
        set_param(&mut g, 0, v);
        let mut k = node("Const Value", gain_id + 1);
        k.def_node = None;
        sub(id, None, level(vec![g, k], &[(gain_id + 1, 0, gain_id, 0)]))
    }

    #[test]
    fn identical_subgraphs_are_linked_by_correspondence() {
        let mut p = level(vec![plain(1, 10, 0.5), plain(2, 20, 0.5), plain(3, 30, 0.9)], &[]);
        let r = link_identical(&mut p, &ParamNames::new());
        assert_eq!(r, LinkReport { groups: 1, instances: 2 });
        let d = definition_of(&p.nodes[0]);
        assert!(d.is_some());
        assert_eq!(definition_of(&p.nodes[1]), d);
        assert_eq!(definition_of(&p.nodes[2]), None);
        let a = inner(&p, 0);
        let b = inner(&p, 1);
        assert_eq!(a.nodes[0].def_node, b.nodes[0].def_node);
        assert_eq!(a.nodes[1].def_node, b.nodes[1].def_node);
        assert_eq!(a.nodes[0].id, 10, "ids are not touched");
        assert!(plan(None, &p).is_empty());
    }

    #[test]
    fn the_comparison_ignores_ids_but_not_values_wires_or_positions() {
        let names = ParamNames::new();
        let canon = |s: &SavedNode| canonical_level(s.inner_graph.as_ref().unwrap(), &names).to_string();
        assert_eq!(canon(&plain(1, 10, 0.5)), canon(&plain(2, 70, 0.5)));
        assert_ne!(canon(&plain(1, 10, 0.5)), canon(&plain(2, 70, 0.6)));
        let mut moved = plain(2, 70, 0.5);
        moved.inner_graph.as_mut().unwrap().nodes[0].pos = [1.0, 1.0];
        assert_ne!(canon(&plain(1, 10, 0.5)), canon(&moved));
        let mut unwired = plain(2, 70, 0.5);
        unwired.inner_graph.as_mut().unwrap().connections.clear();
        assert_ne!(canon(&plain(1, 10, 0.5)), canon(&unwired));
        let mut rewired = plain(2, 70, 0.5);
        rewired.inner_graph.as_mut().unwrap().connections[0].to_port = 1;
        assert_ne!(canon(&plain(1, 10, 0.5)), canon(&rewired));
        let mut other_type = plain(2, 70, 0.5);
        other_type.inner_graph.as_mut().unwrap().nodes[1].type_name = "Lfo".into();
        assert_ne!(canon(&plain(1, 10, 0.5)), canon(&other_type));
    }

    #[test]
    fn the_comparison_looks_through_nesting_and_ignores_panel_values() {
        let names = ParamNames::new();
        let wrap = |id: u64, inner_id: u64, v: f64| sub(id, None, level(vec![plain(inner_id, inner_id * 10, v)], &[]));
        let canon = |s: &SavedNode| canonical_level(s.inner_graph.as_ref().unwrap(), &names).to_string();
        assert_eq!(canon(&wrap(1, 2, 0.5)), canon(&wrap(7, 8, 0.5)));
        assert_ne!(canon(&wrap(1, 2, 0.5)), canon(&wrap(7, 8, 0.6)));

        let on_panel = |v: f64| {
            let mut s = plain(1, 10, v);
            s.inner_graph.as_mut().unwrap().nodes[0].panel = Some(PanelItem { order: 0, whole: true, params: vec![] });
            s
        };
        assert_eq!(canon(&on_panel(0.1)), canon(&on_panel(0.9)));
    }

    #[test]
    fn linking_is_idempotent_and_joins_an_existing_definition() {
        let mut p = level(vec![plain(1, 10, 0.5), plain(2, 20, 0.5), plain(3, 30, 0.5)], &[]);
        set_definition(&mut p.nodes[0], Some(9));
        set_definition(&mut p.nodes[1], Some(9));
        let r = link_identical(&mut p, &ParamNames::new());
        assert_eq!(r.groups, 1);
        assert!(p.nodes.iter().all(|n| definition_of(n) == Some(9)));
        assert_eq!(link_identical(&mut p, &ParamNames::new()), LinkReport::default());
    }

    #[test]
    fn linking_finds_groups_on_inner_levels_too() {
        let mut p = level(
            vec![sub(1, None, level(vec![plain(11, 110, 0.5), plain(12, 120, 0.5)], &[])), plain(2, 20, 0.5)],
            &[],
        );
        let r = link_identical(&mut p, &ParamNames::new());
        assert_eq!(r.instances, 3);
        assert!(!has_self_nesting(&p));
    }

    #[test]
    fn a_library_update_replaces_every_instance_and_keeps_front_panel_values() {
        use crate::engine::types::PanelItem;
        let mut p = pair();
        for i in 0..2 {
            let g = p.nodes[i].inner_graph.as_mut().unwrap();
            g.nodes[0].panel = Some(PanelItem { order: 0, whole: true, params: vec![] });
            set_param(&mut g.nodes[0], 0, 0.1 + i as f64);
        }
        // The library version: the same two nodes (their ids stand for the
        // identities), a third one, and a changed value on each.
        let mut lib = p.nodes[0].clone();
        {
            let g = lib.inner_graph.as_mut().unwrap();
            set_param(&mut g.nodes[0], 0, 0.9);
            set_param(&mut g.nodes[1], 0, 0.3);
            g.nodes.push(node("Lfo", 12));
        }
        let steps = plan_update(&p, 100, &lib, &ParamNames::new(), &mut counter(1000));
        assert_eq!(steps.len(), 2);
        apply_steps(&mut p, &steps);
        for i in 0..2 {
            let g = inner(&p, i);
            assert_eq!(g.nodes.len(), 3);
            assert_eq!(param(&g.nodes[0], 0), 0.1 + i as f64, "the panel value stays with the instance");
            assert_eq!(param(&g.nodes[1], 0), 0.3, "the rest follows the library");
        }
        assert!(plan_sync(None, &p, &ParamNames::new(), &mut counter(3000)).is_empty());
    }

    #[test]
    fn definition_ids_are_distinct_and_stripping_plains_the_tree() {
        let a = new_definition_id();
        let b = new_definition_id();
        assert_ne!(a, b);
        assert!(a < (1 << 53));
        let mut p = nested_linked();
        strip_definitions(&mut p);
        assert!(instances(&p).is_empty());
    }

    #[test]
    fn the_hash_follows_the_content() {
        let a = pair();
        let mut b = a.clone();
        assert_eq!(graph_hash(&a), graph_hash(&b));
        set_param(&mut b.nodes[0].inner_graph.as_mut().unwrap().nodes[0], 0, 0.1);
        assert_ne!(graph_hash(&a), graph_hash(&b));
    }

    #[test]
    fn the_tag_round_trips_through_json_and_old_files_have_none() {
        let n = sub(1, Some(7), level(vec![], &[]));
        let json = serde_json::to_string(&n).unwrap();
        let back: SavedNode = serde_json::from_str(&json).unwrap();
        assert_eq!(definition_of(&back), Some(7));
        let mut with = node("Gain", 3);
        with.def_node = Some(9);
        assert!(serde_json::to_string(&with).unwrap().contains("def_node"));
        assert!(!serde_json::to_string(&node("Gain", 3)).unwrap().contains("def_node"));
        let old: SavedNode = serde_json::from_str(r#"{"type_name":"Gain","id":1,"pos":[0,0],"params":[]}"#).unwrap();
        assert_eq!(old.def_node, None);
    }
}
