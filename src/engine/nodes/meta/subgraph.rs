use crate::engine::nodes::display::led_display::LedDisplayProcessNode;
use crate::engine::nodes::display::value_display::ValueDisplayProcessNode;
use crate::engine::types::*;
use crate::engine::graph::UI_SYNC_EVERY_N_TICKS;
use crate::engine::promoted::PromotedInputs;
use crate::engine::routing::RoutePlan;
use crate::engine::sync_hint::SaveDataGate;
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

pub const PORT_TYPE_NAMES: &[&str] = &["Logic", "Phase", "Untyped", "Color", "Position", "Palette", "Gradient"];

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
}

/// Reserved NodeIds for bridge endpoints.
pub const BRIDGE_IN_NODE_ID: NodeId = NodeId(u64::MAX - 1);
pub const BRIDGE_OUT_NODE_ID: NodeId = NodeId(u64::MAX);

impl InnerGraph {
    pub fn new() -> Self {
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

    /// Run one tick of the inner graph. Mirrors `EngineGraph::tick`; keep the
    /// two in step when changing either.
    pub fn tick(&mut self, input_ports: &[PortDef], output_ports: &[PortDef]) {
        let mut clock = crate::perf::PhaseClock::start();
        if self.index_stale {
            self.rebuild_node_index();
        }

        // 0. Take everything the UI pushed, in a single lock per node, before
        // `process` so an edit lands on the tick it arrives and before the
        // connection scan because `load_data` can rebuild a node's ports.
        self.disabled_mask.clear();
        for (i, node) in self.nodes.iter_mut().enumerate() {
            let (disabled, pending, config_update, promoted_update) = {
                let mut shared = self.shared_states[i].lock().unwrap();
                (
                    shared.disabled,
                    std::mem::take(&mut shared.pending_params),
                    shared.pending_config.take(),
                    shared.pending_promoted.take(),
                )
            };
            self.disabled_mask.push(disabled);
            if promoted_update.is_some() || !pending.is_empty() || config_update.is_some() {
                self.plan_dirty = true;
            }
            if let Some(list) = promoted_update {
                self.promoted[i].adopt(&list, node.as_ref());
            }
            for (idx, val) in pending {
                node.set_param(idx, val);
            }
            if let Some(config) = config_update {
                node.load_data(&config);
            }
        }

        clock.lap("[inner] 0 take pending");

        // 1. Re-resolve routing and the per-node connected-input bitmap (the
        //    "input-overrides-param" pattern inside subgraphs) only when
        //    something that could change them did.
        if self.plan_dirty
            || !self.plan.is_current(&self.nodes, input_ports, output_ports)
        {
            self.rebuild_plan(input_ports, output_ports);
        }

        clock.lap("[inner] 1 connected bitmap");

        // 2. Process inner nodes, skipping disabled ones.
        if crate::perf::node_profiling_enabled() {
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

        clock.lap("[inner] 2 process (nodes)");

        // 3. Propagate connections (including bridge connections).
        self.plan.run(
            &mut self.nodes,
            &mut self.promoted,
            &self.bridge_in,
            &mut self.bridge_out,
        );
        clock.lap("[inner] 3 propagate");

        // 4. Mirror state for the UI at roughly frame rate. `params()`,
        // `save_data()` and `update_display()` allocate, and the UI only sees
        // one update in 16 anyway.
        self.sync_counter += 1;
        if self.sync_counter >= UI_SYNC_EVERY_N_TICKS {
            self.sync_counter = 0;
            self.sync_shared_state();
        }
        clock.lap("[inner] 4 ui sync");
    }

    fn sync_shared_state(&mut self) {
        let sync_save_data = self.save_data_gate.due();
        for (i, node) in self.nodes.iter().enumerate() {
            let mut shared = self.shared_states[i].lock().unwrap();
            shared.inputs_connected.clear();
            shared
                .inputs_connected
                .extend_from_slice(&self.plan.connected[i]);
            shared.non_promotable = node.non_promotable_params();
            if self.disabled_mask[i] {
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
            shared.current_params = node.params();
            if sync_save_data {
                shared.save_data = node.save_data();
            }
            node.update_display(&mut shared);
        }
        for (i, node) in self.nodes.iter().enumerate() {
            self.promoted[i].refresh(node.as_ref());
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
        if let Some(name) = data.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(inputs) = data.get("inputs").and_then(|v| v.as_array()) {
            self.ext_inputs = inputs
                .iter()
                .filter_map(|v| serde_json::from_value::<SubgraphPortDef>(v.clone()).ok())
                .map(|p| p.to_port_def())
                .collect();
        }
        if let Some(outputs) = data.get("outputs").and_then(|v| v.as_array()) {
            self.ext_outputs = outputs
                .iter()
                .filter_map(|v| serde_json::from_value::<SubgraphPortDef>(v.clone()).ok())
                .map(|p| p.to_port_def())
                .collect();
        }
        if let Some(b) = data.get("locked").and_then(|v| v.as_bool()) {
            self.locked = b;
        }
        if let Some(s) = data.get("macro_description").and_then(|v| v.as_str()) {
            self.macro_description = s.to_string();
        }
        if let Some(s) = data.get("macro_path").and_then(|v| v.as_str()) {
            self.macro_path = s.to_string();
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
        Some(serde_json::json!({
            "name": self.name,
            "inputs": inputs,
            "outputs": outputs,
            "locked": self.locked,
            "macro_description": self.macro_description,
            "macro_path": self.macro_path,
        }))
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

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
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
        let Some(sg) = node.as_any_mut().downcast_mut::<SubgraphProcessNode>() else {
            return;
        };
        sg.apply_inner_cmd_at_path(&path[1..], cmd);
    }

    /// Apply an inner command to this subgraph's inner graph.
    pub fn apply_inner_cmd(&mut self, cmd: SubgraphInnerCmd) {
        self.inner.plan_dirty = true;
        match cmd {
            SubgraphInnerCmd::AddNode { node, shared } => {
                self.inner.shared_states.push(shared);
                self.inner.nodes.push(node);
                self.inner.promoted.push(PromotedInputs::default());
                self.inner.index_stale = true;
            }
            SubgraphInnerCmd::RemoveNode(id) => {
                if let Some(idx) = self.inner.nodes.iter().position(|n| n.node_id() == id) {
                    self.inner.nodes.remove(idx);
                    self.inner.shared_states.remove(idx);
                    self.inner.promoted.remove(idx);
                    self.inner.index_stale = true;
                    self.inner
                        .connections
                        .retain(|c| c.from.node != id && c.to.node != id);
                }
            }
            SubgraphInnerCmd::AddConnection(conn) => {
                if !self.inner.connections.contains(&conn) {
                    self.inner.connections.push(conn);
                }
            }
            SubgraphInnerCmd::RemoveConnectionTo(to) => {
                self.inner.connections.retain(|c| c.to != to);
            }
            SubgraphInnerCmd::LoadData { node_id, data } => {
                if let Some(node) = self.inner.nodes.iter_mut().find(|n| n.node_id() == node_id) {
                    node.load_data(&data);
                }
            }
            SubgraphInnerCmd::NotifyConnect {
                node_id,
                input_port,
                source_type,
            } => {
                if let Some(node) = self.inner.nodes.iter_mut().find(|n| n.node_id() == node_id) {
                    node.on_connect(input_port, source_type);
                }
            }
            SubgraphInnerCmd::NotifyDisconnect {
                node_id,
                input_port,
            } => {
                if let Some(node) = self.inner.nodes.iter_mut().find(|n| n.node_id() == node_id) {
                    node.on_disconnect(input_port);
                }
            }
        }
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
        g.shared_states[1].lock().unwrap().pending_promoted = Some(vec![PromotedParam {
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
