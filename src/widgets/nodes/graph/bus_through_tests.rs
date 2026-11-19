//! A bus across Subgraph ports and Portals, with a real engine behind the
//! graph: the widgets are made by the app's node table, the engine commands
//! the graph sends are applied to a root `InnerGraph` the way the engine
//! thread applies them, and values are read from the engine nodes.

use std::rc::Rc;

use super::*;
use crate::app::registry::{NodeContext, NodeDescriptor, lane_limits, node_table, test_context};
use crate::engine::nodes::math::multiplex::type_to_index;
use crate::engine::nodes::meta::subgraph::InnerGraph;
use crate::engine::types::new_shared_state;

pub(super) struct Rig {
    pub(super) g: NodeGraph,
    pub(super) root: InnerGraph,
    ctx: NodeContext,
    table: Rc<Vec<NodeDescriptor>>,
}

pub(super) fn out(node: NodeId, index: usize) -> PortId {
    PortId { node, index, dir: PortDir::Output }
}

pub(super) fn inp(node: NodeId, index: usize) -> PortId {
    PortId { node, index, dir: PortDir::Input }
}

impl Rig {
    pub(super) fn new() -> Self {
        let ctx = test_context();
        let mut g = NodeGraph::new();
        for desc in node_table() {
            let (inputs, outputs) = desc.channels;
            let ctx = ctx.clone();
            g.register_node(desc.category, desc.name, move |id| (desc.widget)(id, new_shared_state(inputs, outputs), &ctx));
        }
        g.set_lane_limits(lane_limits(&ctx));
        let table = Rc::new(node_table());
        let (factory_table, factory_ctx) = (table.clone(), ctx.clone());
        g.set_engine_factory(Rc::new(move |type_name, id| {
            let desc = factory_table.iter().find(|d| d.name == type_name)?;
            Some((desc.engine)(id, &factory_ctx))
        }));
        Self { g, root: InnerGraph::new_root(), ctx, table }
    }

    /// Add a node of a type to the open level; `params` are its settings, as a
    /// project would restore them.
    pub(super) fn add(&mut self, type_name: &str, params: &[(usize, ParamValue)]) -> NodeId {
        self.add_with(type_name, None, params)
    }

    pub(super) fn add_with(&mut self, type_name: &str, data: Option<&serde_json::Value>, params: &[(usize, ParamValue)]) -> NodeId {
        let id = self.g.alloc_id();
        let mut node = self.g.create_from_registry(type_name, id).expect("a registered type");
        node.restore_from_save(data, params);
        node.shared_state().lock().pending_params.extend(params.iter().cloned());
        self.g.add_node(node, Pos2::ZERO);
        id
    }

    pub(super) fn merge(&mut self, inputs: i64) -> NodeId {
        self.add("Bus Merge", &[(0, ParamValue::Choice(type_to_index(PortType::Untyped))), (1, ParamValue::Int(inputs))])
    }

    pub(super) fn subgraph(&mut self, inputs: usize, outputs: usize) -> NodeId {
        let ports = |n: usize, name: &str, ty: usize| (0..n).map(|i| serde_json::json!({"name": format!("{name} {i}"), "port_type_idx": ty})).collect::<Vec<_>>();
        let data = serde_json::json!({"name": "S", "inputs": ports(inputs, "in", 2), "outputs": ports(outputs, "out", 0)});
        self.add_with("Subgraph", Some(&data), &[])
    }

    pub(super) fn wire(&mut self, from: (NodeId, usize), to: (NodeId, usize)) {
        self.g.add_connection(out(from.0, from.1), inp(to.0, to.1));
    }

    pub(super) fn set(&mut self, node: NodeId, index: usize, value: ParamValue) {
        let i = self.g.active().states.iter().position(|s| s.id == node).unwrap();
        let w = self.g.node_mut(i);
        w.restore_from_save(None, &[(index, value.clone())]);
        w.shared_state().lock().pending_params.push((index, value));
    }

