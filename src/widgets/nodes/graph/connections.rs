use super::*;

impl NodeGraph {
    /// Bundle-connect helper: pair `bundle_size` consecutive ports starting
    /// at the source and target indices, skipping pairs where ports are
    /// missing or types don't match. `dc_from` is the port the user dragged
    /// from (could be input or output); `target` is the snapped destination.
    pub(super) fn connect_bundle(&mut self, dc_from: PortId, target: PortId, bundle_size: usize) {
        let dc_from_dir = dc_from.dir;
        let level = self.active();
        let src_node_id = if dc_from_dir == PortDir::Output { dc_from.node } else { target.node };
        let dst_node_id = if dc_from_dir == PortDir::Output { target.node } else { dc_from.node };
        let src_start = if dc_from_dir == PortDir::Output { dc_from.index } else { target.index };
        let dst_start = if dc_from_dir == PortDir::Output { target.index } else { dc_from.index };

        let src_node_idx = match level.states.iter().position(|s| s.id == src_node_id) {
            Some(i) => i, None => return,
        };
        let dst_node_idx = match level.states.iter().position(|s| s.id == dst_node_id) {
            Some(i) => i, None => return,
        };
        let src_outs = level.nodes[src_node_idx].ui_outputs();
        let dst_ins = level.nodes[dst_node_idx].ui_inputs_full();

        let mut pairs: Vec<(usize, usize)> = Vec::with_capacity(bundle_size);
        for k in 0..bundle_size {
            let si = src_start + k;
            let di = dst_start + k;
            let (Some(src_port), Some(dst_port)) = (src_outs.get(si), dst_ins.get(di)) else {
                continue;
            };
            if src_port.disabled || dst_port.disabled { continue; }
            if !src_port.def.port_type.can_feed(&dst_port.def.port_type) { continue; }
            pairs.push((si, di));
        }

        for (si, di) in pairs {
            let from = make_port_id(src_node_id, PortDir::Output, si);
            let to = make_port_id(dst_node_id, PortDir::Input, di);
            self.remove_connection_to(to);
            self.add_connection(from, to);
        }
    }

    pub fn add_connection(&mut self, from: PortId, to: PortId) {
        let conn = Connection { from, to };
        let level = self.active_mut();
        if !level.connections.contains(&conn) {
            level.connections.push(conn.clone());

            if let (Some(src_idx), Some(dst_idx)) = (
                level.states.iter().position(|s| s.id == from.node),
                level.states.iter().position(|s| s.id == to.node),
            ) {
                let src_type = level.nodes[src_idx]
                    .ui_outputs()
                    .get(from.index)
                    .map(|p| p.def.port_type)
                    .unwrap_or(PortType::Untyped);
                let dst_type = level.nodes[dst_idx]
                    .ui_inputs_full()
                    .get(to.index)
                    .map(|p| p.def.port_type)
                    .unwrap_or(PortType::Untyped);

                // Promoted-param ports lie past the node's own inputs, which
                // its connect hooks know nothing about.
                let promoted_dst = to.index >= level.nodes[dst_idx].ui_inputs().len();

                // Notify destination widget (input got connected).
                if !promoted_dst {
                    level.nodes[dst_idx].on_ui_connect(to.index, src_type);
                }
                // Notify source widget (output got connected).
                level.nodes[src_idx].on_ui_output_connect(from.index, dst_type);

                // Notify engine for on_connect callback.
                if !promoted_dst {
                    self.push_engine_cmd(EngineCommand::NotifyConnect {
                        node_id: to.node,
                        input_port: to.index,
                        source_type: src_type,
                    });
                }
            }

            self.push_engine_cmd(EngineCommand::AddConnection(conn));
        }
    }

