//! Cached signal routing for one graph level.
//!
//! Resolving every connection each tick (two id lookups, two port-offset
//! scans, the connected-input bitmap) cost more than the nodes themselves on
//! the reference project. The result only changes when the graph's structure
//! or a node's port layout does, so it is computed once and reused.
//!
//! `EngineGraph` and `InnerGraph` both run on this. A plan is rebuilt when
//! the owner marks it dirty (structural commands, `set_param`, `load_data`,
//! connect hooks, lane counts) or when a node's port count no longer matches,
//! which also catches nodes that rebuild their ports inside `process()`.
//!
//! Buses are resolved here too, once per rebuild: a wire between a bus port or
//! a laned node on either end becomes one flat route per lane (or one route for
//! a bus port straight into another), so a tick never asks how wide anything is.
//! A narrow source feeding a laned destination is copied to each lane.

use std::collections::HashMap;

use super::lanes::Lane;
use super::promoted::PromotedInputs;
use super::types::*;

#[derive(Clone, Copy)]
enum End {
    Node(usize),
    /// The subgraph's bridge: external inputs as a source, outputs as a destination.
    Bridge,
}

/// How one end of a wire is spread over lanes.
#[derive(Clone, Copy, PartialEq)]
enum Spread {
    /// One value: a plain port of a node that runs once, or a bridge.
    Single,
    /// A plain port of a node run as this many copies: lane `k` is copy `k`.
    Copies(usize),
    /// A bus port of this many lanes in one node's flat buffer.
    Bus(usize),
}

/// One end of a wire, resolved.
#[derive(Clone, Copy)]
struct Side {
    end: End,
    spread: Spread,
    /// Channel offset of the port (of its first lane for a bus), or the
    /// promoted slot.
    base: usize,
    /// Channels of one lane of the port's type.
    unit: usize,
}

struct Route {
    src: End,
    /// Which copy of the source node is read (0 is the node itself).
    src_lane: u32,
    src_base: usize,
    dst: End,
    dst_lane: u32,
    /// Channel offset on a node or bridge, or the promoted slot when `promoted`.
    dst_base: usize,
    channels: usize,
    promoted: bool,
    /// A color feeding a gradient input: read three channels, write a
    /// solid gradient.
    solid_gradient: bool,
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
    /// Nodes that reshape their own ports while processing (`reshapes_itself`)
    /// with their (input, output) channel totals; usually none.
    watched: Vec<(usize, usize, usize)>,
    /// (node, lane, channel, value); kept small, this is the hot list.
    writes: Vec<(u32, u32, u32, f32)>,
    /// (node, lane, promoted slot, value)
    promoted_writes: Vec<(u32, u32, u32, f32)>,
    bridge_writes: Vec<(usize, f32)>,
    built: bool,
}

/// Channel `ch` of a gradient made of one color: opaque stops at both ends,
/// the rest unused (alpha below zero).
fn solid_gradient_channel(color: [f32; 3], ch: usize) -> f32 {
    let (stop, field) = (ch / GRADIENT_STOP_FLOATS, ch % GRADIENT_STOP_FLOATS);
    match (stop, field) {
        (0 | 1, 0..=2) => color[field],
        (0 | 1, 3) => 1.0,
        (0, 4) => 0.0,
        (1, 4) => 1.0,
        (_, 3) => -1.0,
        _ => 0.0,
    }
}

fn read_lane(nodes: &[Box<dyn ProcessNode>], lanes: &[Vec<Lane>], i: usize, lane: u32, ch: usize) -> f32 {
    if lane == 0 {
        nodes[i].read_output(ch)
    } else {
        lanes[i].get(lane as usize - 1).map_or(0.0, |l| l.node.read_output(ch))
    }
}

