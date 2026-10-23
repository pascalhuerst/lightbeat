use std::collections::HashMap;
use std::time::Duration;

use ringbuf::traits::Consumer;

use super::CommandConsumer;
use super::nodes::meta::subgraph::SubgraphProcessNode;
use super::promoted::PromotedInputs;
use super::types::*;
use crate::dmx_io::{DmxOutputManager, SharedDmxState, SharedObjectStore};
use crate::perf::{EngineTickTimer, NodeProfiler, SharedPerfStats};

/// The engine-side signal graph. Runs on its own thread at ~1kHz.
pub struct EngineGraph {
    nodes: Vec<Box<dyn ProcessNode>>,
    shared_states: Vec<SharedState>,
    connections: Vec<Connection>,
    dmx: Option<DmxOutputManager>,
    perf: SharedPerfStats,
    /// NodeId to index in `nodes`. Rebuilt only when nodes are added or
    /// removed; without it every connection cost a linear scan per tick.
    node_index: HashMap<NodeId, usize>,
    /// Params the user turned into input ports, parallel to `nodes`.
    promoted: Vec<PromotedInputs>,
    promoted_writes: Vec<(usize, usize, f32)>,
    /// Scratch buffers reused across ticks so the hot loop does not allocate.
    per_node_connected: Vec<Vec<bool>>,
    disabled_mask: Vec<bool>,
    write_buf: Vec<(usize, usize, f32)>,
    sync_counter: u32,
    node_profile: Option<NodeProfiler>,
}

/// How often the engine mirrors its state into shared state for the UI.
/// The UI redraws at ~60 fps, so publishing at 1 kHz threw away 15 of every
/// 16 updates while paying for `params()` allocation, `save_data()` JSON
/// construction and display buffer clones on every one of them.
pub(crate) const UI_SYNC_EVERY_N_TICKS: u32 = 16;

impl EngineGraph {
    pub fn new(
        dmx_shared: SharedDmxState,
        object_store: SharedObjectStore,
        perf: SharedPerfStats,
    ) -> Self {
        Self {
            nodes: Vec::new(),
            shared_states: Vec::new(),
            connections: Vec::new(),
            dmx: Some(DmxOutputManager::new(dmx_shared, object_store)),
            perf,
            node_index: HashMap::new(),
            promoted: Vec::new(),
            promoted_writes: Vec::new(),
            per_node_connected: Vec::new(),
            disabled_mask: Vec::new(),
            write_buf: Vec::new(),
            sync_counter: 0,
            node_profile: NodeProfiler::from_env(),
        }
    }

    fn rebuild_node_index(&mut self) {
        self.node_index.clear();
        for (i, node) in self.nodes.iter().enumerate() {
            self.node_index.insert(node.node_id(), i);
        }
    }

    fn add_node(&mut self, node: Box<dyn ProcessNode>, shared: SharedState) {
        self.shared_states.push(shared);
        self.promoted.push(PromotedInputs::default());
        self.node_index.insert(node.node_id(), self.nodes.len());
        self.nodes.push(node);
    }

    fn remove_node(&mut self, id: NodeId) {
        if let Some(idx) = self.nodes.iter().position(|n| n.node_id() == id) {
            self.nodes.remove(idx);
            self.shared_states.remove(idx);
            self.promoted.remove(idx);
            self.connections
                .retain(|c| c.from.node != id && c.to.node != id);
            // Every index after the removed one shifted down.
            self.rebuild_node_index();
        }
    }

    fn add_connection(&mut self, conn: Connection) {
        if !self.connections.contains(&conn) {
            self.connections.push(conn);
        }
    }

    fn remove_connection_to(&mut self, to: PortId) {
        self.connections.retain(|c| c.to != to);
    }