    pub(super) fn remove_connection_to(&mut self, to: PortId) {
        let level = self.active_mut();
        let had = level.connections.iter().any(|c| c.to == to);
        // Capture the source port before the connection is removed so we can
        // notify the source widget that one of its outputs lost a connection.
        let removed_from = level.connections.iter().find(|c| c.to == to).map(|c| c.from);
        level.connections.retain(|c| c.to != to);
        let mut promoted_dst = false;
        if had {
            if let Some(dst_idx) = level.states.iter().position(|s| s.id == to.node) {
                promoted_dst = to.index >= level.nodes[dst_idx].ui_inputs().len();
                if !promoted_dst {
                    level.nodes[dst_idx].on_ui_disconnect(to.index);
                }
            }
            // Notify the source widget if its output is now unused (no other
            // connections from the same output port).
            if let Some(from) = removed_from {
                let still_connected = level.connections.iter().any(|c| c.from == from);
                if !still_connected
                    && let Some(src_idx) = level.states.iter().position(|s| s.id == from.node) {
                        level.nodes[src_idx].on_ui_output_disconnect(from.index);
                    }
            }

            // Notify engine.
            self.push_engine_cmd(EngineCommand::RemoveConnectionTo(to));
            if !promoted_dst {
                self.push_engine_cmd(EngineCommand::NotifyDisconnect {
                    node_id: to.node,
                    input_port: to.index,
                });
            }
        }
    }

    /// Work out which ports each node with "hide unused ports" on should not
    /// show: those without a wire, except the "+" port that adds one.
    pub(super) fn refresh_hidden_ports(&mut self) {
        for level in self.levels.iter_mut() {
            let mut wired: std::collections::HashSet<(NodeId, PortDir, usize)> = std::collections::HashSet::new();
            let any = level.nodes.iter().any(|n| n.shared_state().lock().unwrap().hide_unused_ports);
            if any {
                for c in &level.connections {
                    wired.insert((c.from.node, PortDir::Output, c.from.index));
                    wired.insert((c.to.node, PortDir::Input, c.to.index));
                }
            }
            for (node, state) in level.nodes.iter().zip(level.states.iter()) {
                let hide = node.shared_state().lock().unwrap().hide_unused_ports;
                let (hidden_in, hidden_out) = if hide {
                    let hidden = |ports: Vec<UiPortDef>, dir| -> Vec<bool> {
                        ports
                            .iter()
                            .enumerate()
                            .map(|(i, p)| p.marker.is_none() && !wired.contains(&(state.id, dir, i)))
                            .collect()
                    };
                    (hidden(node.ui_inputs_full(), PortDir::Input), hidden(node.ui_outputs(), PortDir::Output))
                } else {
                    (Vec::new(), Vec::new())
                };
                let mut shared = node.shared_state().lock().unwrap();
                if shared.hidden_inputs != hidden_in {
                    shared.hidden_inputs = hidden_in;
                }
                if shared.hidden_outputs != hidden_out {
                    shared.hidden_outputs = hidden_out;
                }
            }
        }
    }

    /// Hide the unwired ports of the selected nodes, or show them all again
    /// when every selected node already hides them.
    pub(super) fn toggle_hide_unused_ports(&mut self) {
        let sel = self.selected_nodes.clone();
        let level = self.active();
        let all_hidden = sel
            .iter()
            .filter_map(|&i| level.nodes.get(i))
            .all(|n| n.shared_state().lock().unwrap().hide_unused_ports);
        for &i in &sel {
            if let Some(n) = level.nodes.get(i) {
                n.shared_state().lock().unwrap().hide_unused_ports = !all_hidden;
            }
        }
    }

