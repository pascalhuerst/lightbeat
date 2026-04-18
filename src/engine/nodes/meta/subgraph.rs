use crate::engine::nodes::display::led_display::LedDisplayProcessNode;
use crate::engine::nodes::display::value_display::ValueDisplayProcessNode;
use crate::engine::types::*;
use std::any::Any;

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
        }
    }

    /// Run one tick of the inner graph.
    pub fn tick(&mut self, input_ports: &[PortDef], output_ports: &[PortDef]) {
        // 1. Process all inner nodes.
        for node in self.nodes.iter_mut() {
            node.process();
        }

        // 2. Propagate connections (including bridge connections).
        let mut writes: Vec<(usize, usize, f32)> = Vec::new(); // (dst_node_vec_idx, channel, value)
        let mut bridge_out_writes: Vec<(usize, f32)> = Vec::new();

        for conn in &self.connections {
            // Determine source channel count.
            let src_channels = if conn.from.node == BRIDGE_IN_NODE_ID {
                input_ports
                    .get(conn.from.index)
                    .map(|p| p.port_type.channel_count())
                    .unwrap_or(1)
            } else if let Some(idx) = self
                .nodes
                .iter()
                .position(|n| n.node_id() == conn.from.node)
            {
                self.nodes[idx]
                    .outputs()
                    .get(conn.from.index)
                    .map(|p| p.port_type.channel_count())
                    .unwrap_or(1)
            } else {
                continue;
            };

            // Determine dest channel count.
            let dst_channels = if conn.to.node == BRIDGE_OUT_NODE_ID {
                output_ports
                    .get(conn.to.index)
                    .map(|p| p.port_type.channel_count())
                    .unwrap_or(1)
            } else if let Some(idx) = self.nodes.iter().position(|n| n.node_id() == conn.to.node) {
                self.nodes[idx]
                    .inputs()
                    .get(conn.to.index)
                    .map(|p| p.port_type.channel_count())
                    .unwrap_or(1)
            } else {
                continue;
            };

            let channels = src_channels.min(dst_channels);

            for ch in 0..channels {
                // Read source value.
                let val = if conn.from.node == BRIDGE_IN_NODE_ID {
                    let base = port_base_index(input_ports, conn.from.index);
                    self.bridge_in.get(base + ch).copied().unwrap_or(0.0)
                } else if let Some(idx) = self
                    .nodes
                    .iter()
                    .position(|n| n.node_id() == conn.from.node)
                {
                    let base = port_base_index(self.nodes[idx].outputs(), conn.from.index);
                    self.nodes[idx].read_output(base + ch)
                } else {
                    0.0
                };

                // Queue destination write.
                if conn.to.node == BRIDGE_OUT_NODE_ID {
                    let base = port_base_index(output_ports, conn.to.index);
                    bridge_out_writes.push((base + ch, val));
                } else if let Some(dst_idx) =
                    self.nodes.iter().position(|n| n.node_id() == conn.to.node)
                {
                    let base = port_base_index(self.nodes[dst_idx].inputs(), conn.to.index);
                    writes.push((dst_idx, base + ch, val));
                }
            }
        }

        // Apply writes.
        for (dst_idx, ch, val) in writes {
            self.nodes[dst_idx].write_input(ch, val);
        }
        for (ch, val) in bridge_out_writes {
            if ch < self.bridge_out.len() {
                self.bridge_out[ch] = val;
            }
        }

        // 3. Drain pending param changes from UI (via shared state).
        for (i, node) in self.nodes.iter_mut().enumerate() {
            let (pending, config_update) = {
                let mut shared = self.shared_states[i].lock().unwrap();
                let pending = std::mem::take(&mut shared.pending_params);
                let config = shared.pending_config.take();
                (pending, config)
            };
            for (idx, val) in pending {
                node.set_param(idx, val);
            }
            if let Some(config) = config_update {
                node.load_data(&config);
            }
        }

        // 4. Update shared state for UI.
        for (i, node) in self.nodes.iter().enumerate() {
            let mut shared = self.shared_states[i].lock().unwrap();
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
            }
            SubgraphInnerCmd::RemoveNode(id) => {
                if let Some(idx) = self.inner.nodes.iter().position(|n| n.node_id() == id) {
                    self.inner.nodes.remove(idx);
                    self.inner.shared_states.remove(idx);
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
