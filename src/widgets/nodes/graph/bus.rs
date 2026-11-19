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
        // The bridges inside a subgraph carry no buses (yet).
        if matches!(type_name, "Graph Input" | "Graph Output") {
            return 1;
        }
        self.lane_limits.get(type_name).copied().unwrap_or(MAX_LANES)
    }

    fn lane_nodes(&self, level: &GraphLevel, ports: Option<&LevelPorts>) -> Vec<LaneNode> {
        level
            .nodes
            .iter()
            .zip(&level.states)
            .enumerate()
            .map(|(i, (node, state))| {
                let lanes = |ports: &[UiPortDef]| ports.iter().map(|p| p.def.lanes).collect::<Vec<_>>();
                let (inputs, outputs) = match ports {
                    Some(p) => (lanes(&p.inputs[i]), lanes(&p.outputs[i])),
                    None => (lanes(&node.ui_inputs_full()), lanes(&node.ui_outputs())),
                };
                LaneNode {
                    id: state.id,
                    max_lanes: self.lane_limit(node.type_name()),
                    follows: node.follows_bus(),
                    inputs,
                    outputs,
                }
            })
            .collect()
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

    /// Re-derive the widths of every level whose wires or ports changed, and
    /// tell the nodes that follow a bus how wide theirs is. `ports` are the
    /// open level's port lists; returns true when a following node changed its
    /// own ports, so the caller's copy is out of date.
    pub(super) fn refresh_buses(&mut self, ports: &LevelPorts) -> bool {
        let mut followed = false;
        for li in 0..self.levels.len() {
            let open = li == self.active_level;
            let level = &self.levels[li];
            let sig = self.level_signature(level, open.then_some(ports));
            if sig == level.bus.sig && open == level.bus.deep {
                continue;
            }
            let model = self.lane_nodes(level, open.then_some(ports));
            let widths = lanes::infer(&model, &level.connections);
            let level = &mut self.levels[li];
            for (node, state) in level.nodes.iter_mut().zip(&level.states) {
                if let Some(&w) = widths.follow.get(&state.id) {
                    let before = node.ui_outputs().len();
                    node.follow_bus(w);
                    followed |= open && node.ui_outputs().len() != before;
                }
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
            level.bus = BusState { widths, model, sig, deep: open, applied };
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

    /// Whether wire `from` -> `to` fits the widths, tried out on a copy of the
    /// level's wires: it must not be stale itself, and must not make another
    /// wire stale (two wide wires of different widths into one node).
    pub(super) fn wire_fits(&self, from: PortId, to: PortId) -> bool {
        let level = self.active();
        let bus = &level.bus;
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