    /// When the user promotes or demotes a param, the extra ports after the
    /// node's own inputs change. Move every wire that points at one of them to
    /// its port's new position, and drop the wires of demoted params.
    pub(super) fn remap_promoted_connections(&mut self) {
        let level = self.active_mut();
        // (node, native input count, old list, new list) for nodes that changed.
        let mut changed = Vec::new();
        for (node, state) in level.nodes.iter().zip(level.states.iter()) {
            // Before locking: most widgets lock their own shared state inside
            // `ui_inputs`, and the mutex is not reentrant.
            let native = node.ui_inputs().len();
            let mut shared = node.shared_state().lock().unwrap();
            if shared.promoted != shared.promoted_applied {
                let new = shared.promoted.clone();
                let old = std::mem::replace(&mut shared.promoted_applied, new.clone());
                changed.push((state.id, native, old, new));
            }
        }
        for (node_id, native, old, new) in changed {
            let moved: Vec<(PortId, PortId)> = self
                .active()
                .connections
                .iter()
                .filter(|c| c.to.node == node_id && c.to.index >= native)
                .map(|c| (c.from, c.to))
                .collect();
            for (_, to) in &moved {
                self.remove_connection_to(*to);
            }
            for (from, to) in moved {
                let name = old.get(to.index - native).map(|p| &p.name);
                let new_slot = name.and_then(|n| new.iter().position(|p| &p.name == n));
                if let Some(slot) = new_slot {
                    self.add_connection(from, PortId { index: native + slot, ..to });
                }
            }
        }
    }

    /// A node that had a port removed from the middle of its list reports it
    /// here: the wires on that port go, and the ones on the ports after it
    /// follow them down.
    pub(super) fn remap_removed_ports(&mut self) {
        let level = self.active_mut();
        let ids: Vec<NodeId> = level.states.iter().map(|s| s.id).collect();
        let removed: Vec<(NodeId, PortDir, usize)> = level
            .nodes
            .iter_mut()
            .zip(ids)
            .filter_map(|(node, id)| node.take_removed_port().map(|(dir, i)| (id, dir, i)))
            .collect();
        for (node, dir, index) in removed {
            let on_node = |p: &PortId| p.node == node && p.dir == dir;
            let wires: Vec<(PortId, PortId)> = self
                .active()
                .connections
                .iter()
                .filter(|c| match dir {
                    PortDir::Input => on_node(&c.to) && c.to.index >= index,
                    PortDir::Output => on_node(&c.from) && c.from.index >= index,
                })
                .map(|c| (c.from, c.to))
                .collect();
            for (_, to) in &wires {
                self.remove_connection_to(*to);
            }
            for (from, to) in wires {
                let (from, to) = match dir {
                    PortDir::Input if to.index == index => continue,
                    PortDir::Output if from.index == index => continue,
                    PortDir::Input => (from, PortId { index: to.index - 1, ..to }),
                    PortDir::Output => (PortId { index: from.index - 1, ..from }, to),
                };
                self.add_connection(from, to);
            }
        }
    }

    /// A mode switch reshapes a node's ports; a wire that stays selected across
    /// that ends up pointing at something else, so deselect the node's wires.
    fn deselect_on_port_change(&mut self) {
        use std::hash::{Hash, Hasher};
        let level = &self.levels[self.active_level];
        let mut changed: Vec<NodeId> = Vec::new();
        let mut fresh = std::collections::HashMap::new();
        for (node, state) in level.nodes.iter().zip(level.states.iter()) {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            for p in node.ui_inputs_full() { p.def.name.hash(&mut h); p.def.port_type.hash(&mut h); }
            0xffu8.hash(&mut h);
            for p in node.ui_outputs() { p.def.name.hash(&mut h); p.def.port_type.hash(&mut h); }
            let sig = h.finish();
            if self.port_signatures.get(&state.id).is_some_and(|old| *old != sig) {
                changed.push(state.id);
            }
            fresh.insert(state.id, sig);
        }
        self.port_signatures.extend(fresh);
        if !changed.is_empty() {
            self.selected_connections
                .retain(|c| !changed.contains(&c.from.node) && !changed.contains(&c.to.node));
        }
    }

    pub(crate) fn cleanup_stale_connections(&mut self) {
        self.deselect_on_port_change();
        let level = self.active();
        let stale: Vec<PortId> = level.connections.iter().filter(|conn| {
            let src = level.nodes.iter()
                .zip(level.states.iter())
                .find(|(_, s)| s.id == conn.from.node)
                .and_then(|(n, _)| n.ui_outputs().get(conn.from.index).map(|p| p.def.port_type));
            let dst = level.nodes.iter()
                .zip(level.states.iter())
                .find(|(_, s)| s.id == conn.to.node)
                .and_then(|(n, _)| n.ui_inputs_full().get(conn.to.index).map(|p| p.def.port_type));
            match (src, dst) {
                (Some(s), Some(d)) => !s.can_feed(&d),
                _ => true, // missing endpoint = stale
            }
        }).map(|c| c.to).collect();

        for port_id in stale {
            self.remove_connection_to(port_id);
        }
    }

