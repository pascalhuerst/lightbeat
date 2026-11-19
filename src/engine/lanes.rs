//! How wide each node and wire of one graph level is.
//!
//! A wire is as wide as its source: a bus port is as wide as its `lanes`, a
//! plain port of a laned node as wide as the node, anything else is 1. A node
//! that can be laned (`max_lanes() > 1`) takes its width from its wires: the
//! width of the wide wire on its lowest-numbered input. Narrow wires into it
//! are broadcast to every lane; a second wide wire of another width does not
//! fit and is reported as stale, like a wire of the wrong type.
//!
//! This is a pure function of the level's ports and wires. The UI owns the
//! structure, so it runs this whenever the wires or ports change, and uses the
//! answer to draw, to refuse wires, and to tell the engine how many copies of
//! each node to run. Widths are never saved: loading a project derives them
//! again from the wires.

use std::collections::HashMap;

use super::promoted::PromotedInputs;
use super::types::{Connection, MAX_LANES, NodeId, ParamDef, ParamValue, PortId, ProcessNode};

/// One extra copy of a laned node: lane 0 is the node itself, lanes 1.. are
/// these. A copy has its own promoted-param state, so a wide wire on a setting
/// moves each copy's setting on its own.
pub struct Lane {
    pub node: Box<dyn ProcessNode>,
    pub promoted: PromotedInputs,
}

/// The settings that differ between two `params()` listings of one node, as
/// `set_param` calls. A snapshot recall or fade changes lane 0's settings
/// through the node's own `set_param`; this carries the change to the copies.
pub fn changed_params(before: &[ParamDef], after: &[ParamDef]) -> Vec<(usize, ParamValue)> {
    fn same(a: &ParamValue, b: &ParamValue) -> bool {
        match (a, b) {
            (ParamValue::Float(a), ParamValue::Float(b)) => a == b,
            (ParamValue::Int(a), ParamValue::Int(b)) => a == b,
            (ParamValue::Bool(a), ParamValue::Bool(b)) => a == b,
            (ParamValue::Choice(a), ParamValue::Choice(b)) => a == b,
            _ => false,
        }
    }
    after
        .iter()
        .enumerate()
        .filter(|(i, p)| before.get(*i).is_none_or(|old| !same(&old.as_value(), &p.as_value())))
        .map(|(i, p)| (i, p.as_value()))
        .collect()
}

/// What the inference needs to know about one node.
#[derive(Debug, Clone)]
pub struct LaneNode {
    pub id: NodeId,
    /// `ProcessNode::max_lanes`: above 1 the node can be run as copies.
    pub max_lanes: usize,
    /// A node whose bus input (port 0) takes any width and that reshapes its
    /// own ports to match (Bus Split, Bus Pick).
    pub follows: bool,
    /// Lanes of each input port, promoted ports (past the node's own) included.
    pub inputs: Vec<usize>,
    pub outputs: Vec<usize>,
}

impl LaneNode {
    pub fn laneable(&self) -> bool {
        self.max_lanes > 1 && !self.follows && self.inputs.iter().chain(&self.outputs).all(|&l| l == 1)
    }