    pub(super) fn enter(&mut self, sub: NodeId) {
        let i = self.g.active().states.iter().position(|s| s.id == sub).unwrap();
        self.g.navigate_into_by_index(i);
    }

    pub(super) fn up(&mut self) {
        self.g.navigate_up();
    }

    /// What the app does each frame: engine halves for new widgets, the
    /// graph's upkeep, then the commands that came out of it.
    pub(super) fn pump(&mut self) {
        for _ in 0..8 {
            for new in self.g.drain_new_nodes() {
                let node = self.g.node_mut_at_path(new.index, &new.subgraph_path);
                let (type_name, id, shared) = (node.type_name(), node.node_id(), std::sync::Arc::clone(node.shared_state()));
                let desc = self.table.iter().find(|d| d.name == type_name).unwrap();
                let node = (desc.engine)(id, &self.ctx);
                let cmd = if new.subgraph_path.is_empty() {
                    EngineCommand::AddNode { node, shared }
                } else {
                    EngineCommand::SubgraphInnerCommand {
                        subgraph_path: new.subgraph_path,
                        command: Box::new(SubgraphInnerCmd::AddNode { node, shared }),
                    }
                };
                self.root.apply_command(cmd);
            }
            self.g.maintain();
            for cmd in self.g.drain_engine_commands() {
                self.root.apply_command(cmd);
            }
        }
    }

    pub(super) fn tick(&mut self, n: usize) {
        for _ in 0..n {
            self.root.tick(&[], &[]);
        }
    }

    /// Output `index` (a channel) of a node at the root, as the engine has it.
    pub(super) fn read(&self, node: NodeId, channel: usize) -> f32 {
        let i = self.root.index_of(node).expect("a node of the root, after a tick");
        self.root.nodes[i].read_output(channel)
    }

    /// Output `channel` of a node inside the subgraphs `path` names.
    pub(super) fn read_at(&self, path: &[NodeId], node: NodeId, channel: usize) -> f32 {
        use crate::engine::nodes::meta::subgraph::SubgraphProcessNode;
        let mut graph = &self.root;
        for id in path {
            let i = graph.index_of(*id).expect("a subgraph on the path, after a tick");
            graph = &graph.nodes[i].as_any().downcast_ref::<SubgraphProcessNode>().expect("a subgraph").inner;
        }
        graph.nodes[graph.index_of(node).expect("a node, after a tick")].read_output(channel)
    }

    /// A Portal Send (one number port) or Receive of a name, with the config
    /// a load would have handed its engine half.
    pub(super) fn portal(&mut self, send: bool, name: &str) -> NodeId {
        let ports = serde_json::json!([{"name": "in 1", "port_type_idx": 2}]);
        let (type_name, data) = if send {
            ("Portal Send", serde_json::json!({"name": name, "ports": ports}))
        } else {
            ("Portal Receive", serde_json::json!({"bound_name": name, "ports": ports}))
        };
        let id = self.add_with(type_name, Some(&data), &[]);
        let i = self.g.active().states.iter().position(|s| s.id == id).unwrap();
        self.g.node_mut(i).shared_state().lock().pending_config = Some(data);
        id
    }

    pub(super) fn level(&self, sub: NodeId) -> usize {
        self.g.level_of_subgraph(sub).expect("an opened subgraph")
    }

    /// The width the graph derived for a node of a level.
    pub(super) fn width(&self, level: usize, node: NodeId) -> usize {
        self.g.levels[level].bus.widths.of_node(node)
    }

    pub(super) fn outputs_of(&self, level: usize, node: NodeId) -> Vec<usize> {
        let l = &self.g.levels[level];
        let i = l.states.iter().position(|s| s.id == node).unwrap();
        l.nodes[i].ui_outputs().iter().map(|p| p.def.lanes).collect()
    }
}

const PATTERN: [f32; 8] = [0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0];