    /// For exactly two selected nodes, plan a one-shot pairwise connection
    /// from the leftmost node's outputs to the rightmost node's inputs.
    /// Skips disabled ports and pairs only type-compatible ones (in order).
    /// Returns `(source_node_id, destination_node_id, [(src_out, dst_in), ...])`.
    pub(super) fn auto_connect_plan(&self) -> Option<(NodeId, NodeId, Vec<(usize, usize)>)> {
        if self.selected_nodes.len() != 2 { return None; }
        let level = self.active();
        let i_a = self.selected_nodes[0];
        let i_b = self.selected_nodes[1];
        if i_a >= level.nodes.len() || i_b >= level.nodes.len() { return None; }

        // Spatial direction: leftmost node = source.
        let (src_idx, dst_idx) = if level.states[i_a].pos.x <= level.states[i_b].pos.x {
            (i_a, i_b)
        } else {
            (i_b, i_a)
        };

        let src_id = level.states[src_idx].id;
        let dst_id = level.states[dst_idx].id;
        let outs = level.nodes[src_idx].ui_outputs();
        let ins = level.nodes[dst_idx].ui_inputs_full();

        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let mut o = 0usize;
        let mut i = 0usize;
        while o < outs.len() && i < ins.len() {
            if outs[o].disabled { o += 1; continue; }
            if ins[i].disabled { i += 1; continue; }
            if !outs[o].def.port_type.can_feed(&ins[i].def.port_type) {
                // Advance source first; if next source matches, we'll pair
                // with the same input. Otherwise input also gets skipped.
                o += 1;
                continue;
            }
            pairs.push((o, i));
            o += 1;
            i += 1;
        }
        if pairs.is_empty() { return None; }
        Some((src_id, dst_id, pairs))
    }
}

#[cfg(test)]
mod hide_ports_tests {
    use super::*;
    use crate::engine::nodes::math::math_op::MathOp;
    use crate::engine::types::new_shared_state;
    use crate::widgets::nodes::math::constant::ConstantWidget;
    use crate::widgets::nodes::math::math_op::MathWidget;

    /// An Add (inputs a, b and the "+" port; one output) with a constant on `a`.
    fn graph() -> NodeGraph {
        let mut g = NodeGraph::new();
        g.add_node(Box::new(MathWidget::new(NodeId(1), MathOp::Add, new_shared_state(8, 1))), Pos2::new(200.0, 0.0));
        g.add_node(
            Box::new(ConstantWidget::new(NodeId(2), PortType::Untyped, new_shared_state(0, 1))),
            Pos2::new(0.0, 0.0),
        );
        let from = PortId { node: NodeId(2), index: 0, dir: PortDir::Output };
        let to = PortId { node: NodeId(1), index: 0, dir: PortDir::Input };
        g.add_connection(from, to);
        g
    }

    #[test]
    fn only_wired_ports_and_the_plus_port_stay_when_unused_ports_are_hidden() {
        let mut g = graph();
        g.refresh_hidden_ports();
        assert!(g.active().nodes[0].hidden_ports(PortDir::Input).is_empty(), "nothing hidden by default");

        g.selected_nodes = vec![0];
        g.toggle_hide_unused_ports();
        g.refresh_hidden_ports();
        let node = &g.active().nodes[0];
        // a is wired, b is not, "+" always stays.
        assert_eq!(node.hidden_ports(PortDir::Input), vec![false, true, false]);
        // The output has no wire.
        assert_eq!(node.hidden_ports(PortDir::Output), vec![true]);
        // The constant's output is wired but its node does not hide anything.
        assert!(g.active().nodes[1].hidden_ports(PortDir::Output).is_empty());

        g.toggle_hide_unused_ports();
        g.refresh_hidden_ports();
        assert!(g.active().nodes[0].hidden_ports(PortDir::Input).is_empty(), "toggled back");
    }

