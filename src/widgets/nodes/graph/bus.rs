//! Buses on the canvas: how wide every node and wire of a level is, refusing
//! the wires that do not fit, and drawing the wide ones.
//!
//! The width of everything is derived from the wires by `engine::lanes::infer`
//! and never saved. It is re-derived whenever the level's wires or ports
//! change, which a hash of both tells cheaply, so a project without buses
//! costs one hash a frame.

use std::hash::{Hash, Hasher};

use super::connections::LevelPorts;
use super::*;
use crate::engine::lanes::{self, LaneNode, Widths};
use crate::engine::types::{EngineCommand, MAX_LANES, ProcessNode};
use crate::widgets::nodes::types::fit_lanes;

/// Makes a fresh engine node of a type, for the extra lanes of a laned node.
/// The app provides it: only the app knows how to construct engine nodes.
pub type EngineFactory = std::rc::Rc<dyn Fn(&str, NodeId) -> Option<Box<dyn ProcessNode>>>;

/// The widths of one level, as of the last time its wires or ports changed.
#[derive(Default)]
pub struct BusState {
    pub widths: Widths,
    /// What `widths` was made from, kept so a wire can be tried out before it
    /// is made (`wire_fits`).
    model: Vec<LaneNode>,
    sig: u64,
    /// Whether `sig` looked at the ports (the open level) or only at the wires.
    deep: bool,
    /// How many lanes the engine was last told each laned node runs at.
    applied: HashMap<NodeId, usize>,
}

impl NodeGraph {
    /// How many lanes the nodes of a type may be run at; unknown types are
    /// unlimited. The app fills this from `ProcessNode::max_lanes`.
    pub fn set_lane_limits(&mut self, limits: HashMap<String, usize>) {
        self.lane_limits = limits;
    }

    /// How the app makes engine nodes, so that a node that gets a bus can be
    /// run as several copies.
    pub fn set_engine_factory(&mut self, factory: EngineFactory) {
        self.engine_factory = Some(factory);
    }

    fn lane_limit(&self, type_name: &str) -> usize {
        // The bridges are not run as copies either: there is one inner graph,
        // and their ports take the width of the wires instead.
        if matches!(type_name, "Graph Input" | "Graph Output") {
            return 1;
        }
        self.lane_limits.get(type_name).copied().unwrap_or(MAX_LANES)
    }

    /// The nodes of a level as `lanes::infer` sees them, with where each gets
    /// its port widths from. The ports that depend on another level (`BusEnd`)
    /// are not read from the widget: `derive_buses` fills them in.
    fn lane_nodes(&self, level: &GraphLevel, ports: Option<&LevelPorts>) -> (Vec<LaneNode>, Vec<BusEnd>) {
        let mut ends = Vec::with_capacity(level.nodes.len());
        let nodes = level
            .nodes
            .iter()
            .zip(&level.states)
            .enumerate()
            .map(|(i, (node, state))| {
                let lanes = |ports: &[UiPortDef]| ports.iter().map(|p| p.def.lanes).collect::<Vec<_>>();
                let (mut inputs, outputs) = match ports {
                    Some(p) => (lanes(&p.inputs[i]), lanes(&p.outputs[i])),
                    None => (lanes(&node.ui_inputs_full()), lanes(&node.ui_outputs())),
                };
                let end = node.bus_end();
                let adapts = matches!(end, BusEnd::Subgraph | BusEnd::GraphOutput | BusEnd::PortalSend);
                if adapts {
                    inputs.iter_mut().for_each(|l| *l = 1);
                }
                ends.push(end);
                LaneNode {
                    id: state.id,
                    max_lanes: self.lane_limit(node.type_name()),
                    follows: node.follows_bus(),
                    adapts,
                    inputs,
                    outputs,
                }
            })
            .collect();
        (nodes, ends)
    }

