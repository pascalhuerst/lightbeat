//! Which wires of a level stand out. Plain functions over the connection list,
//! so the drawing code only asks "is wire i bright?".

use std::collections::{HashMap, HashSet};

use egui::{Pos2, Vec2};

use crate::engine::types::{Connection, NodeId};

/// Wires touching one of the `selected` nodes.
pub(super) fn wires_of_nodes(conns: &[Connection], selected: &HashSet<NodeId>) -> Vec<bool> {
    conns.iter()
        .map(|c| selected.contains(&c.from.node) || selected.contains(&c.to.node))
        .collect()
}

/// The whole run through wire `hovered`: the wire itself, everything upstream
/// of its source node and everything downstream of its destination node,
/// following wires through nodes. A node is visited once, so a feedback loop
/// ends the walk instead of repeating it.
pub(super) fn wires_in_run(conns: &[Connection], hovered: usize) -> Vec<bool> {
    let mut lit = vec![false; conns.len()];
    let Some(start) = conns.get(hovered) else { return lit };
    lit[hovered] = true;

    let mut by_dest: HashMap<NodeId, Vec<usize>> = HashMap::new();
    let mut by_source: HashMap<NodeId, Vec<usize>> = HashMap::new();
    for (i, c) in conns.iter().enumerate() {
        by_dest.entry(c.to.node).or_default().push(i);
        by_source.entry(c.from.node).or_default().push(i);
    }

    let mut seen: HashSet<NodeId> = HashSet::new();
    let mut stack = vec![start.from.node];
    seen.insert(start.from.node);
    while let Some(node) = stack.pop() {
        for &i in by_dest.get(&node).into_iter().flatten() {
            lit[i] = true;
            if seen.insert(conns[i].from.node) {
                stack.push(conns[i].from.node);
            }
        }
    }

    let mut seen: HashSet<NodeId> = HashSet::new();
    let mut stack = vec![start.to.node];
    seen.insert(start.to.node);
    while let Some(node) = stack.pop() {
        for &i in by_source.get(&node).into_iter().flatten() {
            lit[i] = true;
            if seen.insert(conns[i].to.node) {
                stack.push(conns[i].to.node);
            }
        }
    }
    lit
}

/// Wires that run between the same two nodes.
#[derive(Debug, PartialEq)]
pub(super) struct Cable {
    pub from: NodeId,
    pub to: NodeId,
    /// Indices into the level's connection list, in list order.
    pub wires: Vec<usize>,
}

/// The sets of two or more wires with the same source node and destination
/// node, in the order their first wire appears.
pub(super) fn cable_groups(conns: &[Connection]) -> Vec<Cable> {
    let mut slot: HashMap<(NodeId, NodeId), usize> = HashMap::new();
    let mut all: Vec<Cable> = Vec::new();
    for (i, c) in conns.iter().enumerate() {
        let key = (c.from.node, c.to.node);
        let at = *slot.entry(key).or_insert_with(|| {
            all.push(Cable { from: key.0, to: key.1, wires: Vec::new() });
            all.len() - 1
        });
        all[at].wires.push(i);
    }
    all.retain(|c| c.wires.len() >= 2);
    all
}

/// Where the fanned-out wires of a cable meet the single thick wire: a step
/// out of the mean of the source ports and a step short of the mean of the
/// destination ports. Both ends of that wire are horizontal, as a bezier
/// leaves a port.
pub(super) fn cable_anchors(from_ports: &[Pos2], to_ports: &[Pos2], fan: f32) -> (Pos2, Pos2) {
    let mean = |pts: &[Pos2]| {
        let sum = pts.iter().fold(Vec2::ZERO, |acc, p| acc + p.to_vec2());
        (sum / pts.len().max(1) as f32).to_pos2()
    };
    let (f, t) = (mean(from_ports), mean(to_ports));
    (Pos2::new(f.x + fan, f.y), Pos2::new(t.x - fan, t.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::{PortDir, PortId};

    fn conn(from: u64, to: u64) -> Connection {
        Connection {
            from: PortId { node: NodeId(from), index: 0, dir: PortDir::Output },
            to: PortId { node: NodeId(to), index: 0, dir: PortDir::Input },
        }
    }

    #[test]
    fn selection_lights_wires_at_either_end() {
        let conns = [conn(1, 2), conn(2, 3), conn(4, 5)];
        let sel: HashSet<NodeId> = [NodeId(2)].into_iter().collect();
        assert_eq!(wires_of_nodes(&conns, &sel), vec![true, true, false]);
        assert_eq!(wires_of_nodes(&conns, &HashSet::new()), vec![false; 3]);
    }

    #[test]
    fn run_follows_upstream_and_downstream_only() {
        // 1 -> 2 -> 3 -> 4, with side branches: 9 -> 3 (joins the run's
        // upstream of 3 but is not upstream of the hovered wire) and 2 -> 8.
        let conns = [conn(1, 2), conn(2, 3), conn(3, 4), conn(9, 3), conn(2, 8), conn(7, 6)];
        let lit = wires_in_run(&conns, 1);
        // Hovered 2 -> 3: upstream is 1 -> 2, downstream is 3 -> 4.
        // 9 -> 3 feeds the destination, not the source: it stays dim, as
        // does the branch 2 -> 8 that leaves the source.
        assert_eq!(lit, vec![true, true, true, false, false, false]);
    }

    #[test]
    fn run_includes_every_feeder_of_the_source() {
        let conns = [conn(1, 3), conn(2, 3), conn(3, 4)];
        let lit = wires_in_run(&conns, 2);
        assert_eq!(lit, vec![true, true, true]);
    }

    #[test]
    fn run_survives_a_loop() {
        let conns = [conn(1, 2), conn(2, 3), conn(3, 1)];
        let lit = wires_in_run(&conns, 0);
        assert_eq!(lit, vec![true, true, true]);
    }

    #[test]
    fn run_of_a_missing_wire_is_empty() {
        assert_eq!(wires_in_run(&[conn(1, 2)], 5), vec![false]);
    }

    #[test]
    fn anchors_sit_a_fan_away_from_the_mean_of_the_ports() {
        let (a, b) = cable_anchors(
            &[Pos2::new(10.0, 0.0), Pos2::new(10.0, 20.0)],
            &[Pos2::new(110.0, 40.0), Pos2::new(110.0, 60.0)],
            8.0,
        );
        assert_eq!(a, Pos2::new(18.0, 10.0));
        assert_eq!(b, Pos2::new(102.0, 50.0));
    }

    #[test]
    fn parallel_wires_form_a_cable() {
        let conns = [conn(1, 2), conn(3, 4), conn(1, 2), conn(2, 1), conn(1, 2), conn(3, 4)];
        let cables = cable_groups(&conns);
        assert_eq!(cables, vec![
            Cable { from: NodeId(1), to: NodeId(2), wires: vec![0, 2, 4] },
            Cable { from: NodeId(3), to: NodeId(4), wires: vec![1, 5] },
        ]);
    }

    #[test]
    fn a_single_wire_is_not_a_cable() {
        assert!(cable_groups(&[conn(1, 2), conn(2, 3), conn(2, 1)]).is_empty());
        assert!(cable_groups(&[]).is_empty());
    }
}
