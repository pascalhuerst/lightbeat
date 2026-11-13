use crate::engine::nodes::display::led_display::LedDisplayProcessNode;
use crate::engine::nodes::display::value_display::ValueDisplayProcessNode;
use crate::engine::types::*;
use crate::engine::graph::UI_SYNC_EVERY_N_TICKS;
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

    fn rebuild_plan(&mut self, input_ports: &[PortDef], output_ports: &[PortDef]) {
        self.plan = RoutePlan::build(
            &self.nodes,
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
        }
        self.plan_dirty = false;
    }

    /// Takes everything the UI pushed, in a single lock per node, before
    /// `process` so an edit lands on the tick it arrives and before the
    /// connection scan because `load_data` can rebuild a node's ports.
    fn take_pending(&mut self) {
        self.disabled_mask.clear();
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
            if let Some(list) = promoted_update {
                self.promoted[i].adopt(&list, node.as_ref());
            }
            if let Some(set) = snapshots_update {
                self.promoted[i].adopt_snapshots(set);
            }
            for (idx, val) in pending {
                node.set_param(idx, val);
            }
            if let Some(config) = config_update {
                node.load_data(&config);
            }
            if let Some(index) = recall {
                self.promoted[i].recall_snapshot(index, node.as_mut());
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
                crate::perf::record_inner_node(
                    node.type_name(),
                    started.elapsed().as_nanos() as u64,
                );
            }
        } else {
            for (i, node) in self.nodes.iter_mut().enumerate() {
                if !self.disabled_mask[i] {
                    node.process();
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
            &mut self.promoted,
            &self.bridge_in,
            &mut self.bridge_out,
        );
        for (i, node) in self.nodes.iter_mut().enumerate() {
            self.promoted[i].step_snapshots(node.as_mut());
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

    /// The structural edits, for the root graph and for every subgraph.
    pub fn apply(&mut self, cmd: SubgraphInnerCmd) {
        self.plan_dirty = true;
        match cmd {
            SubgraphInnerCmd::AddNode { node, shared } => {
                shared.lock().default_params = node.params();
                self.shared_states.push(shared);
                self.nodes.push(node);
                self.promoted.push(PromotedInputs::default());
                self.index_stale = true;
            }
            SubgraphInnerCmd::RemoveNode(id) => {
                if let Some(idx) = self.nodes.iter().position(|n| n.node_id() == id) {
                    self.nodes.remove(idx);
                    self.shared_states.remove(idx);
                    self.promoted.remove(idx);
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
                if let Some(node) = self.nodes.iter_mut().find(|n| n.node_id() == node_id) {
                    node.load_data(&data);
                }
            }
            SubgraphInnerCmd::NotifyConnect {
                node_id,
                input_port,
                source_type,
            } => {
                if let Some(node) = self.nodes.iter_mut().find(|n| n.node_id() == node_id) {
                    node.on_connect(input_port, source_type);
                }
            }
            SubgraphInnerCmd::NotifyDisconnect {
                node_id,
                input_port,
            } => {
                if let Some(node) = self.nodes.iter_mut().find(|n| n.node_id() == node_id) {
                    node.on_disconnect(input_port);
                }
            }
        }
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
            // One `params()` per node (it allocates), shared by the promoted
            // entries' refresh and the UI copy.
            let params = (!disabled || self.promoted[i].wants_refresh()).then(|| node.params());
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
            if disabled {
                continue;
            }
            let total_out = total_channels(node.outputs());
            for ch in 0..total_out.min(shared.outputs.len()) {
                shared.outputs[ch] = node.read_output(ch);
            }
            let total_in = total_channels(node.inputs());
            for ch in 0..total_in.min(shared.inputs.len()) {
                shared.inputs[ch] = node.read_input(ch);
            }
            if let Some(params) = params {
                shared.current_params = params;
            }
            if sync_save_data {
                shared.save_data = node.save_data();
            }
            node.update_display(&mut shared);
        }
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
        if let Some(b) = cfg.locked {
            self.locked = b;
        }
        if let Some(s) = cfg.macro_description {
            self.macro_description = s;
        }
        if let Some(s) = cfg.macro_path {
            self.macro_path = s;
        }
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
    fn a_partial_update_only_touches_what_it_names() {
        let mut n = SubgraphProcessNode::new(NodeId(1));
        n.load_data(&serde_json::json!({"name": "A", "locked": true}));
        n.load_data(&serde_json::json!({"macro_path": "x"}));
        assert_eq!(n.name, "A");
        assert!(n.locked);
        assert_eq!(n.macro_path, "x");
    }
}

