use std::time::Duration;

use ringbuf::traits::Consumer;

use super::CommandConsumer;
use super::types::*;
use crate::dmx_io::{DmxOutputManager, SharedDmxState, SharedFixtureStore};

/// The engine-side signal graph. Runs on its own thread at ~1kHz.
pub struct EngineGraph {
    nodes: Vec<Box<dyn ProcessNode>>,
    shared_states: Vec<SharedState>,
    connections: Vec<Connection>,
    dmx: Option<DmxOutputManager>,
}

impl EngineGraph {
    pub fn new(dmx_shared: SharedDmxState, fixture_store: SharedFixtureStore) -> Self {
        Self {
            nodes: Vec::new(),
            shared_states: Vec::new(),
            connections: Vec::new(),
            dmx: Some(DmxOutputManager::new(dmx_shared, fixture_store)),
        }
    }

    fn add_node(&mut self, node: Box<dyn ProcessNode>, shared: SharedState) {
        self.shared_states.push(shared);
        self.nodes.push(node);
    }

    fn remove_node(&mut self, id: NodeId) {
        if let Some(idx) = self.nodes.iter().position(|n| n.node_id() == id) {
            self.nodes.remove(idx);
            self.shared_states.remove(idx);
            self.connections
                .retain(|c| c.from.node != id && c.to.node != id);
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
                self.connections.clear();
            }
            EngineCommand::SetInterfaces(interfaces) => {
                if let Some(dmx) = &mut self.dmx {
                    dmx.set_interfaces(interfaces);
                }
            }
        }
    }

    /// One tick of the engine: process all nodes, propagate signals, update shared state.
    fn tick(&mut self) {
        // 1. Process all nodes.
        for node in self.nodes.iter_mut() {
            node.process();
        }

        // 2. Propagate signals through connections.
        // Multi-channel ports (Color=3, Position=2) expand to multiple float copies.
        let mut writes: Vec<(usize, usize, f32)> = Vec::new();

        for conn in &self.connections {
            let src_node_idx = match self.nodes.iter().position(|n| n.node_id() == conn.from.node) {
                Some(i) => i,
                None => continue,
            };
            let dst_node_idx = match self.nodes.iter().position(|n| n.node_id() == conn.to.node) {
                Some(i) => i,
                None => continue,
            };

            let src_ports = self.nodes[src_node_idx].outputs();
            let dst_ports = self.nodes[dst_node_idx].inputs();

            let src_base = port_base_index(src_ports, conn.from.index);
            let dst_base = port_base_index(dst_ports, conn.to.index);

            let src_channels = src_ports.get(conn.from.index).map(|p| p.port_type.channel_count()).unwrap_or(1);
            let dst_channels = dst_ports.get(conn.to.index).map(|p| p.port_type.channel_count()).unwrap_or(1);
            let channels = src_channels.min(dst_channels);

            for ch in 0..channels {
                let val = self.nodes[src_node_idx].read_output(src_base + ch);
                writes.push((dst_node_idx, dst_base + ch, val));
            }
        }

        for (dst_idx, channel, val) in writes {
            self.nodes[dst_idx].write_input(channel, val);
        }

        // 3. Drain pending param changes from UI (via shared state).
        for (i, node) in self.nodes.iter_mut().enumerate() {
            let pending: Vec<(usize, ParamValue)> = {
                let mut shared = self.shared_states[i].lock().unwrap();
                std::mem::take(&mut shared.pending_params)
            };
            for (idx, val) in pending {
                node.set_param(idx, val);
            }
        }

        // 4. Update shared state for UI.
        for (i, node) in self.nodes.iter().enumerate() {
            let mut shared = self.shared_states[i].lock().unwrap();
            for pi in 0..node.outputs().len().min(shared.outputs.len()) {
                shared.outputs[pi] = node.read_output(pi);
            }
            for pi in 0..node.inputs().len().min(shared.inputs.len()) {
                shared.inputs[pi] = node.read_input(pi);
            }
            shared.current_params = node.params();
            shared.save_data = node.save_data();
            node.update_display(&mut shared);
        }

        // 5. Tick the DMX output manager (merge overrides, send to interfaces).
        if let Some(dmx) = &mut self.dmx {
            dmx.tick();
        }
    }

    /// Run the engine loop. Called from the engine thread.
    /// `command_rx` is the consumer end of the ring buffer.
    pub fn run(mut self, mut command_rx: CommandConsumer) {
        loop {
            // Drain all pending commands (non-blocking).
            while let Some(cmd) = command_rx.try_pop() {
                self.apply_command(cmd);
            }

            self.tick();

            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