    #[test]
    fn hidden_ports_close_up_and_the_node_shrinks() {
        let mut g = graph();
        let full = node_size(g.active().nodes[0].as_ref(), None);
        let rect = Rect::from_min_size(Pos2::ZERO, full);
        let gap_before = {
            let n = g.active().nodes[0].as_ref();
            n.port_pos(rect, PortDir::Input, 2, 1.0).y - n.port_pos(rect, PortDir::Input, 0, 1.0).y
        };

        g.selected_nodes = vec![0];
        g.toggle_hide_unused_ports();
        g.refresh_hidden_ports();
        let node = g.active().nodes[0].as_ref();
        let small = node_size(node, None);
        assert!(small.y <= full.y);
        let rect = Rect::from_min_size(Pos2::ZERO, small);
        let a = node.port_pos(rect, PortDir::Input, 0, 1.0);
        let plus = node.port_pos(rect, PortDir::Input, 2, 1.0);
        assert!(plus.y - a.y <= gap_before - PORT_SPACING + 0.01, "b is gone from between them");
    }

    #[test]
    fn a_wire_added_later_shows_its_port_again() {
        let mut g = graph();
        g.selected_nodes = vec![0];
        g.toggle_hide_unused_ports();
        let from = PortId { node: NodeId(2), index: 0, dir: PortDir::Output };
        let to = PortId { node: NodeId(1), index: 1, dir: PortDir::Input };
        g.add_connection(from, to);
        g.refresh_hidden_ports();
        assert_eq!(g.active().nodes[0].hidden_ports(PortDir::Input), vec![false, false, false]);
    }
}

#[cfg(test)]
mod removed_port_tests {
    use super::*;
    use crate::engine::types::new_shared_state;
    use crate::midi_io::MidiHub;
    use crate::widgets::nodes::display::value_display::ValueDisplayWidget;
    use crate::widgets::nodes::io::midi_common::new_shared_learned;
    use crate::widgets::nodes::io::midi_input::MidiInputWidget;
    use crate::widgets::nodes::io::midi_output::MidiOutputWidget;
    use crate::widgets::nodes::meta::subgraph::SubgraphWidget;
    use crate::engine::nodes::meta::subgraph::SubgraphPortDef;

    fn three_entries() -> serde_json::Value {
        serde_json::json!({ "port": "dev", "entries": [
            { "id": 1, "name": "a", "binding": {"kind": "cc", "channel": 1, "number": 1}, "mode": "value" },
            { "id": 2, "name": "b", "binding": {"kind": "cc", "channel": 1, "number": 2}, "mode": "value" },
            { "id": 3, "name": "c", "binding": {"kind": "cc", "channel": 1, "number": 3}, "mode": "value" },
        ]})
    }

    /// A MIDI Input (id 1) with entries a, b, c wired to three displays (ids 10, 11, 12).
    fn input_graph() -> NodeGraph {
        let mut g = NodeGraph::new();
        let mut input = MidiInputWidget::new(NodeId(1), new_shared_state(0, 33), MidiHub::idle(), new_shared_learned());
        input.restore_from_save_data(&three_entries());
        g.add_node(Box::new(input), Pos2::ZERO);
        for k in 0..3u64 {
            g.add_node(Box::new(ValueDisplayWidget::new(NodeId(10 + k), new_shared_state(1, 0))), Pos2::new(200.0, k as f32 * 50.0));
            let from = PortId { node: NodeId(1), index: 1 + k as usize, dir: PortDir::Output };
            let to = PortId { node: NodeId(10 + k), index: 0, dir: PortDir::Input };
            g.add_connection(from, to);
        }
        g
    }