    fn apply_command(&mut self, cmd: EngineCommand) {
        match cmd {
            EngineCommand::AddNode { node, shared } => {
                self.add_node(node, shared);
            }
            EngineCommand::RemoveNode(id) => {
                self.remove_node(id);
            }
            EngineCommand::AddConnection(conn) => {
                self.add_connection(conn);
            }
            EngineCommand::RemoveConnectionTo(to) => {
                self.remove_connection_to(to);
            }
            EngineCommand::SetParam {
                node_id,
                index,
                value,
            } => {
                if let Some(node) = self.nodes.iter_mut().find(|n| n.node_id() == node_id) {
                    node.set_param(index, value);
                }
            }
            EngineCommand::NotifyConnect {
                node_id,
                input_port,
                source_type,
            } => {
                if let Some(node) = self.nodes.iter_mut().find(|n| n.node_id() == node_id) {
                    node.on_connect(input_port, source_type);
                }
            }
            EngineCommand::NotifyDisconnect {
                node_id,
                input_port,
            } => {
                if let Some(node) = self.nodes.iter_mut().find(|n| n.node_id() == node_id) {
                    node.on_disconnect(input_port);
                }
            }
            EngineCommand::LoadData { node_id, data } => {
                if let Some(node) = self.nodes.iter_mut().find(|n| n.node_id() == node_id) {
                    node.load_data(&data);
                }
            }
            EngineCommand::RemoveAllNodes => {
                self.nodes.clear();
                self.shared_states.clear();
                self.promoted.clear();
                self.connections.clear();
                self.node_index.clear();
            }
            EngineCommand::SetInterfaces(interfaces) => {
                if let Some(dmx) = &mut self.dmx {
                    dmx.set_interfaces(interfaces);
                }
            }
            EngineCommand::SubgraphInnerCommand { subgraph_path, command } => {
                self.apply_subgraph_inner_cmd(&subgraph_path, *command);
            }
        }
    }

    /// Defensive recursion guard — prevents accidental runaway descent if the
    /// path is malformed. The user is unlikely to nest this deep manually.
    const MAX_SUBGRAPH_NEST_DEPTH: usize = 32;

    fn apply_subgraph_inner_cmd(&mut self, path: &[NodeId], cmd: SubgraphInnerCmd) {
        if path.is_empty() { return; }
        if path.len() > Self::MAX_SUBGRAPH_NEST_DEPTH {
            eprintln!(
                "Subgraph nesting depth {} exceeds limit {}; dropping command",
                path.len(), Self::MAX_SUBGRAPH_NEST_DEPTH,
            );
            return;
        }

        // Find the outermost target subgraph.
        let Some(node) = self.nodes.iter_mut().find(|n| n.node_id() == path[0]) else {
            return;
        };
        let Some(sg) = node.as_any_mut().downcast_mut::<SubgraphProcessNode>() else {
            return;
        };
        sg.apply_inner_cmd_at_path(&path[1..], cmd);
    }