fn lane_count(lanes: &[Vec<Lane>], i: usize) -> usize {
    lanes.get(i).map_or(0, |l| l.len()) + 1
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
            && self
                .watched
                .iter()
                .all(|&(n, i, o)| total_channels(nodes[n].inputs()) == i && total_channels(nodes[n].outputs()) == o)
    }

    pub fn build(
        nodes: &[Box<dyn ProcessNode>],
        lanes: &[Vec<Lane>],
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
            watched: nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.reshapes_itself())
                .map(|(i, n)| (i, total_channels(n.inputs()), total_channels(n.outputs())))
                .collect(),
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
            let src_def = src_ports.get(conn.from.index);
            let src_lanes = src_def.map_or(1, |p| p.lanes);
            let src_unit = src_def.map_or(1, |p| p.port_type.channel_count());
            let src_spread = match src {
                _ if src_lanes > 1 => Spread::Bus(src_lanes),
                End::Node(i) if lane_count(lanes, i) > 1 => Spread::Copies(lane_count(lanes, i)),
                _ => Spread::Single,
            };
            let src_side = Side { end: src, spread: src_spread, base: src_base, unit: src_unit };

            // Past a node's own inputs sit its promoted params, which take a
            // single value and never go through `write_input`.
            if let End::Node(d) = dst
                && conn.to.index >= dst_ports.len()
            {
                let dst_spread = match lane_count(lanes, d) {
                    1 => Spread::Single,
                    n => Spread::Copies(n),
                };
                let dst_side = Side { end: dst, spread: dst_spread, base: conn.to.index - dst_ports.len(), unit: 1 };
                plan.push_lanes(src_side, dst_side, 1, true, false);
                continue;
            }

            let dst_def = dst_ports.get(conn.to.index);
            let dst_lanes = dst_def.map_or(1, |p| p.lanes);
            let dst_unit = dst_def.map_or(1, |p| p.port_type.channel_count());
            let dst_spread = match dst {
                _ if dst_lanes > 1 => Spread::Bus(dst_lanes),
                End::Node(i) if lane_count(lanes, i) > 1 => Spread::Copies(lane_count(lanes, i)),
                _ => Spread::Single,
            };
            let solid_gradient = src_def.map(|p| p.port_type) == Some(PortType::Color)
                && dst_def.map(|p| p.port_type) == Some(PortType::Gradient);
            let per_lane = if solid_gradient { dst_unit } else { src_unit.min(dst_unit) };
            let dst_side = Side { end: dst, spread: dst_spread, base: port_base_index(dst_ports, conn.to.index), unit: dst_unit };
            plan.push_lanes(src_side, dst_side, per_lane, false, solid_gradient);
        }
        plan
    }

    /// The routes of one wire: one per lane it carries, or a single flat copy
    /// when both ends are bus ports (their lanes sit next to each other in both
    /// buffers). Ends that do not fit each other (a bus into a node that runs
    /// once) copy what they have in common; the UI refuses such wires, this
    /// only keeps a stale one from reading out of range.
    fn push_lanes(&mut self, src: Side, dst: Side, per_lane: usize, promoted: bool, solid_gradient: bool) {
        let route = |src_lane: usize, src_base, dst_lane: usize, dst_base, channels| Route {
            src: src.end, src_lane: src_lane as u32, src_base, dst: dst.end, dst_lane: dst_lane as u32, dst_base, channels, promoted, solid_gradient,
        };
        let lanes_of = |s: Spread| match s {
            Spread::Copies(n) | Spread::Bus(n) => n,
            Spread::Single => 1,
        };
        match (src.spread, dst.spread) {
            // Lanes sit next to each other in both buffers, so one copy does it.
            (Spread::Bus(s), Spread::Bus(d)) if src.unit == dst.unit && !solid_gradient => {
                self.routes.push(route(0, src.base, 0, dst.base, per_lane * s.min(d)));
            }
            (Spread::Single, Spread::Copies(d)) => {
                for k in 0..d {
                    self.routes.push(route(0, src.base, k, dst.base, per_lane));
                }
            }
            (Spread::Single, _) | (_, Spread::Single) => {
                self.routes.push(route(0, src.base, 0, dst.base, per_lane));
            }
            (s, d) => {
                for k in 0..lanes_of(s).min(lanes_of(d)) {
                    let (src_lane, src_base) = match s {
                        Spread::Bus(_) => (0, src.base + k * src.unit),
                        _ => (k, src.base),
                    };
                    let (dst_lane, dst_base) = match d {
                        Spread::Bus(_) => (0, dst.base + k * dst.unit),
                        _ => (k, dst.base),
                    };
                    self.routes.push(route(src_lane, src_base, dst_lane, dst_base, per_lane));
                }
            }
        }
    }

    /// Move every value along its wire. All reads happen before any write, so
    /// a signal crosses one node per tick regardless of node order.
    pub fn run(
        &mut self,
        nodes: &mut [Box<dyn ProcessNode>],
        lanes: &mut [Vec<Lane>],
        promoted: &mut [PromotedInputs],
        bridge_in: &[f32],
        bridge_out: &mut [f32],
    ) {
        self.writes.clear();
        self.promoted_writes.clear();
        self.bridge_writes.clear();

        for r in &self.routes {
            let read = |ch: usize| match r.src {
                End::Node(i) => read_lane(nodes, lanes, i, r.src_lane, r.src_base + ch),
                End::Bridge => bridge_in.get(r.src_base + ch).copied().unwrap_or(0.0),
            };
            let color = if r.solid_gradient { [read(0), read(1), read(2)] } else { [0.0; 3] };
            for ch in 0..r.channels {
                let val = if r.solid_gradient { solid_gradient_channel(color, ch) } else { read(ch) };
                match (r.dst, r.promoted) {
                    (End::Node(i), true) => self.promoted_writes.push((i as u32, r.dst_lane, r.dst_base as u32, val)),
                    (End::Node(i), false) => self.writes.push((i as u32, r.dst_lane, (r.dst_base + ch) as u32, val)),
                    (End::Bridge, _) => self.bridge_writes.push((r.dst_base + ch, val)),
                }
            }
        }

        for &(i, lane, ch, val) in &self.writes {
            let (i, ch) = (i as usize, ch as usize);
            if lane == 0 {
                nodes[i].write_input(ch, val);
            } else if let Some(l) = lanes[i].get_mut(lane as usize - 1) {
                l.node.write_input(ch, val);
            }
        }
        for &(ch, val) in &self.bridge_writes {
            if let Some(slot) = bridge_out.get_mut(ch) {
                *slot = val;
            }
        }
        for &(i, lane, slot, val) in &self.promoted_writes {
            let (i, slot) = (i as usize, slot as usize);
            if lane == 0 {
                promoted[i].apply(nodes[i].as_mut(), slot, val);
            } else if let Some(l) = lanes[i].get_mut(lane as usize - 1) {
                l.promoted.apply(l.node.as_mut(), slot, val);
            }
        }
    }

    pub fn native_connected(&self, node: usize, native_inputs: usize) -> &[bool] {
        &self.connected[node][..native_inputs]
    }

    pub fn promoted_connected(&self, node: usize, native_inputs: usize) -> &[bool] {
        &self.connected[node][native_inputs..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_color_becomes_a_gradient_of_that_one_color() {
        let c = [0.1, 0.2, 0.3];
        let g: Vec<f32> = (0..GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS).map(|i| solid_gradient_channel(c, i)).collect();
        let gradient = crate::color::Gradient::from_channels(&g);
        for pos in [0.0, 0.5, 1.0] {
            let s = gradient.sample_with_alpha(pos).0;
            assert!((s.r - 0.1).abs() < 1e-4 && (s.g - 0.2).abs() < 1e-4 && (s.b - 0.3).abs() < 1e-4);
        }
    }
}
