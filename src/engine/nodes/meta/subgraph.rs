use crate::engine::nodes::display::led_display::LedDisplayProcessNode;
use crate::engine::nodes::display::value_display::ValueDisplayProcessNode;
use crate::engine::types::*;
use crate::engine::graph::UI_SYNC_EVERY_N_TICKS;
use crate::engine::lanes::{Lane, changed_params};
use crate::engine::promoted::PromotedInputs;
use crate::engine::routing::RoutePlan;
use crate::engine::sync_hint::SaveDataGate;
use crate::perf::NodeProfiler;
use std::any::Any;
use std::collections::HashMap;

/// Walk the inner graph (recursively descending into nested subgraphs) and
/// collect every Value/LED Display node's current state. Order is depth-first,
/// which keeps a node's list visually stable as the user reorders inner nodes.
fn collect_inner_value_displays(inner: &InnerGraph, out: &mut Vec<InnerValueDisplay>) {
    for node in inner.nodes.iter() {
        let any = node.as_any();
        if let Some(vd) = any.downcast_ref::<ValueDisplayProcessNode>() {
            out.push(InnerValueDisplay {
                name: vd.name().to_string(),
                value: vd.value(),
                mode: 0,
            });
        } else if let Some(led) = any.downcast_ref::<LedDisplayProcessNode>() {
            out.push(InnerValueDisplay {
                name: led.name().to_string(),
                value: led.value(),
                mode: 1,
            });
        } else if let Some(sg) = any.downcast_ref::<SubgraphProcessNode>() {
            collect_inner_value_displays(&sg.inner, out);
        }
    }
}

// ---------------------------------------------------------------------------
// Display state
// ---------------------------------------------------------------------------

pub struct SubgraphDisplay {
    pub inner_node_count: usize,
    pub locked: bool,
    /// Mirror of every Value Display node found anywhere inside this subgraph
    /// (recursive). Surfaced so the parent can render them on the subgraph
    /// node itself, without the user having to navigate in.
    pub inner_value_displays: Vec<InnerValueDisplay>,
}

#[derive(Clone)]
pub struct InnerValueDisplay {
    pub name: String,
    pub value: f32,
    pub mode: u8,
}

// ---------------------------------------------------------------------------
// Port definition for external interface (serializable)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubgraphPortDef {
    pub name: String,
    pub port_type_idx: usize, // index into PortType variants
}

/// The subgraph's own settings, as saved and as the widget pushes them. Each
/// key is optional so a partial update only touches what it names.
#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub struct SubgraphConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "crate::engine::config_json::lenient_vec")]
    pub inputs: Option<Vec<SubgraphPortDef>>,
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "crate::engine::config_json::lenient_vec")]
    pub outputs: Option<Vec<SubgraphPortDef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macro_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macro_path: Option<String>,
    /// Hash of the library file's graph when this instance was placed or last
    /// updated (`linked::graph_hash`). Like `definition`, a config always
    /// states it: absent means none, so it can be cleared again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macro_hash: Option<String>,
    /// Subgraphs with the same value are instances of one definition
    /// (`linked.rs`); absent means a plain subgraph.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<u64>,
    /// How many lanes each port carries. Never saved: the UI derives the
    /// widths from the wires and pushes them, and the node's own `save_data`
    /// leaves them out. Absent leaves the widths as they are; a config that
    /// names the ports without them makes every port one lane wide.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_lanes: Option<Vec<usize>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_lanes: Option<Vec<usize>>,
}

/// Give each port the width `lanes` names for it (1 when it names none).
pub fn apply_lanes(ports: &mut [PortDef], lanes: &[usize]) {
    for (i, port) in ports.iter_mut().enumerate() {
        port.lanes = lanes.get(i).copied().unwrap_or(1).clamp(1, MAX_LANES);
    }
}