    fn wires(g: &NodeGraph) -> Vec<(u64, usize)> {
        let mut w: Vec<(u64, usize)> = g.active().connections.iter().map(|c| (c.to.node.0, c.from.index)).collect();
        w.sort();
        w
    }

    #[test]
    fn removing_an_input_entry_drops_its_wire_and_moves_the_ones_after_it_down() {
        let mut g = input_graph();
        assert_eq!(wires(&g), vec![(10, 1), (11, 2), (12, 3)]);
        g.node_mut(0).as_any_mut().downcast_mut::<MidiInputWidget>().unwrap().remove_entry(1);
        g.remap_removed_ports();
        g.cleanup_stale_connections();
        assert_eq!(wires(&g), vec![(10, 1), (12, 2)], "b's wire is gone, c's now leaves from the second entry's port");
    }

    #[test]
    fn removing_the_last_entry_just_drops_its_wire() {
        let mut g = input_graph();
        g.node_mut(0).as_any_mut().downcast_mut::<MidiInputWidget>().unwrap().remove_entry(2);
        g.remap_removed_ports();
        g.cleanup_stale_connections();
        assert_eq!(wires(&g), vec![(10, 1), (11, 2)]);
    }

    #[test]
    fn removing_an_output_entry_does_the_same_on_the_input_side() {
        let mut g = NodeGraph::new();
        let mut out = MidiOutputWidget::new(NodeId(1), new_shared_state(32, 1), MidiHub::idle(), new_shared_learned());
        out.restore_from_save_data(&three_entries());
        g.add_node(Box::new(out), Pos2::new(200.0, 0.0));
        for k in 0..3u64 {
            g.add_node(
                Box::new(crate::widgets::nodes::math::constant::ConstantWidget::new(NodeId(10 + k), PortType::Untyped, new_shared_state(0, 1))),
                Pos2::new(0.0, k as f32 * 50.0),
            );
            g.add_connection(
                PortId { node: NodeId(10 + k), index: 0, dir: PortDir::Output },
                PortId { node: NodeId(1), index: k as usize, dir: PortDir::Input },
            );
        }
        g.node_mut(0).as_any_mut().downcast_mut::<MidiOutputWidget>().unwrap().remove_entry(0);
        g.remap_removed_ports();
        g.cleanup_stale_connections();
        let mut w: Vec<(u64, usize)> = g.active().connections.iter().map(|c| (c.from.node.0, c.to.index)).collect();
        w.sort();
        assert_eq!(w, vec![(11, 0), (12, 1)]);
    }

    fn constant(id: u64) -> Box<dyn NodeWidget> {
        Box::new(crate::widgets::nodes::math::constant::ConstantWidget::new(NodeId(id), PortType::Untyped, new_shared_state(0, 1)))
    }

    fn display(id: u64) -> Box<dyn NodeWidget> {
        Box::new(ValueDisplayWidget::new(NodeId(id), new_shared_state(1, 0)))
    }

    /// Constants 10..13 wired into the first three input ports of node 1.
    fn wire_inputs_of_node_1(g: &mut NodeGraph) {
        for k in 0..3u64 {
            g.add_node(constant(10 + k), Pos2::new(0.0, k as f32 * 50.0));
            g.add_connection(
                PortId { node: NodeId(10 + k), index: 0, dir: PortDir::Output },
                PortId { node: NodeId(1), index: k as usize, dir: PortDir::Input },
            );
        }
    }

    /// The first three output ports of node 1 wired into displays 10..13.
    fn wire_outputs_of_node_1(g: &mut NodeGraph, first_port: usize) {
        for k in 0..3u64 {
            g.add_node(display(10 + k), Pos2::new(200.0, k as f32 * 50.0));
            g.add_connection(
                PortId { node: NodeId(1), index: first_port + k as usize, dir: PortDir::Output },
                PortId { node: NodeId(10 + k), index: 0, dir: PortDir::Input },
            );
        }
    }

    fn input_wires(g: &NodeGraph) -> Vec<(u64, usize)> {
        let mut w: Vec<(u64, usize)> = g.active().connections.iter().map(|c| (c.from.node.0, c.to.index)).collect();
        w.sort();
        w
    }

