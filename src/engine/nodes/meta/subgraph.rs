use crate::engine::nodes::display::led_display::LedDisplayProcessNode;
use crate::engine::nodes::display::value_display::ValueDisplayProcessNode;
use crate::engine::types::*;
use crate::engine::graph::UI_SYNC_EVERY_N_TICKS;
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
    /// Scratch buffers reused across ticks so the hot loop does not allocate.
    per_node_connected: Vec<Vec<bool>>,
    disabled_mask: Vec<bool>,
    write_buf: Vec<(usize, usize, f32)>,
    bridge_write_buf: Vec<(usize, f32)>,
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
            per_node_connected: Vec::new(),
            disabled_mask: Vec::new(),
            write_buf: Vec::new(),
            bridge_write_buf: Vec::new(),
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

    /// Run one tick of the inner graph. Mirrors `EngineGraph::tick`; keep the
    /// two in step when changing either.
    pub fn tick(&mut self, input_ports: &[PortDef], output_ports: &[PortDef]) {
        if self.index_stale {
            self.rebuild_node_index();
        }

        // 0. Take everything the UI pushed, in a single lock per node, before
        // `process` so an edit lands on the tick it arrives and before the
        // connection scan because `load_data` can rebuild a node's ports.
        self.disabled_mask.clear();
        for (i, node) in self.nodes.iter_mut().enumerate() {
            let (disabled, pending, config_update) = {
                let mut shared = self.shared_states[i].lock().unwrap();
                (
                    shared.disabled,
                    std::mem::take(&mut shared.pending_params),
                    shared.pending_config.take(),
                )
            };
            self.disabled_mask.push(disabled);
            for (idx, val) in pending {
                node.set_param(idx, val);
            }
            if let Some(config) = config_update {
                node.load_data(&config);
            }
        }

        // 1. Per-node connected-input bitmap so the "input-overrides-param"
        //    pattern works inside subgraphs.
        self.per_node_connected.resize_with(self.nodes.len(), Vec::new);
        self.per_node_connected.truncate(self.nodes.len());
        for (i, node) in self.nodes.iter().enumerate() {
            let buf = &mut self.per_node_connected[i];
            buf.clear();
            buf.resize(node.inputs().len(), false);
        }
        for conn in &self.connections {
            if conn.to.node == BRIDGE_OUT_NODE_ID { continue; }
            if let Some(&dst_idx) = self.node_index.get(&conn.to.node)
                && let Some(slot) = self.per_node_connected[dst_idx].get_mut(conn.to.index) {
                    *slot = true;
                }
        }
        for (i, node) in self.nodes.iter_mut().enumerate() {
            node.set_input_connections(&self.per_node_connected[i]);
        }

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

        // 3. Propagate connections (including bridge connections).
        let mut writes = std::mem::take(&mut self.write_buf);
        let mut bridge_out_writes = std::mem::take(&mut self.bridge_write_buf);
        writes.clear();
        bridge_out_writes.clear();

        for conn in &self.connections {
            let src_node = if conn.from.node == BRIDGE_IN_NODE_ID {
                None
            } else if let Some(&idx) = self.node_index.get(&conn.from.node) {
                Some(idx)
            } else {
                continue;
            };
            let dst_node = if conn.to.node == BRIDGE_OUT_NODE_ID {
                None
            } else if let Some(&idx) = self.node_index.get(&conn.to.node) {
                Some(idx)
            } else {
                continue;
            };

            let src_ports = match src_node {
                Some(idx) => self.nodes[idx].outputs(),
                None => input_ports,
            };
            let dst_ports = match dst_node {
                Some(idx) => self.nodes[idx].inputs(),
                None => output_ports,
            };
            let src_base = port_base_index(src_ports, conn.from.index);
            let dst_base = port_base_index(dst_ports, conn.to.index);
            let src_channels = src_ports
                .get(conn.from.index)
                .map(|p| p.port_type.channel_count())
                .unwrap_or(1);
            let dst_channels = dst_ports
                .get(conn.to.index)
                .map(|p| p.port_type.channel_count())
                .unwrap_or(1);

            for ch in 0..src_channels.min(dst_channels) {
                let val = match src_node {
                    Some(idx) => self.nodes[idx].read_output(src_base + ch),
                    None => self.bridge_in.get(src_base + ch).copied().unwrap_or(0.0),
                };
                match dst_node {
                    Some(idx) => writes.push((idx, dst_base + ch, val)),
                    None => bridge_out_writes.push((dst_base + ch, val)),
                }
            }
        }

        for &(dst_idx, ch, val) in &writes {
            self.nodes[dst_idx].write_input(ch, val);
        }
        for &(ch, val) in &bridge_out_writes {
            if ch < self.bridge_out.len() {
                self.bridge_out[ch] = val;
            }
        }
        self.write_buf = writes;
        self.bridge_write_buf = bridge_out_writes;

        // 4. Mirror state for the UI at roughly frame rate. `params()`,
        // `save_data()` and `update_display()` allocate, and the UI only sees
        // one update in 16 anyway.
        self.sync_counter += 1;
        if self.sync_counter >= UI_SYNC_EVERY_N_TICKS {
            self.sync_counter = 0;
            self.sync_shared_state();
        }
    }

    fn sync_shared_state(&mut self) {
        for (i, node) in self.nodes.iter().enumerate() {
            let mut shared = self.shared_states[i].lock().unwrap();
            shared.inputs_connected.clear();
            shared
                .inputs_connected
                .extend_from_slice(&self.per_node_connected[i]);
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
            shared.save_data = node.save_data();
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
        match cmd {
            SubgraphInnerCmd::AddNode { node, shared } => {
                self.inner.shared_states.push(shared);
                self.inner.nodes.push(node);
                self.inner.index_stale = true;
            }
            SubgraphInnerCmd::RemoveNode(id) => {
                if let Some(idx) = self.inner.nodes.iter().position(|n| n.node_id() == id) {
                    self.inner.nodes.remove(idx);
                    self.inner.shared_states.remove(idx);
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