impl SubgraphConfig {
    /// Malformed data loads as an empty update, which changes nothing.
    pub fn from_value(v: &serde_json::Value) -> Self {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

impl SubgraphPortDef {
    pub fn to_port_def(&self) -> PortDef {
        PortDef::new(&self.name, idx_to_port_type(self.port_type_idx))
    }
}

pub fn idx_to_port_type(idx: usize) -> PortType {
    match idx {
        0 => PortType::Logic,
        1 => PortType::Phase,
        2 => PortType::Untyped,
        3 => PortType::Color,
        4 => PortType::Position,
        5 => PortType::Palette,
        6 => PortType::Gradient,
        _ => PortType::Untyped,
    }
}

pub fn port_type_to_idx(pt: PortType) -> usize {
    match pt {
        PortType::Logic => 0,
        PortType::Phase => 1,
        PortType::Untyped => 2,
        PortType::Color => 3,
        PortType::Position => 4,
        PortType::Palette => 5,
        PortType::Gradient => 6,
        PortType::Any => 2,
    }
}

pub const PORT_TYPE_NAMES: &[&str] = &["Logic", "Phase", "Number", "Color", "Position", "Palette", "Gradient"];

// ---------------------------------------------------------------------------
// Inner graph (owns nodes + connections, runs synchronously)
// ---------------------------------------------------------------------------

/// A self-contained graph that runs inside a subgraph node.
/// The "Graph Input" and "Graph Output" are not nodes — they're just
/// value buffers at reserved indices. Inner nodes read from/write to them
/// via connections that reference special node IDs.
pub struct InnerGraph {
    pub nodes: Vec<Box<dyn ProcessNode>>,
    pub shared_states: Vec<SharedState>,
    pub connections: Vec<Connection>,
    /// Values bridged IN from the parent (corresponds to ext_inputs).
    pub bridge_in: Vec<f32>,
    /// Values bridged OUT to the parent (corresponds to ext_outputs).
    pub bridge_out: Vec<f32>,
    /// NodeId -> position in `nodes`. Rebuilt lazily after structural edits;
    /// without it every connection cost a linear scan per channel per tick.
    node_index: HashMap<NodeId, usize>,
    index_stale: bool,
    /// Params the user turned into input ports, parallel to `nodes`.
    promoted: Vec<PromotedInputs>,
    /// The extra copies of nodes that run as lanes (a bus on one of their
    /// inputs), parallel to `nodes`; empty for the usual node. Everything that
    /// addresses a node (settings, config, connect hooks, promoted lists,
    /// snapshot recalls, disabling) addresses its copies the same way.
    lanes: Vec<Vec<Lane>>,
    /// Whether any node has copies, so the plain graph pays one flag a tick
    /// for lanes and nothing per node.
    any_lanes: bool,
    plan: RoutePlan,
    save_data_gate: SaveDataGate,
    /// Set by anything that can change routing or a node's ports; checked
    /// each tick. Port counts changing on their own are caught separately.
    plan_dirty: bool,
    /// Scratch buffers reused across ticks so the hot loop does not allocate.
    disabled_mask: Vec<bool>,
    sync_counter: u32,
    /// Only the root graph has one; see `process_nodes`.
    profiler: Option<NodeProfiler>,
    /// Phase names for the node profile, which tell the root from the inner graphs.
    laps: [&'static str; 5],
}

/// Reserved NodeIds for bridge endpoints.
pub const BRIDGE_IN_NODE_ID: NodeId = NodeId(u64::MAX - 1);
pub const BRIDGE_OUT_NODE_ID: NodeId = NodeId(u64::MAX);

impl InnerGraph {
    pub fn new() -> Self {
        Self::with_profiling(None, [
            "[inner] 0 take pending",
            "[inner] 1 connected bitmap",
            "[inner] 2 process (nodes)",
            "[inner] 3 propagate",
            "[inner] 4 ui sync",
        ])
    }

    /// The top level graph: no bridges, its own node profiler.
    pub fn new_root() -> Self {
        Self::with_profiling(NodeProfiler::from_env(), [
            "[root] 0 take pending",
            "[root] 1 connected bitmap",
            "[root] 2 process (nodes)",
            "[root] 3 propagate",
            "[root] 4 ui sync",
        ])
    }

    fn with_profiling(profiler: Option<NodeProfiler>, laps: [&'static str; 5]) -> Self {
        Self {
            nodes: Vec::new(),
            shared_states: Vec::new(),
            connections: Vec::new(),
            bridge_in: Vec::new(),
            bridge_out: Vec::new(),
            node_index: HashMap::new(),
            index_stale: false,
            promoted: Vec::new(),
            lanes: Vec::new(),
            any_lanes: false,
            plan: RoutePlan::default(),
            save_data_gate: SaveDataGate::default(),
            plan_dirty: true,
            disabled_mask: Vec::new(),
            sync_counter: 0,
            profiler,
            laps,
        }
    }

    fn rebuild_node_index(&mut self) {
        self.node_index.clear();
        for (i, node) in self.nodes.iter().enumerate() {
            self.node_index.insert(node.node_id(), i);
        }
        self.index_stale = false;
    }

    /// `lanes` is kept the length of `nodes`; commands and tests that add
    /// nodes need not know about it.
    fn size_lane_slots(&mut self) {
        if self.lanes.len() != self.nodes.len() {
            self.lanes.resize_with(self.nodes.len(), Vec::new);
        }
    }

    fn note_lanes(&mut self) {
        self.any_lanes = self.lanes.iter().any(|l| !l.is_empty());
    }

    fn rebuild_plan(&mut self, input_ports: &[PortDef], output_ports: &[PortDef]) {
        self.plan = RoutePlan::build(
            &self.nodes,
            &self.lanes,
            &self.node_index,
            &self.connections,
            &self.promoted,
            input_ports,
            output_ports,
            Some(BRIDGE_IN_NODE_ID),
            Some(BRIDGE_OUT_NODE_ID),
        );
        for (i, node) in self.nodes.iter_mut().enumerate() {
            let native = node.inputs().len();
            node.set_input_connections(self.plan.native_connected(i, native));
            if self.promoted[i].len() > 0 {
                self.promoted[i].note_wiring(self.plan.promoted_connected(i, native));
            }
            for lane in self.lanes[i].iter_mut() {
                lane.node.set_input_connections(self.plan.native_connected(i, native));
                if lane.promoted.len() > 0 {
                    lane.promoted.note_wiring(self.plan.promoted_connected(i, native));
                }
            }
        }
        self.plan_dirty = false;
    }

    /// Takes everything the UI pushed, in a single lock per node, before
    /// `process` so an edit lands on the tick it arrives and before the
    /// connection scan because `load_data` can rebuild a node's ports.
    fn take_pending(&mut self) {
        self.disabled_mask.clear();
        self.size_lane_slots();
        for (i, node) in self.nodes.iter_mut().enumerate() {
            let (disabled, pending, config_update, promoted_update, snapshots_update, recall) = {
                let mut shared = self.shared_states[i].lock();
                (
                    shared.disabled,
                    std::mem::take(&mut shared.pending_params),
                    shared.pending_config.take(),
                    shared.pending_promoted.take(),
                    shared.pending_snapshots.take(),
                    shared.pending_recall.take(),
                )
            };
            self.disabled_mask.push(disabled);
            if promoted_update.is_some() || !pending.is_empty() || config_update.is_some() {
                self.plan_dirty = true;
            }
            let copies = &mut self.lanes[i];
            if let Some(list) = promoted_update {
                self.promoted[i].adopt(&list, node.as_ref());
                for lane in copies.iter_mut() {
                    lane.promoted.adopt(&list, lane.node.as_ref());
                }
            }
            if let Some(set) = snapshots_update {
                self.promoted[i].adopt_snapshots(set);
            }
            for (idx, val) in pending {
                for lane in copies.iter_mut() {
                    lane.node.set_param(idx, val.clone());
                }
                node.set_param(idx, val);
            }
            if let Some(config) = config_update {
                for lane in copies.iter_mut() {
                    lane.node.load_data(&config);
                }
                node.load_data(&config);
            }
            if let Some(index) = recall {
                let before = (!copies.is_empty()).then(|| node.params());
                self.promoted[i].recall_snapshot(index, node.as_mut());
                if let Some(before) = before {
                    let moved = changed_params(&before, &node.params());
                    for lane in copies.iter_mut() {
                        for (idx, val) in &moved {
                            lane.node.set_param(*idx, val.clone());
                        }
                    }
                }
            }
        }
    }

    /// Processes all nodes except disabled ones, which keep their previous
    /// output values so downstream signal flow freezes instead of dropping to
    /// zero. The root graph has its own profiler; nodes inside subgraphs
    /// report to the shared sample list instead.
    fn process_nodes(&mut self) {
        if let Some(profile) = &mut self.profiler {
            for (i, node) in self.nodes.iter_mut().enumerate() {
                if self.disabled_mask[i] {
                    continue;
                }
                let started = std::time::Instant::now();
                node.process();
                if self.any_lanes {
                    for lane in self.lanes[i].iter_mut() {
                        lane.node.process();
                    }
                }
                profile.record(node.type_name(), started.elapsed().as_nanos() as u64);
            }
            profile.end_tick();
        } else if crate::perf::node_profiling_enabled() {
            for (i, node) in self.nodes.iter_mut().enumerate() {
                if self.disabled_mask[i] {
                    continue;
                }
                let started = std::time::Instant::now();
                node.process();
                if self.any_lanes {
                    for lane in self.lanes[i].iter_mut() {
                        lane.node.process();
                    }
                }
                crate::perf::record_inner_node(
                    node.type_name(),
                    started.elapsed().as_nanos() as u64,
                );
            }
        } else {
            for (i, node) in self.nodes.iter_mut().enumerate() {
                if !self.disabled_mask[i] {
                    node.process();
                    if self.any_lanes {
                        for lane in self.lanes[i].iter_mut() {
                            lane.node.process();
                        }
                    }
                }
            }
        }
    }

    /// Run one tick of the graph: the root graph and every subgraph's inner
    /// graph go through here. `after_propagate` runs once the signals have
    /// moved and before the UI mirror; the root uses it for the hover probe.
    pub fn tick_with(
        &mut self,
        input_ports: &[PortDef],
        output_ports: &[PortDef],
        after_propagate: impl FnOnce(&InnerGraph),
    ) {
        let mut clock = crate::perf::PhaseClock::start();
        if self.index_stale {
            self.rebuild_node_index();
        }

        // 0. Pending params, config, promoted lists and snapshot recalls.
        self.take_pending();
        clock.lap(self.laps[0]);

        // 1. Re-resolve routing and the per-node "which input ports are
        //    connected" bitmap (the "input-overrides-param" pattern) only when
        //    something that could change them did.
        if self.plan_dirty
            || !self.plan.is_current(&self.nodes, input_ports, output_ports)
        {
            self.rebuild_plan(input_ports, output_ports);
        }
        clock.lap(self.laps[1]);

        // 2. Process the nodes.
        self.process_nodes();
        clock.lap(self.laps[2]);

        // 3. Propagate connections (including bridge connections). Multi-channel
        // ports (Color=3, Position=2) expand to multiple float copies.
        self.plan.run(
            &mut self.nodes,
            &mut self.lanes,
            &mut self.promoted,
            &self.bridge_in,
            &mut self.bridge_out,
        );
        for (i, node) in self.nodes.iter_mut().enumerate() {
            // A fade moves lane 0's settings through the node; the copies are
            // brought along from what changed.
            let before = (self.any_lanes && !self.lanes[i].is_empty() && self.promoted[i].snapshot_running()).then(|| node.params());
            self.promoted[i].step_snapshots(node.as_mut());
            if let Some(before) = before {
                let moved = changed_params(&before, &node.params());
                for lane in self.lanes[i].iter_mut() {
                    for (idx, val) in &moved {
                        lane.node.set_param(*idx, val.clone());
                    }
                }
            }
        }
        clock.lap(self.laps[3]);

        after_propagate(self);

        // 4. Mirror state for the UI at roughly frame rate. `params()`,
        // `save_data()` and `update_display()` allocate, and the UI only sees
        // one update in 16 anyway.
        self.sync_counter += 1;
        if self.sync_counter >= UI_SYNC_EVERY_N_TICKS {
            self.sync_counter = 0;
            self.sync_shared_state();
        }
        clock.lap(self.laps[4]);
    }

    /// Defensive guard against a malformed path; the user is unlikely to nest
    /// this deep by hand.
    const MAX_SUBGRAPH_NEST_DEPTH: usize = 32;

    /// A command from the UI, applied to this graph as the root. What is not
    /// about the graph (the DMX interfaces) is handed back.
    pub fn apply_command(&mut self, cmd: EngineCommand) -> Option<EngineCommand> {
        let inner = match cmd {
            EngineCommand::AddNode { node, shared } => SubgraphInnerCmd::AddNode { node, shared },
            EngineCommand::RemoveNode(id) => SubgraphInnerCmd::RemoveNode(id),
            EngineCommand::AddConnection(conn) => SubgraphInnerCmd::AddConnection(conn),
            EngineCommand::RemoveConnectionTo(to) => SubgraphInnerCmd::RemoveConnectionTo(to),
            EngineCommand::NotifyConnect { node_id, input_port, source_type } => {
                SubgraphInnerCmd::NotifyConnect { node_id, input_port, source_type }
            }
            EngineCommand::NotifyDisconnect { node_id, input_port } => {
                SubgraphInnerCmd::NotifyDisconnect { node_id, input_port }
            }
            EngineCommand::LoadData { node_id, data } => SubgraphInnerCmd::LoadData { node_id, data },
            EngineCommand::SetLanes { node_id, extra } => SubgraphInnerCmd::SetLanes { node_id, extra },
            EngineCommand::RemoveAllNodes => {
                self.clear();
                return None;
            }
            EngineCommand::SubgraphInnerCommand { subgraph_path, command } => {
                self.mark_plan_dirty();
                self.apply_at_path(&subgraph_path, *command);
                return None;
            }
            other @ EngineCommand::SetInterfaces(_) => return Some(other),
        };
        self.apply(inner);
        None
    }

    fn apply_at_path(&mut self, path: &[NodeId], cmd: SubgraphInnerCmd) {
        if path.is_empty() {
            return;
        }
        if path.len() > Self::MAX_SUBGRAPH_NEST_DEPTH {
            eprintln!(
                "Subgraph nesting depth {} exceeds limit {}; dropping command",
                path.len(), Self::MAX_SUBGRAPH_NEST_DEPTH,
            );
            return;
        }
        let Some(node) = self.nodes.iter_mut().find(|n| n.node_id() == path[0]) else {
            return;
        };
        let Some(sg) = node.as_any_mut().and_then(|a| a.downcast_mut::<SubgraphProcessNode>()) else {
            return;
        };
        sg.apply_inner_cmd_at_path(&path[1..], cmd);
    }

    /// The structural edits, for the root graph and for every subgraph.
    pub fn apply(&mut self, cmd: SubgraphInnerCmd) {
        self.plan_dirty = true;
        match cmd {
            SubgraphInnerCmd::AddNode { node, shared } => {
                shared.lock().default_params = node.params();
                self.size_lane_slots();
                self.shared_states.push(shared);
                self.nodes.push(node);
                self.promoted.push(PromotedInputs::default());
                self.lanes.push(Vec::new());
                self.index_stale = true;
            }
            SubgraphInnerCmd::RemoveNode(id) => {
                if let Some(idx) = self.nodes.iter().position(|n| n.node_id() == id) {
                    self.size_lane_slots();
                    self.nodes.remove(idx);
                    self.shared_states.remove(idx);
                    self.promoted.remove(idx);
                    self.lanes.remove(idx);
                    self.note_lanes();
                    self.index_stale = true;
                    self.connections
                        .retain(|c| c.from.node != id && c.to.node != id);
                }
            }
            SubgraphInnerCmd::AddConnection(conn) => {
                if !self.connections.contains(&conn) {
                    self.connections.push(conn);
                }
            }
            SubgraphInnerCmd::RemoveConnectionTo(to) => {
                self.connections.retain(|c| c.to != to);
            }
            SubgraphInnerCmd::LoadData { node_id, data } => {
                self.size_lane_slots();
                if let Some(i) = self.nodes.iter().position(|n| n.node_id() == node_id) {
                    for lane in self.lanes[i].iter_mut() {
                        lane.node.load_data(&data);
                    }
                    self.nodes[i].load_data(&data);
                }
            }
            SubgraphInnerCmd::NotifyConnect {
                node_id,
                input_port,
                source_type,
            } => {
                self.size_lane_slots();
                if let Some(i) = self.nodes.iter().position(|n| n.node_id() == node_id) {
                    for lane in self.lanes[i].iter_mut() {
                        lane.node.on_connect(input_port, source_type);
                    }
                    self.nodes[i].on_connect(input_port, source_type);
                }
            }
            SubgraphInnerCmd::NotifyDisconnect {
                node_id,
                input_port,
            } => {
                self.size_lane_slots();
                if let Some(i) = self.nodes.iter().position(|n| n.node_id() == node_id) {
                    for lane in self.lanes[i].iter_mut() {
                        lane.node.on_disconnect(input_port);
                    }
                    self.nodes[i].on_disconnect(input_port);
                }
            }
            SubgraphInnerCmd::SetLanes { node_id, extra } => self.set_lanes(node_id, extra),
        }
    }

    /// Run a node as `extra.len() + 1` lanes. The copies it already has stay
    /// (so growing a bus does not reset the lanes that were there); new ones
    /// start as copies of lane 0: its settings, its saved state, and the
    /// connect hooks of the wires that end at it.
    fn set_lanes(&mut self, node_id: NodeId, extra: Vec<Box<dyn ProcessNode>>) {
        self.size_lane_slots();
        let Some(i) = self.nodes.iter().position(|n| n.node_id() == node_id) else { return };
        self.lanes[i].truncate(extra.len());
        let have = self.lanes[i].len();
        let params = self.nodes[i].params();
        let saved = self.nodes[i].save_data();
        let wired: Vec<(usize, PortType)> = self
            .connections
            .iter()
            .filter(|c| c.to.node == node_id)
            .filter_map(|c| {
                let src = self.nodes.iter().find(|n| n.node_id() == c.from.node)?;
                Some((c.to.index, src.outputs().get(c.from.index)?.port_type))
            })
            .collect();
        for mut copy in extra.into_iter().skip(have) {
            if let Some(data) = &saved {
                copy.load_data(data);
            }
            for (idx, def) in params.iter().enumerate() {
                copy.set_param(idx, def.as_value());
            }
            for &(port, source_type) in &wired {
                if port < copy.inputs().len() {
                    copy.on_connect(port, source_type);
                }
            }
            let mut promoted = PromotedInputs::default();
            promoted.adopt(self.promoted[i].wanted(), copy.as_ref());
            self.lanes[i].push(Lane { node: copy, promoted });
        }
        self.note_lanes();
        self.plan_dirty = true;
    }

    /// Any command, however deep, makes the root re-resolve its routing.
    pub fn mark_plan_dirty(&mut self) {
        self.plan_dirty = true;
    }

    pub fn clear(&mut self) {
        self.plan_dirty = true;
        self.nodes.clear();
        self.shared_states.clear();
        self.promoted.clear();
        self.lanes.clear();
        self.any_lanes = false;
        self.connections.clear();
        self.node_index.clear();
        self.index_stale = false;
    }

    /// Where a node sits in `nodes`; only valid after a tick has rebuilt a stale index.
    pub fn index_of(&self, id: NodeId) -> Option<usize> {
        self.node_index.get(&id).copied()
    }

    pub fn tick(&mut self, input_ports: &[PortDef], output_ports: &[PortDef]) {
        self.tick_with(input_ports, output_ports, |_| {});
    }

    fn sync_shared_state(&mut self) {
        let sync_save_data = self.save_data_gate.due();
        for (i, node) in self.nodes.iter().enumerate() {
            let disabled = self.disabled_mask[i];
            // A laned node shows one lane to the UI, the one the user is
            // looking at; its saved state is always lane 0's.
            let copies = &self.lanes[i];
            let view = if copies.is_empty() { 0 } else { self.shared_states[i].lock().view_lane.min(copies.len()) };
            let shown: &dyn ProcessNode = if view == 0 { node.as_ref() } else { copies[view - 1].node.as_ref() };
            // One `params()` per node (it allocates), shared by the promoted
            // entries' refresh and the UI copy.
            let params = (!disabled || self.promoted[i].wants_refresh()).then(|| shown.params());
            if self.promoted[i].wants_refresh()
                && let Some(params) = &params
            {
                self.promoted[i].refresh(params);
            }
            let (active, fading, progress) = self.promoted[i].snapshot_state();

            let mut shared = self.shared_states[i].lock();
            shared.inputs_connected.clear();
            shared
                .inputs_connected
                .extend_from_slice(&self.plan.connected[i]);
            let non_promotable = node.non_promotable_params();
            if !std::ptr::eq(shared.non_promotable, non_promotable) {
                shared.non_promotable = non_promotable;
            }
            shared.snapshot_active = active;
            shared.snapshot_fading = fading;
            shared.snapshot_progress = progress;
            shared.lanes = copies.len() + 1;
            if disabled {
                continue;
            }
            let total_out = total_channels(node.outputs());
            for ch in 0..total_out.min(shared.outputs.len()) {
                shared.outputs[ch] = shown.read_output(ch);
            }
            let total_in = total_channels(node.inputs());
            for ch in 0..total_in.min(shared.inputs.len()) {
                shared.inputs[ch] = shown.read_input(ch);
            }
            if let Some(params) = params {
                shared.current_params = params;
            }
            if sync_save_data {
                shared.save_data = node.save_data();
            }
            shown.update_display(&mut shared);
        }
    }

    /// What the hover scope records for a port: the lane the UI shows.
    pub fn probe_read(&self, i: usize, output: bool, channel: usize) -> f32 {
        let view = if self.lanes.get(i).is_none_or(|l| l.is_empty()) {
            0
        } else {
            self.shared_states[i].lock().view_lane.min(self.lanes[i].len())
        };
        let node: &dyn ProcessNode = if view == 0 { self.nodes[i].as_ref() } else { self.lanes[i][view - 1].node.as_ref() };
        if output { node.read_output(channel) } else { node.read_input(channel) }
    }
}

// ---------------------------------------------------------------------------
// Subgraph process node
// ---------------------------------------------------------------------------

pub struct SubgraphProcessNode {
    id: NodeId,
    pub name: String,
    ext_inputs: Vec<PortDef>,
    ext_outputs: Vec<PortDef>,
    ext_input_values: Vec<f32>,
    pub inner: InnerGraph,
    /// UI-only flag (round-tripped through save/load + display so the
    /// widget side can mirror it).
    pub locked: bool,
    /// Macro metadata — only set when this Subgraph is a locked macro
    /// instance. Empty strings for regular (non-macro) subgraphs.
    pub macro_description: String,
    pub macro_path: String,
    pub macro_hash: Option<String>,
    pub definition: Option<u64>,
}

impl SubgraphProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            name: "Subgraph".to_string(),
            ext_inputs: Vec::new(),
            ext_outputs: Vec::new(),
            ext_input_values: Vec::new(),
            inner: InnerGraph::new(),
            locked: false,
            macro_description: String::new(),
            macro_path: String::new(),
            macro_hash: None,
            definition: None,
        }
    }

    fn rebuild_from_port_defs(&mut self) {
        let in_ch = total_channels(&self.ext_inputs);
        let out_ch = total_channels(&self.ext_outputs);
        self.ext_input_values.resize(in_ch, 0.0);
        self.inner.bridge_in.resize(in_ch, 0.0);
        self.inner.bridge_out.resize(out_ch, 0.0);
    }
}

impl ProcessNode for SubgraphProcessNode {
    fn max_lanes(&self) -> usize { 1 }