    fn three_subgraph_ports() -> Vec<SubgraphPortDef> {
        ["a", "b", "c"].iter().map(|n| SubgraphPortDef { name: n.to_string(), port_type_idx: 2 }).collect()
    }

    #[test]
    fn removing_a_subgraph_input_moves_the_outer_wires_after_it_down() {
        let mut g = NodeGraph::new();
        let mut sub = SubgraphWidget::new(NodeId(1), new_shared_state(3, 3));
        sub.input_defs = three_subgraph_ports();
        g.add_node(Box::new(sub), Pos2::new(200.0, 0.0));
        wire_inputs_of_node_1(&mut g);
        assert_eq!(input_wires(&g), vec![(10, 0), (11, 1), (12, 2)]);
        g.node_mut(0).as_any_mut().downcast_mut::<SubgraphWidget>().unwrap().remove_input(1);
        g.remap_removed_ports();
        g.cleanup_stale_connections();
        assert_eq!(input_wires(&g), vec![(10, 0), (12, 1)]);
    }

    #[test]
    fn removing_a_subgraph_output_moves_the_outer_wires_after_it_down() {
        let mut g = NodeGraph::new();
        let mut sub = SubgraphWidget::new(NodeId(1), new_shared_state(3, 3));
        sub.output_defs = three_subgraph_ports();
        g.add_node(Box::new(sub), Pos2::ZERO);
        wire_outputs_of_node_1(&mut g, 0);
        g.node_mut(0).as_any_mut().downcast_mut::<SubgraphWidget>().unwrap().remove_output(0);
        g.remap_removed_ports();
        g.cleanup_stale_connections();
        assert_eq!(wires(&g), vec![(11, 0), (12, 1)]);
    }

    #[test]
    fn removing_a_portal_send_port_moves_the_wires_after_it_down() {
        use crate::widgets::nodes::meta::portal::PortalSendWidget;
        let mut g = NodeGraph::new();
        let mut send = PortalSendWidget::new(NodeId(1), new_shared_state(3, 0));
        send.restore_from_save_data(&serde_json::json!({ "name": "p", "ports": [
            { "name": "a", "port_type_idx": 2 }, { "name": "b", "port_type_idx": 2 }, { "name": "c", "port_type_idx": 2 },
        ]}));
        g.add_node(Box::new(send), Pos2::new(200.0, 0.0));
        wire_inputs_of_node_1(&mut g);
        assert_eq!(input_wires(&g), vec![(10, 0), (11, 1), (12, 2)]);
        g.node_mut(0).as_any_mut().downcast_mut::<PortalSendWidget>().unwrap().remove_port(0);
        g.remap_removed_ports();
        g.cleanup_stale_connections();
        assert_eq!(input_wires(&g), vec![(11, 0), (12, 1)]);
    }

    #[test]
    fn removing_a_lookup_column_moves_the_output_wires_after_it_down() {
        use crate::widgets::nodes::math::lookup::LookupWidget;
        let mut g = NodeGraph::new();
        let mut lookup = LookupWidget::new(
            NodeId(1),
            new_shared_state(1, 321),
            crate::widgets::nodes::math::gradient_source::new_shared_gradient_library(),
        );
        lookup.restore_from_save_data(&serde_json::json!({ "columns": [
            { "name": "a" }, { "name": "b" }, { "name": "c" },
        ]}));
        g.add_node(Box::new(lookup), Pos2::ZERO);
        // Output 0 is "rows"; the columns are ports 1..=3.
        wire_outputs_of_node_1(&mut g, 1);
        assert_eq!(wires(&g), vec![(10, 1), (11, 2), (12, 3)]);
        g.node_mut(0).as_any_mut().downcast_mut::<LookupWidget>().unwrap().remove_column(1);
        g.remap_removed_ports();
        g.cleanup_stale_connections();
        assert_eq!(wires(&g), vec![(10, 1), (12, 2)]);
    }
}