/// Eight constants (of `PATTERN`, as far as `count` goes) into a Merge.
fn source(r: &mut Rig, count: usize) -> NodeId {
    let merge = r.merge(count as i64);
    for (k, v) in PATTERN.iter().take(count).enumerate() {
        let c = r.add("Const Value", &[(0, ParamValue::Float(*v))]);
        r.wire((c, 0), (merge, k));
    }
    merge
}

fn lane_values(r: &Rig, split: NodeId, count: usize) -> Vec<f32> {
    (0..count).map(|k| r.read(split, k)).collect()
}

/// Graph Input -> Schmitt Trigger -> Graph Output, inside the open subgraph.
fn schmitt_inside(r: &mut Rig) -> NodeId {
    let s = r.add("Schmitt Trigger", &[]);
    r.wire((BRIDGE_IN_NODE_ID, 0), (s, 0));
    r.wire((s, 0), (BRIDGE_OUT_NODE_ID, 0));
    s
}

#[test]
fn a_bus_goes_through_a_subgraph_and_comes_out_as_wide() {
    let mut r = Rig::new();
    let merge = source(&mut r, 8);
    let sub = r.subgraph(1, 1);
    let split = r.add("Bus Split", &[]);
    r.enter(sub);
    let inner = schmitt_inside(&mut r);
    r.up();
    r.wire((merge, 0), (sub, 0));
    r.wire((sub, 0), (split, 0));
    r.pump();
    r.tick(60);

    let level = r.level(sub);
    assert_eq!(r.outputs_of(level, BRIDGE_IN_NODE_ID), vec![8], "the bridge hands the bus on");
    assert_eq!(r.width(level, inner), 8, "the node inside is run as lanes");
    assert_eq!(r.outputs_of(0, sub), vec![8], "the subgraph leaves as wide as the bus inside");
    assert_eq!(r.outputs_of(0, split).len(), 8);
    assert_eq!(lane_values(&r, split, 8), PATTERN.to_vec());
    assert_eq!(r.g.levels[0].connections.len(), 8 + 2, "no wire was dropped");
}

#[test]
fn a_width_that_changes_outside_is_followed_inside() {
    let mut r = Rig::new();
    let merge = source(&mut r, 8);
    let sub = r.subgraph(1, 1);
    let split = r.add("Bus Split", &[]);
    r.enter(sub);
    let inner = schmitt_inside(&mut r);
    r.up();
    r.wire((merge, 0), (sub, 0));
    r.wire((sub, 0), (split, 0));
    r.pump();
    r.tick(40);

    r.set(merge, 1, ParamValue::Int(4));
    r.pump();
    r.tick(40);
    let level = r.level(sub);
    assert_eq!(r.width(level, inner), 4);
    assert_eq!(r.outputs_of(0, split).len(), 4);
    assert_eq!(lane_values(&r, split, 4), PATTERN[..4].to_vec());

    // Back to a single wire: everything collapses to one lane.
    r.g.remove_connection_to(inp(sub, 0));
    let c = r.add("Const Value", &[(0, ParamValue::Float(1.0))]);
    r.wire((c, 0), (sub, 0));
    r.pump();
    r.tick(40);
    assert_eq!(r.width(level, inner), 1);
    assert_eq!(r.outputs_of(0, sub), vec![1]);
    assert_eq!(r.read(split, 0), 1.0);
}

#[test]
fn a_bus_crosses_two_levels_of_subgraphs() {
    let mut r = Rig::new();
    let merge = source(&mut r, 8);
    let outer = r.subgraph(1, 1);
    let split = r.add("Bus Split", &[]);
    r.enter(outer);
    let middle = r.subgraph(1, 1);
    r.wire((BRIDGE_IN_NODE_ID, 0), (middle, 0));
    r.wire((middle, 0), (BRIDGE_OUT_NODE_ID, 0));
    r.enter(middle);
    let inner = schmitt_inside(&mut r);
    r.up();
    r.up();
    r.wire((merge, 0), (outer, 0));
    r.wire((outer, 0), (split, 0));
    r.pump();
    r.tick(80);

    assert_eq!(r.width(r.level(middle), inner), 8);
    assert_eq!(r.outputs_of(r.level(outer), middle), vec![8]);
    assert_eq!(r.outputs_of(0, outer), vec![8]);
    assert_eq!(lane_values(&r, split, 8), PATTERN.to_vec());
}