    fn node_id(&self) -> NodeId {
        self.id
    }
    fn type_name(&self) -> &'static str {
        "Subgraph"
    }
    fn inputs(&self) -> &[PortDef] {
        &self.ext_inputs
    }
    fn outputs(&self) -> &[PortDef] {
        &self.ext_outputs
    }

    fn write_input(&mut self, ch: usize, v: f32) {
        if ch < self.ext_input_values.len() {
            self.ext_input_values[ch] = v;
        }
    }
    fn read_input(&self, ch: usize) -> f32 {
        self.ext_input_values.get(ch).copied().unwrap_or(0.0)
    }

    fn process(&mut self) {
        // Copy external inputs to inner bridge.
        let copy_len = self.inner.bridge_in.len().min(self.ext_input_values.len());
        self.inner.bridge_in[..copy_len].copy_from_slice(&self.ext_input_values[..copy_len]);

        // Tick inner graph.
        self.inner.tick(&self.ext_inputs, &self.ext_outputs);
    }

    fn read_output(&self, ch: usize) -> f32 {
        self.inner.bridge_out.get(ch).copied().unwrap_or(0.0)
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        // Port configuration from widget.
        let cfg = SubgraphConfig::from_value(data);
        if let Some(name) = cfg.name {
            self.name = name;
        }
        if let Some(inputs) = cfg.inputs {
            self.ext_inputs = inputs.iter().map(SubgraphPortDef::to_port_def).collect();
        }
        if let Some(outputs) = cfg.outputs {
            self.ext_outputs = outputs.iter().map(SubgraphPortDef::to_port_def).collect();
        }
        if let Some(lanes) = &cfg.input_lanes {
            apply_lanes(&mut self.ext_inputs, lanes);
        }
        if let Some(lanes) = &cfg.output_lanes {
            apply_lanes(&mut self.ext_outputs, lanes);
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
        self.rebuild_from_port_defs();
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        let inputs: Vec<SubgraphPortDef> = self
            .ext_inputs
            .iter()
            .map(|p| SubgraphPortDef {
                name: p.name.clone(),
                port_type_idx: port_type_to_idx(p.port_type),
            })
            .collect();
        let outputs: Vec<SubgraphPortDef> = self
            .ext_outputs
            .iter()
            .map(|p| SubgraphPortDef {
                name: p.name.clone(),
                port_type_idx: port_type_to_idx(p.port_type),
            })
            .collect();
        Some(SubgraphConfig {
            name: Some(self.name.clone()),
            inputs: Some(inputs),
            outputs: Some(outputs),
            locked: Some(self.locked),
            macro_description: Some(self.macro_description.clone()),
            macro_path: Some(self.macro_path.clone()),
            macro_hash: self.macro_hash.clone(),
            definition: self.definition,
            input_lanes: None,
            output_lanes: None,
        }.to_value())
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        let mut inner_value_displays = Vec::new();
        collect_inner_value_displays(&self.inner, &mut inner_value_displays);
        shared.display = Some(Box::new(SubgraphDisplay {
            inner_node_count: self.inner.nodes.len(),
            locked: self.locked,
            inner_value_displays,
        }));
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl SubgraphProcessNode {
    /// Apply an inner command at the given remaining sub-path. An empty
    /// `path` targets this subgraph; any further elements descend into
    /// nested subgraphs by `NodeId`. Used by `EngineGraph::apply_subgraph_inner_cmd`
    /// for arbitrarily-deep nesting.
    pub fn apply_inner_cmd_at_path(&mut self, path: &[NodeId], cmd: SubgraphInnerCmd) {
        if path.is_empty() {
            self.apply_inner_cmd(cmd);
            return;
        }
        let next_id = path[0];
        let Some(node) = self.inner.nodes.iter_mut().find(|n| n.node_id() == next_id) else {
            return;
        };
        let Some(sg) = node.as_any_mut().and_then(|a| a.downcast_mut::<SubgraphProcessNode>()) else {
            return;
        };
        sg.apply_inner_cmd_at_path(&path[1..], cmd);
    }

    /// Apply an inner command to this subgraph's inner graph.
    pub fn apply_inner_cmd(&mut self, cmd: SubgraphInnerCmd) {
        self.inner.apply(cmd);
    }
}

#[cfg(test)]
mod promoted_tests {
    use super::*;
    use crate::engine::nodes::math::constant::ConstantProcessNode;
    use crate::engine::nodes::transport::lfo::LfoProcessNode;
    use crate::engine::promoted::{PromotedKind, PromotedParam};

    fn port(node: u64, index: usize, dir: PortDir) -> PortId {
        PortId { node: NodeId(node), index, dir }
    }

    /// A Constant wired into the LFO's promoted "Rate" port, which sits right
    /// after the LFO's own inputs.
    fn graph_with_promoted_rate() -> (InnerGraph, usize) {
        let mut g = InnerGraph::new();
        let lfo = LfoProcessNode::new(NodeId(2));
        let native = lfo.inputs().len();
        g.nodes.push(Box::new(ConstantProcessNode::new(NodeId(1), PortType::Untyped, 0.0)));
        g.nodes.push(Box::new(lfo));
        for _ in 0..2 {
            g.shared_states.push(new_shared_state(8, 8));
            g.promoted.push(PromotedInputs::default());
        }
        g.index_stale = true;
        g.shared_states[1].lock().pending_promoted = Some(vec![PromotedParam {
            name: "Rate".to_string(),
            kind: PromotedKind::Float,
        }]);
        g.connections.push(Connection {
            from: port(1, 0, PortDir::Output),
            to: port(2, native, PortDir::Input),
        });
        (g, native)
    }

    fn lfo_rate(g: &InnerGraph) -> f32 {
        match &g.nodes[1].params()[0] {
            ParamDef::Float { value, .. } => *value,
            _ => panic!("Rate is not the first param"),
        }
    }

    #[test]
    fn wire_into_promoted_port_sets_the_param() {
        let (mut g, _) = graph_with_promoted_rate();
        g.nodes[0].set_param(0, ParamValue::Float(3.0));
        g.tick(&[], &[]);
        assert_eq!(lfo_rate(&g), 3.0);

        g.nodes[0].set_param(0, ParamValue::Float(500.0));
        g.tick(&[], &[]);
        // Clamped to the param's own range rather than passed through.
        assert_eq!(lfo_rate(&g), 20.0);
    }

    #[test]
    fn promoted_port_counts_as_connected() {
        let (mut g, native) = graph_with_promoted_rate();
        g.tick(&[], &[]);
        assert_eq!(g.plan.connected[1].len(), native + 1);
        assert!(g.plan.connected[1][native]);
    }

    #[test]
    fn unwired_promoted_param_keeps_its_value() {
        let (mut g, _) = graph_with_promoted_rate();
        g.connections.clear();
        g.nodes[1].set_param(0, ParamValue::Float(7.0));
        g.tick(&[], &[]);
        assert_eq!(lfo_rate(&g), 7.0);
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    use crate::engine::nodes::math::gain::GainProcessNode;

    fn port(node: NodeId, index: usize, dir: PortDir) -> PortId {
        PortId { node, index, dir }
    }

    /// bridge in -> Gain (x1.0 by default) -> bridge out.
    fn passthrough() -> (InnerGraph, Vec<PortDef>, Vec<PortDef>) {
        let mut g = InnerGraph::new();
        let gain_id = NodeId(1);
        g.nodes.push(Box::new(GainProcessNode::new(gain_id)));
        g.shared_states.push(new_shared_state(4, 4));
        g.promoted.push(PromotedInputs::default());
        g.index_stale = true;
        g.bridge_in = vec![0.0];
        g.bridge_out = vec![0.0];
        g.connections.push(Connection {
            from: port(BRIDGE_IN_NODE_ID, 0, PortDir::Output),
            to: port(gain_id, 0, PortDir::Input),
        });
        g.connections.push(Connection {
            from: port(gain_id, 0, PortDir::Output),
            to: port(BRIDGE_OUT_NODE_ID, 0, PortDir::Input),
        });
        let ports = vec![PortDef::new("x", PortType::Untyped)];
        (g, ports.clone(), ports)
    }

    #[test]
    fn values_cross_the_bridge_one_node_per_tick() {
        let (mut g, ins, outs) = passthrough();
        g.bridge_in[0] = 0.75;
        g.tick(&ins, &outs);
        g.tick(&ins, &outs);
        assert_eq!(g.bridge_out[0], 0.75);
    }

    #[test]
    fn a_removed_wire_stops_carrying_values() {
        let (mut g, ins, outs) = passthrough();
        g.tick(&ins, &outs);
        // What `apply_inner_cmd` does for a RemoveConnectionTo.
        g.connections.retain(|c| c.to.node != BRIDGE_OUT_NODE_ID);
        g.plan_dirty = true;
        g.bridge_in[0] = 0.5;
        g.bridge_out[0] = 0.0;
        g.tick(&ins, &outs);
        g.tick(&ins, &outs);
        // The cached plan must have been rebuilt without the removed wire.
        assert_eq!(g.bridge_out[0], 0.0);
    }

    #[test]
    fn plan_notices_a_change_in_bridge_channels() {
        let (mut g, ins, outs) = passthrough();
        g.tick(&ins, &outs);
        assert!(g.plan.is_current(&g.nodes, &ins, &outs));
        let wider = vec![PortDef::new("c", PortType::Color)];
        assert!(!g.plan.is_current(&g.nodes, &wider, &outs));
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn the_hand_written_save_shape_loads_and_saves_unchanged() {
        let saved = serde_json::json!({
            "name": "Chase",
            "inputs": [{"name": "in", "port_type_idx": 1}],
            "outputs": [{"name": "out", "port_type_idx": 2}, {"name": "gate", "port_type_idx": 0}],
            "locked": true,
            "macro_description": "walks a light along",
            "macro_path": "fx/chase",
        });
        let mut n = SubgraphProcessNode::new(NodeId(1));
        n.load_data(&saved);
        assert_eq!(n.name, "Chase");
        assert_eq!(n.ext_inputs.len(), 1);
        assert_eq!(n.ext_outputs.len(), 2);
        assert!(n.locked);
        assert_eq!(n.macro_path, "fx/chase");
        assert_eq!(n.save_data(), Some(saved));
    }

    #[test]
    fn widths_size_the_bridge_and_are_never_saved() {
        let mut n = SubgraphProcessNode::new(NodeId(1));
        n.load_data(&serde_json::json!({
            "inputs": [{"name": "in", "port_type_idx": 3}],
            "outputs": [{"name": "out", "port_type_idx": 2}],
            "input_lanes": [4], "output_lanes": [8],
        }));
        assert_eq!((n.ext_inputs[0].lanes, n.ext_outputs[0].lanes), (4, 8));
        assert_eq!((n.inner.bridge_in.len(), n.inner.bridge_out.len()), (12, 8), "lanes x channels of the type");
        let saved = n.save_data().unwrap();
        assert!(saved.get("input_lanes").is_none() && saved.get("output_lanes").is_none());

        // Ports named again without widths are one lane wide.
        n.load_data(&serde_json::json!({"inputs": [{"name": "in", "port_type_idx": 3}]}));
        assert_eq!(n.ext_inputs[0].lanes, 1);
    }

    #[test]
    fn a_partial_update_only_touches_what_it_names() {
        let mut n = SubgraphProcessNode::new(NodeId(1));
        n.load_data(&serde_json::json!({"name": "A", "locked": true}));
        n.load_data(&serde_json::json!({"macro_path": "x"}));
        assert_eq!(n.name, "A");
        assert!(n.locked);
        assert_eq!(n.macro_path, "x");
    }
}


#[cfg(test)]
mod bus_tests {
    use super::*;
    use crate::engine::nodes::math::bus::{BusPickProcessNode, BusReshapeProcessNode, PICK_PARAM_INDEX};
    use crate::engine::nodes::math::constant::ConstantProcessNode;

    fn port(node: u64, index: usize, dir: PortDir) -> PortId {
        PortId { node: NodeId(node), index, dir }
    }

    /// A node with one output that holds fixed values.
    struct Source {
        id: NodeId,
        outputs: Vec<PortDef>,
        values: Vec<f32>,
    }

    impl Source {
        fn new(id: u64, port_type: PortType, values: &[f32]) -> Self {
            Self { id: NodeId(id), outputs: vec![PortDef::new("out", port_type)], values: values.to_vec() }
        }
    }

    impl ProcessNode for Source {
        fn node_id(&self) -> NodeId { self.id }
        fn type_name(&self) -> &'static str { "Source" }
        fn inputs(&self) -> &[PortDef] { &[] }
        fn outputs(&self) -> &[PortDef] { &self.outputs }
        fn process(&mut self) {}
        fn read_output(&self, ch: usize) -> f32 { self.values.get(ch).copied().unwrap_or(0.0) }
    }

    fn add(g: &mut InnerGraph, node: Box<dyn ProcessNode>) {
        g.nodes.push(node);
        g.shared_states.push(new_shared_state(64, 64));
        g.promoted.push(PromotedInputs::default());
        g.index_stale = true;
    }

    fn wire(g: &mut InnerGraph, from: (u64, usize), to: (u64, usize)) {
        g.connections.push(Connection { from: port(from.0, from.1, PortDir::Output), to: port(to.0, to.1, PortDir::Input) });
    }

    /// Four constants (ids 10..14, values 1..4) into a Merge (id 1).
    fn merged() -> InnerGraph {
        let mut g = InnerGraph::new();
        add(&mut g, Box::new(BusReshapeProcessNode::merge(NodeId(1))));
        for k in 0..4u64 {
            add(&mut g, Box::new(ConstantProcessNode::new(NodeId(10 + k), PortType::Untyped, k as f32 + 1.0)));
            wire(&mut g, (10 + k, 0), (1, k as usize));
        }
        g.nodes[0].set_param(0, ParamValue::Choice(crate::engine::nodes::math::multiplex::type_to_index(PortType::Untyped)));
        g
    }

    #[test]
    fn a_bus_made_by_merge_comes_apart_in_split_with_every_lane_where_it_was() {
        let mut g = merged();
        add(&mut g, Box::new(BusReshapeProcessNode::split(NodeId(2))));
        g.nodes.last_mut().unwrap().set_param(0, ParamValue::Choice(3));
        wire(&mut g, (1, 0), (2, 0));
        for _ in 0..4 {
            g.tick(&[], &[]);
        }
        let split = g.nodes.last().unwrap();
        let out: Vec<f32> = (0..4).map(|k| split.read_output(k)).collect();
        assert_eq!(out, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn a_bus_of_colors_keeps_the_channels_of_each_lane_together() {
        let mut g = InnerGraph::new();
        add(&mut g, Box::new(BusReshapeProcessNode::merge(NodeId(1))));
        let color = crate::engine::nodes::math::multiplex::type_to_index(PortType::Color);
        g.nodes[0].set_param(0, ParamValue::Choice(color));
        for k in 0..4u64 {
            add(&mut g, Box::new(Source::new(10 + k, PortType::Color, &[k as f32, k as f32 + 0.5, 0.25])));
            wire(&mut g, (10 + k, 0), (1, k as usize));
        }
        add(&mut g, Box::new(BusPickProcessNode::new(NodeId(2))));
        g.nodes.last_mut().unwrap().set_param(0, ParamValue::Choice(color));
        g.nodes.last_mut().unwrap().set_param(PICK_PARAM_INDEX, ParamValue::Int(3));
        wire(&mut g, (1, 0), (2, 0));
        for _ in 0..4 {
            g.tick(&[], &[]);
        }
        let pick = g.nodes.last().unwrap();
        assert_eq!([pick.read_output(0), pick.read_output(1), pick.read_output(2)], [3.0, 3.5, 0.25]);
    }
}

#[cfg(test)]
mod lane_tests {
    use super::*;
    use crate::engine::nodes::math::bus::BusReshapeProcessNode;
    use crate::engine::nodes::math::logic_gate::{LogicGateProcessNode, LogicOp};
    use crate::engine::nodes::math::multiplex::type_to_index;
    use crate::engine::nodes::math::schmitt::SchmittTriggerProcessNode;
    use crate::engine::promoted::{PromotedKind, PromotedParam};
    use crate::engine::snapshots::{SnapshotSet, capture};

    const WIDTH: usize = 16;

    fn port(node: u64, index: usize, dir: PortDir) -> PortId {
        PortId { node: NodeId(node), index, dir }
    }

    /// A node with one output that holds a fixed value.
    struct Source {
        id: NodeId,
        outputs: Vec<PortDef>,
        value: f32,
    }

    impl ProcessNode for Source {
        fn node_id(&self) -> NodeId { self.id }
        fn type_name(&self) -> &'static str { "Source" }
        fn inputs(&self) -> &[PortDef] { &[] }
        fn outputs(&self) -> &[PortDef] { &self.outputs }
        fn process(&mut self) {}
        fn read_output(&self, _: usize) -> f32 { self.value }
    }

    fn add(g: &mut InnerGraph, node: Box<dyn ProcessNode>) {
        g.apply(SubgraphInnerCmd::AddNode { node, shared: new_shared_state(64, 64) });
    }

    fn wire(g: &mut InnerGraph, from: (u64, usize), to: (u64, usize)) {
        g.apply(SubgraphInnerCmd::AddConnection(Connection {
            from: port(from.0, from.1, PortDir::Output),
            to: port(to.0, to.1, PortDir::Input),
        }));
    }

    /// A Merge (id `merge`) of WIDTH inputs fed by sources (ids from `first`)
    /// whose values come from `value(lane)`.
    fn bus_of(g: &mut InnerGraph, merge: u64, first: u64, value: impl Fn(usize) -> f32) {
        let mut m = BusReshapeProcessNode::merge(NodeId(merge));
        m.set_param(0, ParamValue::Choice(type_to_index(PortType::Untyped)));
        m.set_param(1, ParamValue::Int(WIDTH as i64));
        add(g, Box::new(m));
        for k in 0..WIDTH {
            let id = first + k as u64;
            add(g, Box::new(Source { id: NodeId(id), outputs: vec![PortDef::new("out", PortType::Untyped)], value: value(k) }));
            wire(g, (id, 0), (merge, k));
        }
    }

    fn lanes_for(g: &mut InnerGraph, id: u64, make: impl Fn() -> Box<dyn ProcessNode>) {
        g.apply(SubgraphInnerCmd::SetLanes { node_id: NodeId(id), extra: (1..WIDTH).map(|_| make()).collect() });
    }

    fn schmitt() -> Box<dyn ProcessNode> {
        Box::new(SchmittTriggerProcessNode::new(NodeId(2)))
    }

    fn and_gate() -> Box<dyn ProcessNode> {
        Box::new(LogicGateProcessNode::new(NodeId(3), LogicOp::And))
    }

    fn index_of(g: &InnerGraph, id: u64) -> usize {
        g.nodes.iter().position(|n| n.node_id() == NodeId(id)).unwrap()
    }

    fn run(g: &mut InnerGraph, ticks: usize) {
        for _ in 0..ticks {
            g.tick(&[], &[]);
        }
    }

    /// Merge(WIDTH; even lanes 0.9, odd 0.1) -> Schmitt -> AND with a single
    /// wire on its second input -> Split.
    fn chain(second: f32) -> InnerGraph {
        let mut g = InnerGraph::new();
        bus_of(&mut g, 1, 100, |k| if k % 2 == 0 { 0.9 } else { 0.1 });
        add(&mut g, schmitt());
        add(&mut g, and_gate());
        let mut split = BusReshapeProcessNode::split(NodeId(4));
        split.set_param(0, ParamValue::Choice(type_to_index(PortType::Logic)));
        split.set_param(1, ParamValue::Int(WIDTH as i64));
        add(&mut g, Box::new(split));
        add(&mut g, Box::new(Source { id: NodeId(50), outputs: vec![PortDef::new("out", PortType::Logic)], value: second }));
        lanes_for(&mut g, 2, schmitt);
        lanes_for(&mut g, 3, and_gate);
        wire(&mut g, (1, 0), (2, 0));
        wire(&mut g, (2, 0), (3, 0));
        wire(&mut g, (50, 0), (3, 1));
        wire(&mut g, (3, 0), (4, 0));
        g
    }

    fn split_outputs(g: &InnerGraph) -> Vec<f32> {
        let split = &g.nodes[index_of(g, 4)];
        (0..WIDTH).map(|k| split.read_output(k)).collect()
    }

    fn even_lanes_high() -> Vec<f32> {
        (0..WIDTH).map(|k| if k % 2 == 0 { 1.0 } else { 0.0 }).collect()
    }

    #[test]
    fn a_bus_through_laned_nodes_with_a_broadcast_input_comes_out_lane_by_lane() {
        let mut g = chain(1.0);
        run(&mut g, 10);
        assert_eq!(split_outputs(&g), even_lanes_high());
        // The single wire is broadcast: switching it off silences every lane.
        let source = index_of(&g, 50);
        g.nodes[source] = Box::new(Source { id: NodeId(50), outputs: vec![PortDef::new("out", PortType::Logic)], value: 0.0 });
        run(&mut g, 10);
        assert_eq!(split_outputs(&g), vec![0.0; WIDTH]);
    }

    #[test]
    fn every_copy_is_its_own_node_with_its_own_state() {
        let mut g = chain(1.0);
        run(&mut g, 10);
        let i = index_of(&g, 2);
        assert_eq!(g.lanes[i].len(), WIDTH - 1);
        for (k, copy) in g.lanes[i].iter().enumerate() {
            let expected = if (k + 1) % 2 == 0 { 1.0 } else { 0.0 };
            assert_eq!(copy.node.read_output(0), expected, "lane {}", k + 1);
        }
    }

    #[test]
    fn a_setting_changed_by_hand_reaches_every_lane() {
        let mut g = chain(1.0);
        run(&mut g, 10);
        let i = index_of(&g, 2);
        // Threshold 0.95 with no hysteresis: 0.9 is below it, so the high lanes fall.
        g.shared_states[i].lock().pending_params.extend([(0, ParamValue::Float(0.95)), (1, ParamValue::Float(0.0))]);
        run(&mut g, 10);
        assert_eq!(split_outputs(&g), vec![0.0; WIDTH]);
        for copy in &g.lanes[i] {
            assert!(matches!(copy.node.params()[0], ParamDef::Float { value, .. } if (value - 0.95).abs() < 1e-6));
        }
    }

    #[test]
    fn saved_state_and_settings_reach_every_lane_through_config() {
        let mut g = chain(1.0);
        let i = index_of(&g, 3);
        g.apply(SubgraphInnerCmd::LoadData { node_id: NodeId(3), data: serde_json::json!({ "input_count": 3 }) });
        assert_eq!(g.nodes[i].inputs().len(), 3);
        assert!(g.lanes[i].iter().all(|l| l.node.inputs().len() == 3));
    }

    #[test]
    fn new_lanes_start_as_copies_of_lane_zero() {
        let mut g = InnerGraph::new();
        let mut first = SchmittTriggerProcessNode::new(NodeId(2));
        first.set_param(0, ParamValue::Float(0.3));
        add(&mut g, Box::new(first));
        lanes_for(&mut g, 2, schmitt);
        for copy in &g.lanes[0] {
            assert!(matches!(copy.node.params()[0], ParamDef::Float { value, .. } if (value - 0.3).abs() < 1e-6));
        }
    }

    #[test]
    fn a_setting_driven_by_a_bus_differs_per_lane() {
        let mut g = InnerGraph::new();
        bus_of(&mut g, 1, 100, |_| 0.5);
        bus_of(&mut g, 5, 200, |k| if k % 2 == 0 { 0.0 } else { 0.9 });
        add(&mut g, schmitt());
        let mut split = BusReshapeProcessNode::split(NodeId(4));
        split.set_param(1, ParamValue::Int(WIDTH as i64));
        add(&mut g, Box::new(split));
        let i = index_of(&g, 2);
        g.shared_states[i].lock().pending_promoted = Some(vec![PromotedParam { name: "Threshold".into(), kind: PromotedKind::Float }]);
        lanes_for(&mut g, 2, schmitt);
        wire(&mut g, (1, 0), (2, 0));
        // The first port past the node's own single input.
        wire(&mut g, (5, 0), (2, 1));
        wire(&mut g, (2, 0), (4, 0));
        run(&mut g, 12);
        // 0.5 is over threshold 0.0 + 0.1 but not over 0.9 + 0.1.
        assert_eq!(split_outputs(&g), even_lanes_high());
    }

    #[test]
    fn a_snapshot_recall_reaches_every_lane() {
        let mut g = InnerGraph::new();
        let mut node = SchmittTriggerProcessNode::new(NodeId(2));
        let mut set = SnapshotSet::default();
        node.set_param(0, ParamValue::Float(0.2));
        set.add(None, capture(&node.params(), node.non_promotable_params()));
        node.set_param(0, ParamValue::Float(0.8));
        set.add(None, capture(&node.params(), node.non_promotable_params()));
        add(&mut g, Box::new(node));
        lanes_for(&mut g, 2, schmitt);
        g.shared_states[0].lock().pending_snapshots = Some(set);
        run(&mut g, 2);
        g.shared_states[0].lock().pending_recall = Some(0);
        run(&mut g, 2);
        let thresholds: Vec<f32> = std::iter::once(&g.nodes[0])
            .chain(g.lanes[0].iter().map(|l| &l.node))
            .map(|n| match n.params()[0] {
                ParamDef::Float { value, .. } => value,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(thresholds, vec![0.2; WIDTH]);
    }

    #[test]
    fn growing_keeps_the_lanes_that_were_there_and_removing_the_source_keeps_lane_zero() {
        let mut g = chain(1.0);
        run(&mut g, 10);
        let i = index_of(&g, 2);
        // Grow by one: lane 2 (a high one) keeps its state.
        g.apply(SubgraphInnerCmd::SetLanes { node_id: NodeId(2), extra: (1..WIDTH + 1).map(|_| schmitt()).collect() });
        assert_eq!(g.lanes[i].len(), WIDTH);
        assert_eq!(g.lanes[i][1].node.read_output(0), 1.0);
        // The source goes: one instance, lane 0's state, and the plan follows.
        g.apply(SubgraphInnerCmd::SetLanes { node_id: NodeId(2), extra: Vec::new() });
        g.apply(SubgraphInnerCmd::RemoveConnectionTo(port(2, 0, PortDir::Input)));
        assert!(g.lanes[i].is_empty());
        run(&mut g, 4);
        assert_eq!(g.nodes[i].read_output(0), 1.0, "lane 0 kept what it had");
    }

    #[test]
    fn a_disabled_node_holds_every_lane() {
        let mut g = chain(1.0);
        run(&mut g, 10);
        let i = index_of(&g, 2);
        g.shared_states[i].lock().disabled = true;
        run(&mut g, 3);
        assert_eq!(g.lanes[i][1].node.read_output(0), 1.0);
    }

    #[test]
    fn the_ui_sees_the_lane_it_looks_at() {
        let mut g = chain(1.0);
        let i = index_of(&g, 2);
        g.shared_states[i].lock().view_lane = 1;
        run(&mut g, 40);
        let shared = g.shared_states[i].lock();
        assert_eq!(shared.lanes, WIDTH);
        // Lane 1 is an odd lane, which sits low; lane 0 would be high.
        assert_eq!(shared.outputs[0], 0.0);
        drop(shared);
        // Past the last lane the view stops at the last one, lane 15, which is odd and so low;
        // make a high one the last by growing to 17 lanes.
        g.apply(SubgraphInnerCmd::SetLanes { node_id: NodeId(2), extra: (1..WIDTH + 1).map(|_| schmitt()).collect() });
        g.shared_states[i].lock().view_lane = 99;
        run(&mut g, 40);
        assert_eq!(g.shared_states[i].lock().lanes, WIDTH + 1);
        assert_eq!(g.probe_read(i, true, 0), g.lanes[i][WIDTH - 1].node.read_output(0));
    }
}