    fn level_signature(&self, level: &GraphLevel, ports: Option<&LevelPorts>) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for c in &level.connections {
            (c.from.node.0, c.from.index, c.to.node.0, c.to.index).hash(&mut h);
        }
        for (i, (node, state)) in level.nodes.iter().zip(&level.states).enumerate() {
            state.id.0.hash(&mut h);
            if let Some(p) = ports {
                node.type_name().hash(&mut h);
                node.follows_bus().hash(&mut h);
                p.inputs[i].iter().for_each(|port| port.def.lanes.hash(&mut h));
                0xffu8.hash(&mut h);
                p.outputs[i].iter().for_each(|port| port.def.lanes.hash(&mut h));
            }
        }
        h.finish()
    }

    /// The widths of every level, from the wires. A bus can cross a Subgraph
    /// port or a Portal, so one level's widths depend on another's: an input of
    /// a Subgraph is as wide as the wire into it, and that is the width of the
    /// Graph Input bridge's port inside; an output is as wide as the wire into
    /// the Graph Output inside; a Portal Receive is as wide as its Send. So the
    /// levels are worked out again and again, each time with what the others
    /// gave, until nothing moves. It starts from 1 everywhere each time, so a
    /// loop through a portal that has no real source stays at 1. The bound only
    /// matters for loops whose sources contradict each other.
    fn derive_buses(&self, models: &mut [(Vec<LaneNode>, Vec<BusEnd>)]) -> Vec<Widths> {
        let n = self.levels.len();
        let inner_of: HashMap<NodeId, usize> =
            self.levels.iter().enumerate().filter_map(|(li, l)| Some((l.subgraph_id?, li))).collect();
        let find = |li: usize, end: BusEnd| models[li].1.iter().position(|e| *e == end);
        let bridges: Vec<(Option<usize>, Option<usize>)> =
            (0..n).map(|li| (find(li, BusEnd::GraphInput), find(li, BusEnd::GraphOutput))).collect();

        // What the nodes whose outputs are given from elsewhere get, by node index.
        let mut given: Vec<HashMap<usize, Vec<usize>>> = (0..n)
            .map(|li| {
                let (nodes, ends) = &models[li];
                ends.iter()
                    .enumerate()
                    .filter(|(_, e)| matches!(e, BusEnd::Subgraph | BusEnd::GraphInput | BusEnd::PortalReceive))
                    .map(|(k, _)| (k, vec![1; nodes[k].outputs.len()]))
                    .collect()
            })
            .collect();
        let mut widths: Vec<Widths> = vec![Widths::default(); n];
        let mut dirty = vec![true; n];

        for _ in 0..2 * n + 6 {
            if !dirty.contains(&true) {
                break;
            }
            let levels = dirty.iter_mut().zip(models.iter_mut()).zip(&given).zip(widths.iter_mut()).zip(&self.levels);
            for ((((flag, (nodes, _)), given_here), width), level) in levels {
                if !std::mem::take(flag) {
                    continue;
                }
                for (&k, out) in given_here {
                    nodes[k].outputs.clone_from(out);
                }
                *width = lanes::infer(nodes, &level.connections);
            }

            // What each adapting node took, by input port.
            let taken = |li: usize, k: usize| -> Vec<usize> {
                let node = &models[li].0[k];
                (0..node.inputs.len())
                    .map(|index| widths[li].of_adapted(PortId { node: node.id, index, dir: PortDir::Input }))
                    .collect()
            };
            let mut hand = |li: usize, k: usize, wanted: Vec<usize>, dirty: &mut [bool]| {
                let len = models[li].0[k].outputs.len();
                let wanted = fit_lanes(&wanted, len);
                if given[li].get(&k) != Some(&wanted) {
                    given[li].insert(k, wanted);
                    dirty[li] = true;
                }
            };

            let mut sends: HashMap<String, Vec<usize>> = HashMap::new();
            for (li, (nodes, ends)) in models.iter().enumerate() {
                for (k, end) in ends.iter().enumerate() {
                    match end {
                        BusEnd::Subgraph => {
                            let Some(&inner) = inner_of.get(&nodes[k].id) else { continue };
                            if let Some(gi) = bridges[inner].0 {
                                hand(inner, gi, taken(li, k), &mut dirty);
                            }
                            let out = bridges[inner].1.map(|go| taken(inner, go)).unwrap_or_default();
                            hand(li, k, out, &mut dirty);
                        }
                        BusEnd::PortalSend => {
                            if let Some(name) = self.levels[li].nodes[k].link_key() {
                                sends.entry(name).or_insert_with(|| taken(li, k));
                            }
                        }
                        _ => {}
                    }
                }
            }
            for (li, (_, ends)) in models.iter().enumerate() {
                for (k, end) in ends.iter().enumerate() {
                    if *end == BusEnd::PortalReceive {
                        let wanted = self.levels[li].nodes[k].link_key().and_then(|name| sends.get(&name).cloned());
                        hand(li, k, wanted.unwrap_or_default(), &mut dirty);
                    }
                }
            }
        }
        widths
    }

    /// Re-derive the widths of every level when the wires or ports of any of
    /// them changed, and tell the nodes that follow a bus how wide theirs is.
    /// `ports` are the open level's port lists; returns true when a following
    /// node changed its own ports, so the caller's copy is out of date.
    pub(super) fn refresh_buses(&mut self, ports: &LevelPorts) -> bool {
        let sigs: Vec<u64> = (0..self.levels.len())
            .map(|li| self.level_signature(&self.levels[li], (li == self.active_level).then_some(ports)))
            .collect();
        let current = self
            .levels
            .iter()
            .zip(&sigs)
            .enumerate()
            .all(|(li, (level, sig))| *sig == level.bus.sig && (li == self.active_level) == level.bus.deep);
        if current {
            return false;
        }

        let mut models: Vec<(Vec<LaneNode>, Vec<BusEnd>)> = (0..self.levels.len())
            .map(|li| self.lane_nodes(&self.levels[li], (li == self.active_level).then_some(ports)))
            .collect();
        let all_widths = self.derive_buses(&mut models);

        let mut followed = false;
        for (li, ((model, ends), widths)) in models.into_iter().zip(all_widths).enumerate() {
            let open = li == self.active_level;
            let level = &mut self.levels[li];
            for (node, state) in level.nodes.iter_mut().zip(&level.states) {
                if let Some(&w) = widths.follow.get(&state.id) {
                    let before = node.ui_outputs().len();
                    node.follow_bus(w);
                    followed |= open && node.ui_outputs().len() != before;
                }
            }
            for (k, end) in ends.iter().enumerate() {
                if *end == BusEnd::None {
                    continue;
                }
                let id = model[k].id;
                let taken: Vec<usize> = (0..model[k].inputs.len())
                    .map(|index| widths.of_adapted(PortId { node: id, index, dir: PortDir::Input }))
                    .collect();
                level.nodes[k].set_bus_lanes(&taken, &model[k].outputs);
            }
            let mut applied = std::mem::take(&mut level.bus.applied);
            applied.retain(|id, _| level.states.iter().any(|s| s.id == *id));
            let mut changes: Vec<(NodeId, &'static str, usize)> = Vec::new();
            for (node, state) in level.nodes.iter().zip(&level.states) {
                let width = widths.of_node(state.id);
                if width != applied.get(&state.id).copied().unwrap_or(1) {
                    changes.push((state.id, node.type_name(), width));
                    applied.insert(state.id, width);
                    // The lane the node shows must exist.
                    let mut shared = node.shared_state().lock();
                    shared.view_lane = shared.view_lane.min(width - 1);
                }
            }
            level.bus = BusState { widths, model, sig: sigs[li], deep: open, applied };
            for (node_id, type_name, width) in changes {
                self.send_lanes(li, node_id, type_name, width);
            }
        }
        followed
    }

    /// Tell the engine to run a node as `width` lanes, with fresh nodes for
    /// the copies. Without a factory (a graph with no engine behind it) there
    /// is nothing to tell.
    fn send_lanes(&mut self, level: usize, node_id: NodeId, type_name: &str, width: usize) {
        let Some(factory) = self.engine_factory.clone() else { return };
        let extra: Vec<Box<dyn ProcessNode>> = (1..width).filter_map(|_| factory(type_name, node_id)).collect();
        if extra.len() + 1 != width {
            return;
        }
        self.at_level(level, |g| g.push_engine_cmd(EngineCommand::SetLanes { node_id, extra }));
    }

    /// A Bus Split follows the width of the wire on its bus input and drops
    /// the outputs past it, wires and all. Refuse a wire that would make it
    /// narrower than its last wired output; the user removes those first.
    fn would_cut_a_followers_wires(&self, from: PortId, to: PortId) -> bool {
        let level = self.active();
        let bus = &level.bus;
        let Some(dst) = bus.model.iter().find(|n| n.id == to.node && n.follows) else { return false };
        if to.index != 0 {
            return false;
        }
        let mut wires: Vec<Connection> = level.connections.iter().filter(|c| c.to != to).cloned().collect();
        wires.push(Connection { from, to });
        let width = lanes::infer(&bus.model, &wires).follow.get(&dst.id).copied().unwrap_or(1);
        level.connections.iter().any(|c| c.from.node == dst.id && c.from.index >= width)
    }

    /// Whether wire `from` -> `to` fits the widths, tried out on a copy of the
    /// level's wires: it must not be stale itself, and must not make another
    /// wire stale (two wide wires of different widths into one node).
    pub(super) fn wire_fits(&self, from: PortId, to: PortId) -> bool {
        let level = self.active();
        let bus = &level.bus;
        if self.would_cut_a_followers_wires(from, to) {
            return false;
        }
        let narrow_source = bus.model.iter().find(|n| n.id == from.node).is_none_or(|n| {
            n.outputs.get(from.index).copied().unwrap_or(1) == 1 && bus.widths.of_node(n.id) == 1
        });
        let replaces_wide = level.connections.iter().any(|c| c.to == to && bus.widths.of_wire(c.to) > 1);
        let dst_plain = bus.model.iter().find(|n| n.id == to.node).is_none_or(|n| n.inputs.get(to.index).copied().unwrap_or(1) == 1);
        if narrow_source && !replaces_wide && dst_plain {
            return true;
        }
        let mut wires: Vec<Connection> = level.connections.iter().filter(|c| c.to != to).cloned().collect();
        wires.push(Connection { from, to });
        let after = lanes::infer(&bus.model, &wires);
        !after.stale.iter().any(|p| *p == to || !bus.widths.stale.contains(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::new_shared_state;
    use crate::widgets::nodes::math::bus::BusWidget;
    use crate::widgets::nodes::math::constant::ConstantWidget;
    use crate::widgets::nodes::math::math_op::MathWidget;
    use crate::engine::nodes::math::math_op::MathOp;

    fn out(node: u64, index: usize) -> PortId {
        PortId { node: NodeId(node), index, dir: PortDir::Output }
    }

    fn inp(node: u64, index: usize) -> PortId {
        PortId { node: NodeId(node), index, dir: PortDir::Input }
    }

    fn limits() -> HashMap<String, usize> {
        let mut m = HashMap::new();
        for name in ["Bus Merge", "Bus Split", "Bus Pick", "Const Value"] {
            m.insert(name.to_string(), 1);
        }
        m
    }

    /// Four constants (ids 10..14) into a Merge (id 1), an Add (id 2) and a Split (id 3).
    fn graph() -> NodeGraph {
        let mut g = NodeGraph::new();
        g.set_lane_limits(limits());
        g.add_node(Box::new(BusWidget::merge(NodeId(1), new_shared_state(64 * 40, 64 * 40))), Pos2::ZERO);
        g.add_node(Box::new(MathWidget::new(NodeId(2), MathOp::Add, new_shared_state(8, 1))), Pos2::new(200.0, 0.0));
        g.add_node(Box::new(BusWidget::split(NodeId(3), new_shared_state(64 * 40, 64 * 40))), Pos2::new(400.0, 0.0));
        for k in 0..4u64 {
            g.add_node(
                Box::new(ConstantWidget::new(NodeId(10 + k), PortType::Untyped, new_shared_state(0, 1))),
                Pos2::new(0.0, 50.0 * k as f32),
            );
        }
        g
    }

    fn wire(g: &mut NodeGraph, from: PortId, to: PortId) {
        g.add_connection(from, to);
        g.tidy_ports();
    }

    #[test]
    fn a_bus_through_a_node_and_into_a_split_makes_the_split_as_wide_as_the_merge() {
        let mut g = graph();
        g.tidy_ports();
        wire(&mut g, out(1, 0), inp(2, 0));
        wire(&mut g, out(2, 0), inp(3, 0));
        assert_eq!(g.active().bus.widths.of_node(NodeId(2)), 4);
        assert_eq!(g.active().bus.widths.of_wire(inp(3, 0)), 4);
        g.tidy_ports();
        assert_eq!(g.active().nodes[2].ui_outputs().len(), 4, "the split has an output per lane");
    }

    #[test]
    fn a_wide_wire_into_a_node_that_cannot_be_laned_is_refused_and_dropped() {
        let mut g = graph();
        g.tidy_ports();
        // Merge (4 lanes) into another Merge's single input.
        g.add_node(Box::new(BusWidget::merge(NodeId(5), new_shared_state(64 * 40, 64 * 40))), Pos2::ZERO);
        g.tidy_ports();
        assert!(!g.wire_fits(out(1, 0), inp(5, 0)));
        assert!(g.wire_fits(out(10, 0), inp(5, 0)));
        g.add_connection(out(1, 0), inp(5, 0));
        g.tidy_ports();
        assert!(g.active().connections.is_empty(), "cleanup drops what does not fit");
    }

    /// Merge 1 (4 lanes) feeds the Add's first input; a second Merge (id 5) of
    /// `width` lanes is ready to meet it on the second.
    fn two_buses(width: i64) -> NodeGraph {
        let mut g = graph();
        g.add_node(Box::new(BusWidget::merge(NodeId(5), new_shared_state(64 * 40, 64 * 40))), Pos2::ZERO);
        g.tidy_ports();
        g.add_connection(out(1, 0), inp(2, 0));
        g.tidy_ports();
        let merge5 = g.active().nodes.iter().position(|n| n.node_id() == NodeId(5)).unwrap();
        g.node_mut(merge5).restore_from_save(None, &[(1, ParamValue::Int(width))]);
        g.tidy_ports();
        g
    }

    #[test]
    fn a_narrow_wire_into_a_wide_node_is_broadcast() {
        let g = two_buses(4);
        assert!(g.wire_fits(out(10, 0), inp(2, 1)));
    }

    #[test]
    fn two_buses_of_the_same_width_meet_lane_by_lane() {
        let g = two_buses(4);
        assert!(g.wire_fits(out(5, 0), inp(2, 1)));
    }

    #[test]
    fn two_buses_of_different_widths_do_not_fit() {
        let mut g = two_buses(6);
        assert!(!g.wire_fits(out(5, 0), inp(2, 1)));
        g.add_connection(out(5, 0), inp(2, 1));
        g.tidy_ports();
        g.tidy_ports();
        let wires: Vec<_> = g.active().connections.iter().map(|c| (c.from.node.0, c.to.index)).collect();
        assert_eq!(wires, vec![(1, 0)], "the wire that does not fit is dropped, the first one stays");
    }

    #[test]
    fn the_width_collapses_when_the_source_is_removed() {
        let mut g = graph();
        g.tidy_ports();
        wire(&mut g, out(1, 0), inp(2, 0));
        wire(&mut g, out(2, 0), inp(3, 0));
        g.remove_connection_to(inp(2, 0));
        g.tidy_ports();
        assert_eq!(g.active().bus.widths.of_node(NodeId(2)), 1);
        g.tidy_ports();
        // The split keeps its outputs, there is no wire to follow any more.
        assert_eq!(g.active().connections.len(), 1);
    }

    #[test]
    fn a_merge_that_gets_an_input_more_is_still_followed_and_one_that_loses_it_drops_only_what_no_longer_fits() {
        let mut g = graph();
        g.tidy_ports();
        wire(&mut g, out(1, 0), inp(2, 0));
        wire(&mut g, out(2, 0), inp(3, 0));
        g.node_mut(0).restore_from_save(None, &[(1, ParamValue::Int(6))]);
        g.tidy_ports();
        g.tidy_ports();
        assert_eq!(g.active().bus.widths.of_node(NodeId(2)), 6);
        assert_eq!(g.active().nodes[2].ui_outputs().len(), 6);
        assert_eq!(g.active().connections.len(), 2, "a chain that can follow follows");
    }

    #[test]
    fn a_wire_that_would_shrink_a_split_below_its_wired_outputs_is_refused() {
        let mut g = graph();
        g.tidy_ports();
        wire(&mut g, out(1, 0), inp(3, 0));
        assert_eq!(g.active().nodes[2].ui_outputs().len(), 4);
        // Output 0 only: the Split may be narrowed.
        wire(&mut g, out(3, 0), inp(2, 0));
        assert!(g.wire_fits(out(10, 0), inp(3, 0)));
        // A wire on output 2 is in the way.
        wire(&mut g, out(3, 2), inp(2, 1));
        assert!(!g.wire_fits(out(10, 0), inp(3, 0)), "a single wire would drop the wire on output 2");
        assert!(g.wire_fits(out(1, 0), inp(3, 0)), "the same bus again is fine");
    }

    fn lane_commands(g: &mut NodeGraph) -> Vec<(u64, usize)> {
        g.drain_engine_commands()
            .into_iter()
            .filter_map(|c| match c {
                EngineCommand::SetLanes { node_id, extra } => Some((node_id.0, extra.len() + 1)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_engine_is_told_when_a_node_gets_wider_and_when_it_goes_back() {
        let mut g = graph();
        g.set_engine_factory(std::rc::Rc::new(|name, id| {
            (name == "Add").then(|| Box::new(crate::engine::nodes::math::math_op::MathProcessNode::new(id, MathOp::Add)) as Box<dyn ProcessNode>)
        }));
        g.tidy_ports();
        g.drain_engine_commands();
        wire(&mut g, out(1, 0), inp(2, 0));
        assert_eq!(lane_commands(&mut g), vec![(2, 4)]);
        g.tidy_ports();
        assert!(lane_commands(&mut g).is_empty(), "nothing changed, nothing is sent");
        g.node_mut(1).shared_state().lock().view_lane = 3;
        g.remove_connection_to(inp(2, 0));
        g.tidy_ports();
        assert_eq!(lane_commands(&mut g), vec![(2, 1)]);
        assert_eq!(g.active().nodes[1].shared_state().lock().view_lane, 0, "the lane that was shown is gone");
    }
}