#[test]
fn nothing_about_widths_is_saved_and_a_loaded_project_derives_them_again() {
    let mut r = Rig::new();
    let merge = source(&mut r, 8);
    let sub = r.subgraph(1, 1);
    let split = r.add("Bus Split", &[]);
    r.enter(sub);
    let inner = schmitt_inside(&mut r);
    r.up();
    r.wire((merge, 0), (sub, 0));
    r.wire((sub, 0), (split, 0));
    r.pump();
    // A save reads what the engine mirrored into the shared states.
    r.tick(64);

    let saved = crate::project::save_graph(&r.g);
    let text = serde_json::to_string(&saved).unwrap();
    assert!(!text.contains("lanes"), "a width is derived, never saved: {text}");

    let mut again = Rig::new();
    crate::project::load_graph(&mut again.g, &saved);
    again.pump();
    again.tick(60);
    assert_eq!(again.g.active_level_index(), 0, "the inner level was never open for this");
    assert_eq!(again.width(again.level(sub), inner), 8);
    assert_eq!(lane_values(&again, split, 8), PATTERN.to_vec());
}

/// A Receive of "bus" and a Split behind it in the open level.
fn receive_into_split(r: &mut Rig) -> (NodeId, NodeId) {
    let rx = r.portal(false, "bus");
    let split = r.add("Bus Split", &[]);
    r.wire((rx, 0), (split, 0));
    (rx, split)
}

#[test]
fn a_bus_through_a_portal_arrives_as_wide_on_every_level() {
    let mut r = Rig::new();
    let merge = source(&mut r, 8);
    let send = r.portal(true, "bus");
    let (root_rx, root_split) = receive_into_split(&mut r);
    let x = r.subgraph(0, 0);
    let y = r.subgraph(0, 0);
    r.enter(x);
    let (x_rx, x_split) = receive_into_split(&mut r);
    r.up();
    r.enter(y);
    let (y_rx, y_split) = receive_into_split(&mut r);
    r.up();
    r.wire((merge, 0), (send, 0));
    r.pump();
    r.tick(60);

    assert_eq!(r.outputs_of(0, root_rx), vec![8]);
    assert_eq!(r.outputs_of(r.level(x), x_rx), vec![8]);
    assert_eq!(r.outputs_of(r.level(y), y_rx), vec![8]);
    assert_eq!(lane_values(&r, root_split, 8), PATTERN.to_vec());
    let at = |path: &[NodeId], split| (0..8).map(|k| r.read_at(path, split, k)).collect::<Vec<_>>();
    assert_eq!(at(&[x], x_split), PATTERN.to_vec());
    assert_eq!(at(&[y], y_split), PATTERN.to_vec());
    assert_eq!(r.g.levels[r.level(x)].connections.len(), 1, "the wire behind the Receive stayed");

    // The wide source goes: every Receive is a single wire again.
    r.g.remove_connection_to(inp(send, 0));
    r.pump();
    r.tick(20);
    assert_eq!(r.outputs_of(0, root_rx), vec![1]);
    assert_eq!(r.outputs_of(r.level(x), x_rx), vec![1]);
    assert_eq!(r.outputs_of(r.level(y), y_rx), vec![1]);
    assert_eq!(r.outputs_of(r.level(y), y_split).len(), 1);
}