    /// One tick of the engine: process all nodes, propagate signals, update shared state.
    fn tick(&mut self) {
        // 0. Take everything the UI pushed, in a single lock per node. This
        // runs before `process` so an edit lands on the tick it arrives, and
        // before the connection scan below because `load_data` can rebuild a
        // node's ports.
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

        // 1. Compute per-node "which logical input ports are connected"
        //    so nodes can decide whether to honour wired values over their
        //    own params (the "input-overrides-param" pattern).
        self.per_node_connected.resize_with(self.nodes.len(), Vec::new);
        self.per_node_connected.truncate(self.nodes.len());
        for (i, node) in self.nodes.iter().enumerate() {
            let slots = node.inputs().len() + self.promoted[i].len();
            let buf = &mut self.per_node_connected[i];
            buf.clear();
            buf.resize(slots, false);
        }
        for conn in &self.connections {
            if let Some(&dst_idx) = self.node_index.get(&conn.to.node)
                && let Some(slot) = self.per_node_connected[dst_idx].get_mut(conn.to.index) {
                    *slot = true;
                }
        }
        for (i, node) in self.nodes.iter_mut().enumerate() {
            let native = node.inputs().len();
            node.set_input_connections(&self.per_node_connected[i][..native]);
            if self.promoted[i].len() > 0 {
                self.promoted[i].note_wiring(&self.per_node_connected[i][native..]);
            }
        }

        // 2. Process all nodes — skipping disabled ones. Disabled nodes
        // keep their previous tick's output values, so downstream signal
        // flow freezes instead of dropping to zero.
        if let Some(profile) = &mut self.node_profile {
            for (i, node) in self.nodes.iter_mut().enumerate() {
                if self.disabled_mask.get(i).copied().unwrap_or(false) {
                    continue;
                }
                let started = std::time::Instant::now();
                node.process();
                profile.record(node.type_name(), started.elapsed().as_nanos() as u64);
            }
            profile.end_tick();
        } else {
            for (i, node) in self.nodes.iter_mut().enumerate() {
                if !self.disabled_mask.get(i).copied().unwrap_or(false) {
                    node.process();
                }
            }
        }

        // 3. Propagate signals through connections.
        // Multi-channel ports (Color=3, Position=2) expand to multiple float copies.
        let mut writes = std::mem::take(&mut self.write_buf);
        writes.clear();
        let mut promoted_writes = std::mem::take(&mut self.promoted_writes);
        promoted_writes.clear();

        for conn in &self.connections {
            let (Some(&src_node_idx), Some(&dst_node_idx)) = (
                self.node_index.get(&conn.from.node),
                self.node_index.get(&conn.to.node),
            ) else {
                continue;
            };

            let src_ports = self.nodes[src_node_idx].outputs();
            let dst_ports = self.nodes[dst_node_idx].inputs();

            let src_base = port_base_index(src_ports, conn.from.index);

            // Past the node's own inputs sit the promoted params; they take a
            // single value and never go through `write_input`.
            if conn.to.index >= dst_ports.len() {
                let val = self.nodes[src_node_idx].read_output(src_base);
                promoted_writes.push((dst_node_idx, conn.to.index - dst_ports.len(), val));
                continue;
            }

            let dst_base = port_base_index(dst_ports, conn.to.index);

            let src_channels = src_ports.get(conn.from.index).map(|p| p.port_type.channel_count()).unwrap_or(1);
            let dst_channels = dst_ports.get(conn.to.index).map(|p| p.port_type.channel_count()).unwrap_or(1);
            let channels = src_channels.min(dst_channels);

            for ch in 0..channels {
                let val = self.nodes[src_node_idx].read_output(src_base + ch);
                writes.push((dst_node_idx, dst_base + ch, val));
            }
        }

        for &(dst_idx, channel, val) in &writes {
            self.nodes[dst_idx].write_input(channel, val);
        }
        self.write_buf = writes;
        for &(dst_idx, slot, val) in &promoted_writes {
            self.promoted[dst_idx].apply(self.nodes[dst_idx].as_mut(), slot, val);
        }
        self.promoted_writes = promoted_writes;

        // 4. Update shared state for UI, at UI frame rate rather than tick
        // rate. `params()` allocates a fresh Vec of ParamDefs with owned
        // Strings, `save_data()` builds a serde_json tree and
        // `update_display()` clones display buffers — all per node, and all
        // discarded unread 15 times out of 16 when done every tick.
        //
        // For disabled nodes we skip update_display / save_data / shared
        // buffer copies — their values are all frozen so repeating the
        // work accomplishes nothing. `inputs_connected` still
        // refreshes so the UI sees live wire state even on off nodes
        // (so you can still see what's wired where before re-enabling).
        self.sync_counter += 1;
        if self.sync_counter >= UI_SYNC_EVERY_N_TICKS {
            self.sync_counter = 0;
            self.sync_shared_state();
        }

        // 5. Tick the DMX output manager (merge overrides, send to interfaces).
        if let Some(dmx) = &mut self.dmx {
            dmx.tick();
        }
    }

    fn sync_shared_state(&mut self) {
        for (i, node) in self.nodes.iter().enumerate() {
            let mut shared = self.shared_states[i].lock().unwrap();
            shared.inputs_connected.clear();
            shared
                .inputs_connected
                .extend_from_slice(&self.per_node_connected[i]);
            shared.non_promotable = node.non_promotable_params();
            if self.disabled_mask.get(i).copied().unwrap_or(false) {
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
        for (i, node) in self.nodes.iter().enumerate() {
            self.promoted[i].refresh(node.as_ref());
        }
    }

    /// Run the engine loop. Called from the engine thread.
    /// `command_rx` is the consumer end of the ring buffer.
    pub fn run(mut self, mut command_rx: CommandConsumer) {
        let perf = self.perf.clone();
        let mut timer = EngineTickTimer::default();
        loop {
            // Drain all pending commands (non-blocking).
            while let Some(cmd) = command_rx.try_pop() {
                self.apply_command(cmd);
            }

            timer.measure(&perf, || self.tick());

            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
