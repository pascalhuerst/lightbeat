//! The Desk placement of a node inside a linked subgraph belongs to the
//! instance (`linked.rs`), so a sync neither copies it nor reads it as an edit.

use super::*;
use crate::engine::types::{DeskItem, DeskRect, new_shared_state};
use crate::project::{self, ProjectFile};
use crate::widgets::nodes::math::constant::ConstantWidget;

fn registry() -> NodeGraph {
    let mut g = NodeGraph::new();
    g.register_node("Route", "Subgraph", |id| Box::new(SubgraphWidget::new(id, new_shared_state(12, 12))));
    g.register_node("Signal", "Const Value", |id| {
        Box::new(ConstantWidget::new(id, PortType::Untyped, new_shared_state(0, 1)))
    });
    g
}

/// Two instances (1 and its copy) of a subgraph with two Const Values inside.
fn with_two_instances() -> NodeGraph {
    let mut g = registry();
    let json = r#"{
        "nodes": [{
            "type_name": "Subgraph", "id": 1, "pos": [0.0, 0.0], "params": [],
            "data": {"name": "Voice", "inputs": [], "outputs": []},
            "inner_graph": {
                "nodes": [
                    {"type_name": "Const Value", "id": 2, "pos": [10.0, 0.0], "params": []},
                    {"type_name": "Const Value", "id": 3, "pos": [10.0, 50.0], "params": []}
                ],
                "connections": []
            }
        }],
        "connections": []
    }"#;
    project::load_graph(&mut g, &serde_json::from_str(json).unwrap());
    g.selected_nodes = vec![0];
    g.duplicate_selected();
    g
}

fn root_ids(g: &NodeGraph) -> Vec<u64> {
    g.root_level().nodes.iter().map(|n| n.node_id().0).collect()
}

fn inner_ids(g: &NodeGraph, sub: u64) -> Vec<u64> {
    g.find_level_for_subgraph(NodeId(sub)).unwrap().nodes.iter()
        .map(|n| n.node_id().0)
        .filter(|&id| id < BRIDGE_IN_NODE_ID.0)
        .collect()
}

fn put_on_desk(g: &NodeGraph, sub: u64, id: u64, page: u64) {
    let level = g.find_level_for_subgraph(NodeId(sub)).unwrap();
    let node = level.nodes.iter().find(|n| n.node_id() == NodeId(id)).unwrap();
    let rect = DeskRect { page, pos: [20.0, 40.0], size: [100.0, 60.0] };
    node.shared_state().lock().desk = Some(DeskItem { node: Some(rect), snapshots: None, params: vec![] });
}

fn desk_page(node: &project::SavedNode) -> Option<u64> {
    node.desk.as_ref()?.node.as_ref().map(|r| r.page)
}

fn add_const_inside(g: &mut NodeGraph, sub: u64) {
    let level = g.level_of_subgraph(NodeId(sub)).unwrap();
    g.active_level = level;
    let id = g.alloc_id();
    let node = g.create_from_registry("Const Value", id).unwrap();
    g.add_node(node, Pos2::new(5.0, 5.0));
    g.active_level = 0;
}

/// What the app does after an edit: plan and apply until it settles.
fn settle(g: &mut NodeGraph) -> ProjectFile {
    g.assign_def_nodes();
    let mut current = project::save_graph(g);
    let names = g.known();
    for _ in 0..8 {
        let steps = crate::linked::plan_sync(&current, &crate::linked::first_instances(&current), &names, &mut || g.alloc_id().0);
        if steps.is_empty() {
            break;
        }
        for s in &steps {
            g.apply_sync_step(s);
        }
        crate::linked::apply_steps(&mut current, &steps);
    }
    current
}

#[test]
fn the_desk_placement_stays_with_the_instance_through_a_sync() {
    let mut g = with_two_instances();
    let copy = root_ids(&g)[1];
    let copy_first = inner_ids(&g, copy)[0];
    // The same node of each instance sits on a different page.
    put_on_desk(&g, 1, 2, 1);
    put_on_desk(&g, copy, copy_first, 2);
    add_const_inside(&mut g, 1);
    let settled = settle(&mut g);
    let inner = |id: u64| settled.nodes.iter().find(|n| n.id == id).unwrap().inner_graph.as_ref().unwrap();
    let first = inner(1).nodes.iter().find(|n| n.def_node == Some(2)).unwrap();
    let second = inner(copy).nodes.iter().find(|n| n.def_node == Some(2)).unwrap();
    assert_eq!((desk_page(first), desk_page(second)), (Some(1), Some(2)));
    // The node that arrived in the copy is not on its Desk.
    assert_eq!(inner(copy).nodes.len(), 3);
    assert!(inner(copy).nodes.iter().filter(|n| n.def_node != Some(2)).all(|n| n.desk.is_none()));
}

#[test]
fn putting_a_node_on_the_desk_is_no_edit_of_the_definition() {
    let g = with_two_instances();
    let copy = root_ids(&g)[1];
    let copy_first = inner_ids(&g, copy)[0];
    let known = g.known();
    let plan = |p: &ProjectFile| crate::linked::plan_sync(p, &crate::linked::first_instances(p), &known, &mut || 9999);
    assert!(plan(&project::save_graph(&g)).is_empty());
    put_on_desk(&g, 1, 2, 1);
    assert!(plan(&project::save_graph(&g)).is_empty(), "one instance's flag is not copied to the other");
    put_on_desk(&g, copy, copy_first, 2);
    assert!(plan(&project::save_graph(&g)).is_empty(), "and two different ones are not a difference");
    // The instance being edited as the source, as when the user has it open.
    let saved = project::save_graph(&g);
    assert!(crate::linked::plan_sync(&saved, &[vec![copy]], &known, &mut || 9999).is_empty());
}

#[test]
fn a_duplicated_instance_is_not_on_the_desk() {
    let mut g = with_two_instances();
    put_on_desk(&g, 1, 2, 1);
    g.selected_nodes = vec![0];
    g.duplicate_selected();
    let copy = root_ids(&g)[2];
    let saved = project::save_graph(&g);
    let inner = |id: u64| saved.nodes.iter().find(|n| n.id == id).unwrap().inner_graph.as_ref().unwrap();
    assert_eq!(inner(1).nodes.iter().filter(|n| n.desk.is_some()).count(), 1, "the original stays");
    assert!(inner(copy).nodes.iter().all(|n| n.desk.is_none()));
}