#[test]
fn a_portal_loop_with_no_source_stays_one_wide() {
    // Send -> Receive -> (laned) Schmitt -> Send: nothing in it is wide.
    let mut r = Rig::new();
    let send = r.portal(true, "loop");
    let rx = r.portal(false, "loop");
    let schmitt = r.add("Schmitt Trigger", &[]);
    r.wire((rx, 0), (schmitt, 0));
    r.wire((schmitt, 0), (send, 0));
    r.pump();
    assert_eq!(r.outputs_of(0, rx), vec![1]);
    assert_eq!(r.width(0, schmitt), 1);
}

#[test]
fn a_bus_into_a_loop_through_a_portal_settles() {
    // Merge(8) and a Receive both feed an Add laned to 8, which feeds the Send
    // the Receive listens to. The loop takes the source's width and stays there.
    let mut r = Rig::new();
    let merge = source(&mut r, 8);
    let send = r.portal(true, "loop");
    let rx = r.portal(false, "loop");
    let add = r.add("Add", &[]);
    r.wire((merge, 0), (add, 0));
    r.wire((rx, 0), (add, 1));
    r.wire((add, 0), (send, 0));
    r.pump();
    assert_eq!((r.outputs_of(0, rx), r.width(0, add)), (vec![8], 8));
    assert_eq!(r.g.levels[0].bus.widths.stale.len(), 0);

    // Without the source the loop has nothing to hold it up.
    r.g.remove_connection_to(inp(add, 0));
    r.pump();
    assert_eq!((r.outputs_of(0, rx), r.width(0, add)), (vec![1], 1));
}

/// Merge(8) -> Schmitt Trigger (run as lanes) -> Split, all at the root.
fn chain(r: &mut Rig) -> (NodeId, NodeId, NodeId) {
    let merge = source(r, 8);
    let schmitt = r.add("Schmitt Trigger", &[]);
    let split = r.add("Bus Split", &[]);
    r.wire((merge, 0), (schmitt, 0));
    r.wire((schmitt, 0), (split, 0));
    r.pump();
    r.tick(40);
    (merge, schmitt, split)
}

fn index_of(r: &Rig, node: NodeId) -> usize {
    r.g.active().states.iter().position(|s| s.id == node).unwrap()
}

#[test]
fn moving_a_laned_node_into_a_subgraph_keeps_the_wide_wires_it_cuts() {
    let mut r = Rig::new();
    let (merge, schmitt, split) = chain(&mut r);
    assert_eq!(lane_values(&r, split, 8), PATTERN.to_vec());

    r.g.selected_nodes = vec![index_of(&r, schmitt)];
    r.g.move_selection_to_subgraph();
    r.pump();
    r.tick(80);

    let sub = r.g.levels[0].nodes.iter().find(|n| n.type_name() == "Subgraph").unwrap().node_id();
    assert_eq!(r.g.levels[0].connections.len(), 8 + 2, "the wide wire in and the wide wire out are still there");
    assert!(r.g.levels[0].connections.iter().any(|c| c.from.node == merge && c.to.node == sub));
    assert_eq!(r.outputs_of(0, sub), vec![8]);
    assert_eq!(r.width(r.level(sub), schmitt), 8);
    assert_eq!(lane_values(&r, split, 8), PATTERN.to_vec());
}

#[test]
fn unpacking_a_subgraph_keeps_the_wide_wires_through_it() {
    let mut r = Rig::new();
    let (_, schmitt, split) = chain(&mut r);
    r.g.selected_nodes = vec![index_of(&r, schmitt)];
    r.g.move_selection_to_subgraph();
    r.pump();
    r.tick(40);
    let sub = r.g.levels[0].nodes.iter().find(|n| n.type_name() == "Subgraph").unwrap().node_id();

    let at = index_of(&r, sub);
    r.g.unpack_subgraph(at);
    r.pump();
    r.tick(80);

    assert!(r.g.levels[0].nodes.iter().all(|n| n.type_name() != "Subgraph"));
    assert_eq!(r.g.levels[0].connections.len(), 8 + 2);
    assert_eq!(r.width(0, schmitt), 8);
    assert_eq!(lane_values(&r, split, 8), PATTERN.to_vec());
}
