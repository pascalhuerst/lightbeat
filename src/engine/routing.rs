//! Cached signal routing for one graph level.
//!
//! Resolving every connection each tick (two id lookups, two port-offset
//! scans, the connected-input bitmap) cost more than the nodes themselves on
//! the reference project. The result only changes when the graph's structure
//! or a node's port layout does, so it is computed once and reused.
//!
//! `EngineGraph` and `InnerGraph` both run on this. A plan is rebuilt when
//! the owner marks it dirty (structural commands, `set_param`, `load_data`,
//! connect hooks) or when a node's port count no longer matches, which also
//! catches nodes that rebuild their ports inside `process()`.

use std::collections::HashMap;

use super::promoted::PromotedInputs;
use super::types::*;

#[derive(Clone, Copy)]
enum End {
    Node(usize),
    /// The subgraph's bridge: external inputs as a source, outputs as a destination.
    Bridge,
}

struct Route {
    src: End,
    src_base: usize,
    dst: End,
    /// Channel offset on a node or bridge, or the promoted slot when `promoted`.
    dst_base: usize,
    channels: usize,
    promoted: bool,
}

#[derive(Default)]
pub struct RoutePlan {
    routes: Vec<Route>,
    /// Per node: which input ports have a wire, native ports first and then
    /// promoted ones.
    pub connected: Vec<Vec<bool>>,
    /// Input and output port counts each node had when the plan was built.
    port_counts: Vec<(usize, usize)>,
    /// Bridge channel totals, for subgraph levels.
    bridge_channels: (usize, usize),
    writes: Vec<(usize, usize, f32)>,
    promoted_writes: Vec<(usize, usize, f32)>,
    bridge_writes: Vec<(usize, f32)>,
    built: bool,
}

impl RoutePlan {
    /// True while the plan still describes `nodes` and the bridge ports.
    pub fn is_current(
        &self,
        nodes: &[Box<dyn ProcessNode>],
        bridge_in: &[PortDef],
        bridge_out: &[PortDef],
    ) -> bool {
        self.built
            && self.port_counts.len() == nodes.len()
            && self.bridge_channels == (total_channels(bridge_in), total_channels(bridge_out))
            && nodes
                .iter()
                .zip(&self.port_counts)
                .all(|(n, &(i, o))| n.inputs().len() == i && n.outputs().len() == o)
    }

    pub fn build(
        nodes: &[Box<dyn ProcessNode>],
        node_index: &HashMap<NodeId, usize>,
        connections: &[Connection],
        promoted: &[PromotedInputs],
        bridge_in: &[PortDef],
        bridge_out: &[PortDef],
        bridge_in_id: Option<NodeId>,
        bridge_out_id: Option<NodeId>,
    ) -> Self {
        let mut plan = RoutePlan {
            connected: nodes
                .iter()
                .zip(promoted)
                .map(|(n, p)| vec![false; n.inputs().len() + p.len()])
                .collect(),
            port_counts: nodes.iter().map(|n| (n.inputs().len(), n.outputs().len())).collect(),
            bridge_channels: (total_channels(bridge_in), total_channels(bridge_out)),
            built: true,
            ..Default::default()
        };

        for conn in connections {
            let is_bridge_dst = bridge_out_id == Some(conn.to.node);
            if !is_bridge_dst
                && let Some(&dst) = node_index.get(&conn.to.node)
                && let Some(slot) = plan.connected[dst].get_mut(conn.to.index)
            {
                *slot = true;
            }

            let src = if bridge_in_id == Some(conn.from.node) {
                End::Bridge
            } else if let Some(&idx) = node_index.get(&conn.from.node) {
                End::Node(idx)
            } else {
                continue;
            };
            let dst = if is_bridge_dst {
                End::Bridge
            } else if let Some(&idx) = node_index.get(&conn.to.node) {
                End::Node(idx)
            } else {
                continue;
            };

            let src_ports = match src {
                End::Node(i) => nodes[i].outputs(),
                End::Bridge => bridge_in,
            };
            let dst_ports = match dst {
                End::Node(i) => nodes[i].inputs(),
                End::Bridge => bridge_out,
            };
            let src_base = port_base_index(src_ports, conn.from.index);

            // Past a node's own inputs sit its promoted params, which take a
            // single value and never go through `write_input`.
            if let End::Node(_) = dst
                && conn.to.index >= dst_ports.len()
            {
                plan.routes.push(Route {
                    src,
                    src_base,
                    dst,
                    dst_base: conn.to.index - dst_ports.len(),
                    channels: 1,
                    promoted: true,
                });
                continue;
            }

            let src_channels = src_ports
                .get(conn.from.index)
                .map(|p| p.port_type.channel_count())
                .unwrap_or(1);
            let dst_channels = dst_ports
                .get(conn.to.index)
                .map(|p| p.port_type.channel_count())
                .unwrap_or(1);
            plan.routes.push(Route {
                src,
                src_base,
                dst,
                dst_base: port_base_index(dst_ports, conn.to.index),
                channels: src_channels.min(dst_channels),
                promoted: false,
            });
        }
        plan
    }

    /// Move every value along its wire. All reads happen before any write, so
    /// a signal crosses one node per tick regardless of node order.
    pub fn run(
        &mut self,
        nodes: &mut [Box<dyn ProcessNode>],
        promoted: &mut [PromotedInputs],
        bridge_in: &[f32],
        bridge_out: &mut [f32],
    ) {
        self.writes.clear();
        self.promoted_writes.clear();
        self.bridge_writes.clear();

        for r in &self.routes {
            for ch in 0..r.channels {
                let val = match r.src {
                    End::Node(i) => nodes[i].read_output(r.src_base + ch),
                    End::Bridge => bridge_in.get(r.src_base + ch).copied().unwrap_or(0.0),
                };
                match (r.dst, r.promoted) {
                    (End::Node(i), true) => self.promoted_writes.push((i, r.dst_base, val)),
                    (End::Node(i), false) => self.writes.push((i, r.dst_base + ch, val)),
                    (End::Bridge, _) => self.bridge_writes.push((r.dst_base + ch, val)),
                }
            }
        }

        for &(i, ch, val) in &self.writes {
            nodes[i].write_input(ch, val);
        }
        for &(ch, val) in &self.bridge_writes {
            if let Some(slot) = bridge_out.get_mut(ch) {
                *slot = val;
            }
        }
        for &(i, slot, val) in &self.promoted_writes {
            promoted[i].apply(nodes[i].as_mut(), slot, val);
        }
    }

    pub fn native_connected(&self, node: usize, native_inputs: usize) -> &[bool] {
        &self.connected[node][..native_inputs]
    }

    pub fn promoted_connected(&self, node: usize, native_inputs: usize) -> &[bool] {
        &self.connected[node][native_inputs..]
    }
}