    fn input_lanes(&self, index: usize) -> usize {
        self.inputs.get(index).copied().unwrap_or(1)
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Widths {
    /// Width of every laned node; absent means 1.
    pub node: HashMap<NodeId, usize>,
    /// Width of every wire wider than 1, by its destination port.
    pub wire: HashMap<PortId, usize>,
    /// What each following node's bus input is fed with.
    pub follow: HashMap<NodeId, usize>,
    /// Destination ports of the wires that do not fit and are to be dropped.
    pub stale: Vec<PortId>,
}

impl Widths {
    pub fn of_node(&self, id: NodeId) -> usize {
        self.node.get(&id).copied().unwrap_or(1)
    }

    pub fn of_wire(&self, to: PortId) -> usize {
        self.wire.get(&to).copied().unwrap_or(1)
    }
}

fn source_width(src: &LaneNode, port: usize, widths: &HashMap<NodeId, usize>) -> usize {
    match src.outputs.get(port) {
        Some(&l) if l > 1 => l,
        _ if src.laneable() => widths.get(&src.id).copied().unwrap_or(1),
        _ => 1,
    }
}

pub fn infer(nodes: &[LaneNode], wires: &[Connection]) -> Widths {
    let by_id: HashMap<NodeId, &LaneNode> = nodes.iter().map(|n| (n.id, n)).collect();

    // Wide wires into each laneable node, by input port, lowest first.
    let mut into: HashMap<NodeId, Vec<&Connection>> = HashMap::new();
    for c in wires {
        if by_id.get(&c.to.node).is_some_and(|n| n.laneable()) && by_id.contains_key(&c.from.node) {
            into.entry(c.to.node).or_default().push(c);
        }
    }
    for list in into.values_mut() {
        list.sort_by_key(|c| c.to.index);
    }

    // Start everything at 1 and recompute from the wires until nothing moves.
    // A loop with no source in it never leaves 1; the bound only matters for
    // loops with contradicting sources, which end up with stale wires.
    let mut width: HashMap<NodeId, usize> = HashMap::new();
    for _ in 0..nodes.len() + 2 {
        let mut next: HashMap<NodeId, usize> = HashMap::new();
        for (id, list) in &into {
            let max = by_id[id].max_lanes.min(MAX_LANES);
            let wide = list.iter().find_map(|c| {
                let w = source_width(by_id[&c.from.node], c.from.index, &width);
                (w > 1 && w <= max).then_some(w)
            });
            if let Some(w) = wide {
                next.insert(*id, w);
            }
        }
        if next == width {
            break;
        }
        width = next;
    }

    let mut out = Widths { node: width.clone(), ..Widths::default() };
    for c in wires {
        let (Some(src), Some(dst)) = (by_id.get(&c.from.node), by_id.get(&c.to.node)) else { continue };
        let ws = source_width(src, c.from.index, &width);
        if ws > 1 {
            out.wire.insert(c.to, ws);
        }
        if dst.follows && c.to.index == 0 {
            out.follow.insert(dst.id, ws);
            continue;
        }
        let lanes = dst.input_lanes(c.to.index);
        let fits = if lanes > 1 {
            ws == lanes
        } else {
            ws == 1 || (dst.laneable() && width.get(&dst.id) == Some(&ws))
        };
        if !fits {
            out.stale.push(c.to);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::PortDir;

    fn out(id: u64, lanes: usize) -> LaneNode {
        LaneNode { id: NodeId(id), max_lanes: 1, follows: false, inputs: vec![], outputs: vec![lanes] }
    }

    /// A node that can be laned, with `inputs` plain inputs.
    fn laned(id: u64, inputs: usize) -> LaneNode {
        LaneNode { id: NodeId(id), max_lanes: MAX_LANES, follows: false, inputs: vec![1; inputs], outputs: vec![1] }
    }

    fn plain(id: u64, inputs: usize) -> LaneNode {
        LaneNode { max_lanes: 1, ..laned(id, inputs) }
    }

    fn split(id: u64) -> LaneNode {
        LaneNode { id: NodeId(id), max_lanes: 1, follows: true, inputs: vec![1], outputs: vec![1; 4] }
    }

    fn wire(from: u64, to: u64, input: usize) -> Connection {
        Connection {
            from: PortId { node: NodeId(from), index: 0, dir: PortDir::Output },
            to: PortId { node: NodeId(to), index: input, dir: PortDir::Input },
        }
    }

    fn to(node: u64, input: usize) -> PortId {
        PortId { node: NodeId(node), index: input, dir: PortDir::Input }
    }

    #[test]
    fn a_bus_into_a_node_makes_the_node_as_wide() {
        let w = infer(&[out(1, 16), laned(2, 1)], &[wire(1, 2, 0)]);
        assert_eq!(w.of_node(NodeId(2)), 16);
        assert_eq!(w.of_wire(to(2, 0)), 16);
        assert!(w.stale.is_empty());
    }

    #[test]
    fn width_flows_down_a_chain() {
        let nodes = [out(1, 8), laned(2, 1), laned(3, 1), laned(4, 1)];
        let w = infer(&nodes, &[wire(1, 2, 0), wire(2, 3, 0), wire(3, 4, 0)]);
        assert_eq!([2, 3, 4].map(|n| w.of_node(NodeId(n))), [8, 8, 8]);
        assert_eq!(w.of_wire(to(4, 0)), 8);
    }

    #[test]
    fn a_single_wire_into_a_wide_node_is_broadcast() {
        let nodes = [out(1, 16), out(2, 1), laned(3, 2)];
        let w = infer(&nodes, &[wire(1, 3, 0), wire(2, 3, 1)]);
        assert_eq!(w.of_node(NodeId(3)), 16);
        assert_eq!(w.of_wire(to(3, 1)), 1);
        assert!(w.stale.is_empty());
    }

    #[test]
    fn two_wide_wires_of_different_widths_keep_the_lower_input_and_drop_the_other() {
        let nodes = [out(1, 16), out(2, 8), laned(3, 2)];
        let w = infer(&nodes, &[wire(2, 3, 1), wire(1, 3, 0)]);
        assert_eq!(w.of_node(NodeId(3)), 16);
        assert_eq!(w.stale, vec![to(3, 1)]);
    }

    #[test]
    fn two_wide_wires_of_the_same_width_go_lane_by_lane() {
        let nodes = [out(1, 16), out(2, 16), laned(3, 2)];
        let w = infer(&nodes, &[wire(1, 3, 0), wire(2, 3, 1)]);
        assert!(w.stale.is_empty());
    }

    #[test]
    fn a_bus_into_a_node_that_cannot_be_laned_is_stale() {
        let w = infer(&[out(1, 4), plain(2, 1)], &[wire(1, 2, 0)]);
        assert_eq!(w.stale, vec![to(2, 0)]);
        assert_eq!(w.of_node(NodeId(2)), 1);
    }

    #[test]
    fn a_single_wire_into_a_bus_port_does_not_fit() {
        let sink = LaneNode { id: NodeId(2), max_lanes: 1, follows: false, inputs: vec![4], outputs: vec![] };
        assert_eq!(infer(&[out(1, 1), sink.clone()], &[wire(1, 2, 0)]).stale, vec![to(2, 0)]);
        assert!(infer(&[out(1, 4), sink.clone()], &[wire(1, 2, 0)]).stale.is_empty());
        assert_eq!(infer(&[out(1, 5), sink], &[wire(1, 2, 0)]).stale, vec![to(2, 0)]);
    }

    #[test]
    fn a_split_takes_any_width_and_reports_it() {
        let w = infer(&[out(1, 12), split(2)], &[wire(1, 2, 0)]);
        assert_eq!(w.follow.get(&NodeId(2)), Some(&12));
        assert!(w.stale.is_empty());
        let w = infer(&[out(1, 1), split(2)], &[wire(1, 2, 0)]);
        assert_eq!(w.follow.get(&NodeId(2)), Some(&1));
    }

    #[test]
    fn a_split_behind_a_laned_chain_follows_the_chain() {
        let w = infer(&[out(1, 6), laned(2, 1), split(3)], &[wire(1, 2, 0), wire(2, 3, 0)]);
        assert_eq!(w.follow.get(&NodeId(3)), Some(&6));
    }

    #[test]
    fn the_width_shrinks_when_the_source_goes() {
        let nodes = [out(1, 16), laned(2, 1), laned(3, 1)];
        let before = infer(&nodes, &[wire(1, 2, 0), wire(2, 3, 0)]);
        assert_eq!(before.of_node(NodeId(3)), 16);
        let after = infer(&nodes, &[wire(2, 3, 0)]);
        assert_eq!(after.of_node(NodeId(2)), 1);
        assert_eq!(after.of_node(NodeId(3)), 1);
        assert!(after.stale.is_empty());
    }

    #[test]
    fn a_source_that_gets_narrower_pulls_the_chain_with_it() {
        let wires = [wire(1, 2, 0), wire(2, 3, 0)];
        let wide = infer(&[out(1, 16), laned(2, 1), laned(3, 1)], &wires);
        let narrow = infer(&[out(1, 4), laned(2, 1), laned(3, 1)], &wires);
        assert_eq!((wide.of_node(NodeId(3)), narrow.of_node(NodeId(3))), (16, 4));
        assert!(narrow.stale.is_empty());
    }

    #[test]
    fn a_loop_with_no_source_stays_one_wide() {
        let w = infer(&[laned(1, 1), laned(2, 1)], &[wire(1, 2, 0), wire(2, 1, 0)]);
        assert_eq!((w.of_node(NodeId(1)), w.of_node(NodeId(2))), (1, 1));
        assert!(w.stale.is_empty());
    }

    #[test]
    fn a_loop_fed_by_a_source_takes_its_width() {
        let nodes = [out(9, 8), laned(1, 2), laned(2, 1)];
        let w = infer(&nodes, &[wire(9, 1, 0), wire(1, 2, 0), wire(2, 1, 1)]);
        assert_eq!((w.of_node(NodeId(1)), w.of_node(NodeId(2))), (8, 8));
        assert!(w.stale.is_empty());
    }

    #[test]
    fn a_loop_that_contradicts_its_source_ends_with_a_stale_wire_and_terminates() {
        let nodes = [out(8, 8), out(9, 4), laned(1, 2), laned(2, 2)];
        let w = infer(&nodes, &[wire(8, 1, 0), wire(1, 2, 0), wire(2, 1, 1), wire(9, 2, 1)]);
        assert!(!w.stale.is_empty());
    }

    #[test]
    fn a_width_above_what_the_node_allows_is_stale() {
        let limited = LaneNode { max_lanes: 8, ..laned(2, 1) };
        let w = infer(&[out(1, 16), limited], &[wire(1, 2, 0)]);
        assert_eq!(w.of_node(NodeId(2)), 1);
        assert_eq!(w.stale, vec![to(2, 0)]);
    }

    #[test]
    fn a_wire_to_a_promoted_port_counts_like_any_input() {
        // Port 1 is past the node's own input: a promoted setting.
        let w = infer(&[out(1, 4), out(2, 4), laned(3, 1)], &[wire(1, 3, 0), wire(2, 3, 1)]);
        assert!(w.stale.is_empty());
        assert_eq!(w.of_wire(to(3, 1)), 4);
        let w = infer(&[out(2, 4), laned(3, 1)], &[wire(2, 3, 1)]);
        assert_eq!(w.of_node(NodeId(3)), 4, "a bus on a setting alone is enough");
    }

    #[test]
    fn wires_to_missing_nodes_are_not_this_functions_business() {
        let w = infer(&[out(1, 4)], &[wire(1, 7, 0)]);
        assert!(w.stale.is_empty());
    }
}
