//! Tests of the graph's side of linked subgraphs (`instances.rs`), without
//! an engine: the widgets hold what the engine would have mirrored.

use super::*;
use crate::engine::types::{PanelItem, new_shared_state};
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

/// One plain subgraph (id 1) with two Const Values (2, 3) inside.
fn with_plain_subgraph() -> NodeGraph {
    let mut g = registry();
    let json = r#"{
        "nodes": [{
            "type_name": "Subgraph", "id": 1, "pos": [0.0, 0.0], "params": [],
            "data": {"name": "Strobe", "inputs": [], "outputs": []},
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

fn keys(g: &NodeGraph, sub: u64) -> Vec<Option<u64>> {
    g.find_level_for_subgraph(NodeId(sub)).unwrap().nodes.iter()
        .filter(|n| n.node_id().0 < BRIDGE_IN_NODE_ID.0)
        .map(|n| n.shared_state().lock().def_node)
        .collect()
}

fn def(g: &NodeGraph) -> u64 {
    g.definition_at(0, 0).expect("the first node is linked")
}

fn with_two_instances() -> NodeGraph {
    let mut g = with_plain_subgraph();
    g.selected_nodes = vec![0];
    g.duplicate_selected();
    g
}

/// What the app does after an edit: identities, then plan and apply until it settles.
fn sync(g: &mut NodeGraph, _baseline: &ProjectFile) -> ProjectFile {
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

fn add_const_inside(g: &mut NodeGraph, sub: u64) {
    let level = g.level_of_subgraph(NodeId(sub)).unwrap();
    g.active_level = level;
    let id = g.alloc_id();
    let node = g.create_from_registry("Const Value", id).unwrap();
    g.add_node(node, Pos2::new(5.0, 5.0));
    g.active_level = 0;
}

#[test]
fn duplicating_a_plain_subgraph_makes_two_instances_of_a_new_definition() {
    let g = with_two_instances();
    assert_eq!(root_ids(&g).len(), 2);
    assert!(g.definition_at(0, 0).is_some());
    assert_eq!(g.definition_at(0, 1), g.definition_at(0, 0));
    assert_eq!(g.instance_counts()[&def(&g)], 2);
    let copy = root_ids(&g)[1];
    assert_ne!(copy, 1);
    let (a, b) = (inner_ids(&g, 1), inner_ids(&g, copy));
    assert_eq!(a.len(), 2);
    assert!(a.iter().all(|id| !b.contains(id)), "the copy has ids of its own");
    assert_eq!(keys(&g, 1), vec![Some(2), Some(3)], "the first instance's nodes stand for themselves");
    assert_eq!(keys(&g, copy), keys(&g, 1), "the copy's nodes have the same identities");
    let name = |i: usize| g.root_level().nodes[i].display_title().to_string();
    assert_eq!((name(0), name(1)), ("Strobe".to_string(), "Strobe".to_string()), "no renaming");
}

#[test]
fn a_unique_duplicate_is_plain() {
    let mut g = with_plain_subgraph();
    g.selected_nodes = vec![0];
    g.duplicate_selected_unique();
    assert_eq!(root_ids(&g).len(), 2);
    assert_eq!(g.definition_at(0, 0), None);
    assert_eq!(g.definition_at(0, 1), None);
    assert!(g.instance_counts().is_empty());
}

#[test]
fn duplicating_an_instance_adds_another_and_a_unique_copy_of_it_leaves_the_definition() {
    let mut g = with_two_instances();
    g.selected_nodes = vec![1];
    g.duplicate_selected();
    assert_eq!(g.instance_counts()[&def(&g)], 3);
    g.selected_nodes = vec![0];
    g.duplicate_selected_unique();
    assert_eq!(g.instance_counts()[&def(&g)], 3);
    assert_eq!(root_ids(&g).len(), 4);
    assert_eq!(g.definition_at(0, 3), None);
}

#[test]
fn the_tags_survive_a_save_and_a_load() {
    let g = with_two_instances();
    let json = serde_json::to_string(&project::save_graph(&g)).unwrap();
    assert!(json.contains(&format!("\"definition\":{}", def(&g))));
    assert!(json.contains("\"def_node\":2"));
    let mut g2 = registry();
    project::load_graph(&mut g2, &serde_json::from_str(&json).unwrap());
    assert_eq!(g2.instance_counts()[&def(&g)], 2);
    assert!(project::save_graph(&g2) == project::save_graph(&g));
}

#[test]
fn old_projects_load_as_plain_subgraphs() {
    let g = with_plain_subgraph();
    assert!(g.instance_counts().is_empty());
    let json = serde_json::to_string(&project::save_graph(&g)).unwrap();
    assert!(!json.contains("definition"));
    assert!(!json.contains("def_node"));
}

#[test]
fn a_definition_is_not_pasted_into_itself() {
    let mut g = with_two_instances();
    g.selected_nodes = vec![1];
    g.copy_selected();
    g.navigate_into_by_index(0);
    let before = g.active().nodes.len();
    g.paste(Pos2::ZERO);
    assert_eq!(g.active().nodes.len(), before, "nothing was pasted");
    assert_eq!(g.take_notices().len(), 1);
}

#[test]
fn a_wrapper_holding_an_instance_is_refused_inside_that_definition_too() {
    let mut g = with_two_instances();
    // A plain wrapper (id 9) around an instance of definition 1.
    let wrapper = project::SavedNode {
        type_name: "Subgraph".into(),
        id: 9,
        pos: [0.0, 0.0],
        size: None,
        params: vec![],
        data: Some(serde_json::json!({"name": "W", "inputs": [], "outputs": []})),
        inner_graph: Some(project::save_graph(&g)),
        disabled: false,
        promoted: vec![],
        hide_unused_ports: false,
        panel: None,
        desk: None,
        def_node: None,
        snapshots: Default::default(),
    };
    let mut inside = std::collections::HashSet::new();
    crate::linked::definitions_in(wrapper.data.as_ref(), wrapper.inner_graph.as_ref(), &mut inside);
    g.navigate_into_by_index(0);
    assert!(g.paste_would_nest(&inside));
    g.navigate_to_level(0);
    assert!(!g.paste_would_nest(&inside));
}

#[test]
fn a_pasted_instance_without_its_definition_in_the_project_is_plain() {
    let mut g = with_two_instances();
    g.selected_nodes = vec![0];
    g.copy_selected();
    let mut other = registry();
    other.clipboard = std::mem::take(&mut g.clipboard);
    other.clipboard_connections = std::mem::take(&mut g.clipboard_connections);
    other.paste(Pos2::ZERO);
    assert_eq!(other.root_level().nodes.len(), 1);
    assert_eq!(other.definition_at(0, 0), None);
    // Pasting it into the project it came from makes another instance.
    g.clipboard = std::mem::take(&mut other.clipboard);
    g.paste(Pos2::ZERO);
    assert_eq!(g.instance_counts()[&def(&g)], 3);
}

#[test]
fn making_unique_detaches_and_the_last_other_one_is_plain_again() {
    let mut g = with_two_instances();
    g.make_unique(NodeId(1));
    assert!(g.instance_counts().is_empty());
    assert_eq!(g.definition_at(0, 0), None);
    assert_eq!(g.definition_at(0, 1), None);
}

#[test]
fn a_changed_value_reaches_the_other_instance_in_place() {
    let mut g = with_two_instances();
    let baseline = project::save_graph(&g);
    let copy = root_ids(&g)[1];
    let copy_inner = inner_ids(&g, copy);
    let level = g.level_of_subgraph(NodeId(1)).unwrap();
    g.levels[level].states.iter_mut().find(|s| s.id == NodeId(2)).unwrap().pos = Pos2::new(222.0, 5.0);
    let settled = sync(&mut g, &baseline);
    assert_eq!(inner_ids(&g, copy), copy_inner, "the ids stay");
    let copy_level = g.find_level_for_subgraph(NodeId(copy)).unwrap();
    let moved = copy_level.states.iter().find(|s| s.id == NodeId(copy_inner[0])).unwrap();
    assert_eq!(moved.pos, Pos2::new(222.0, 5.0));
    assert!(project::save_graph(&g) == settled, "the saved form is what the sync computed");
}

#[test]
fn a_node_added_in_one_instance_is_built_in_the_other_and_one_undo_state_holds_both() {
    let mut g = with_two_instances();
    let baseline = project::save_graph(&g);
    let copy = root_ids(&g)[1];
    add_const_inside(&mut g, 1);
    let settled = sync(&mut g, &baseline);
    assert_eq!(inner_ids(&g, copy).len(), 3);
    assert_eq!(inner_ids(&g, 1).len(), 3);
    assert!(project::save_graph(&g) == settled);
    // Nothing is left to do: the next change check finds nothing, and
    // restoring that state (an undo) does not start another sync.
    let again = crate::linked::plan_sync(&settled, &crate::linked::first_instances(&settled), &g.known(), &mut || 9999);
    assert!(again.is_empty());
    assert_eq!(keys(&g, 1), keys(&g, copy), "the new node has the same identity in both");
}

#[test]
fn a_removed_node_goes_from_every_instance() {
    let mut g = with_two_instances();
    let baseline = project::save_graph(&g);
    let copy = root_ids(&g)[1];
    let level = g.level_of_subgraph(NodeId(1)).unwrap();
    g.active_level = level;
    let at = g.active().nodes.iter().position(|n| n.node_id() == NodeId(3)).unwrap();
    g.selected_nodes = vec![at];
    g.delete_selected();
    g.active_level = 0;
    let settled = sync(&mut g, &baseline);
    assert_eq!(inner_ids(&g, copy).len(), 1);
    assert!(project::save_graph(&g) == settled);
}

#[test]
fn a_wire_added_in_one_instance_is_drawn_in_the_other() {
    let mut g = with_two_instances();
    let baseline = project::save_graph(&g);
    let copy = root_ids(&g)[1];
    let level = g.level_of_subgraph(NodeId(1)).unwrap();
    g.levels[level].connections.push(Connection {
        from: PortId { node: NodeId(2), index: 0, dir: PortDir::Output },
        to: PortId { node: BRIDGE_OUT_NODE_ID, index: 0, dir: PortDir::Input },
    });
    sync(&mut g, &baseline);
    let copy_level = g.find_level_for_subgraph(NodeId(copy)).unwrap();
    assert_eq!(copy_level.connections.len(), 1);
    let first_copy_node = inner_ids(&g, copy)[0];
    assert_eq!(copy_level.connections[0].from.node, NodeId(first_copy_node));
}

#[test]
fn front_panel_values_stay_with_the_instance() {
    use crate::engine::types::ParamDef;
    let mut g = with_two_instances();
    let copy = root_ids(&g)[1];
    let copy_first = inner_ids(&g, copy)[0];
    // Node 2 is on the panel in both; each has its own value.
    for (sub, id, v) in [(1u64, 2u64, 0.2f32), (copy, copy_first, 0.8f32)] {
        let level = g.find_level_for_subgraph(NodeId(sub)).unwrap();
        let node = level.nodes.iter().find(|n| n.node_id() == NodeId(id)).unwrap();
        let mut s = node.shared_state().lock();
        s.panel = Some(PanelItem { order: 0, whole: true, params: vec![] });
        s.current_params = vec![ParamDef::Float { name: "Value".into(), value: v, min: 0.0, max: 1.0, step: 0.01, unit: "" }];
    }
    let baseline = project::save_graph(&g);
    add_const_inside(&mut g, 1);
    let settled = sync(&mut g, &baseline);
    let copy_saved = settled.nodes.iter().find(|n| n.id == copy).unwrap().inner_graph.as_ref().unwrap();
    let node = copy_saved.nodes.iter().find(|n| n.def_node == Some(2)).unwrap();
    assert!(node.panel.as_ref().unwrap().whole, "that a node is on the panel follows the definition");
    match node.params[0].value {
        crate::project::SavedParamValue::Float(v) => assert!((v - 0.8).abs() < 1e-6, "the copy kept its own value, got {v}"),
        _ => panic!("a float"),
    }
}

#[test]
fn linking_identical_subgraphs_tags_them_and_counts() {
    let mut g = with_plain_subgraph();
    g.selected_nodes = vec![0];
    g.duplicate_selected_unique();
    let report = g.link_identical();
    assert_eq!((report.groups, report.instances), (1, 2));
    assert_eq!(g.instance_counts().values().copied().collect::<Vec<_>>(), vec![2]);
    assert_eq!(g.link_identical(), crate::linked::LinkReport::default());
}

#[test]
fn the_instances_are_listed_for_a_jump_and_counted_for_the_badge() {
    let mut g = with_two_instances();
    g.refresh_instance_counts();
    let sub = g.levels[0].nodes[0].as_any_mut().downcast_mut::<SubgraphWidget>().unwrap();
    assert_eq!(sub.instances(), 2);
    let peers = g.instance_peers_of(0, 0);
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].node_id, g.root_level().nodes[1].node_id());
    assert!(g.go_to_node(peers[0].node_id));
}

#[test]
fn a_macro_placed_again_finds_its_definition_by_path_and_hash() {
    let mut g = registry();
    let json = r#"{
        "nodes": [{
            "type_name": "Subgraph", "id": 1, "pos": [0.0, 0.0], "params": [],
            "data": {"name": "M", "inputs": [], "outputs": [], "locked": true,
                     "macro_path": "/lib/m.lbm", "macro_hash": "abc", "definition": 77},
            "inner_graph": {"nodes": [], "connections": []}
        }],
        "connections": []
    }"#;
    project::load_graph(&mut g, &serde_json::from_str(json).unwrap());
    assert_eq!(g.macro_definition("/lib/m.lbm", "abc"), Some(77));
    assert_eq!(g.macro_definition("/lib/m.lbm", "changed"), None, "a changed file is a new definition");
    assert_eq!(g.macro_definition("/lib/other.lbm", "abc"), None);
    g.selected_nodes = vec![0];
    assert_eq!(g.selected_macro(), Some((NodeId(1), "/lib/m.lbm".to_string(), Some("abc".to_string()))));
}

#[test]
fn embedding_a_macro_instance_unlocks_the_definition_and_forgets_the_file() {
    let mut g = registry();
    let json = r#"{
        "nodes": [
            {"type_name": "Subgraph", "id": 1, "pos": [0.0, 0.0], "params": [],
             "data": {"name": "M", "inputs": [], "outputs": [], "locked": true, "macro_path": "/lib/m.lbm", "macro_hash": "abc", "definition": 77},
             "inner_graph": {"nodes": [{"type_name": "Const Value", "id": 5, "pos": [0.0, 0.0], "params": [], "def_node": 5}], "connections": []}},
            {"type_name": "Subgraph", "id": 2, "pos": [0.0, 100.0], "params": [],
             "data": {"name": "M", "inputs": [], "outputs": [], "locked": true, "macro_path": "/lib/m.lbm", "macro_hash": "abc", "definition": 77},
             "inner_graph": {"nodes": [{"type_name": "Const Value", "id": 6, "pos": [0.0, 0.0], "params": [], "def_node": 5}], "connections": []}}
        ],
        "connections": []
    }"#;
    project::load_graph(&mut g, &serde_json::from_str(json).unwrap());
    let baseline = project::save_graph(&g);
    {
        let w = g.levels[0].nodes[0].as_any_mut().downcast_mut::<SubgraphWidget>().unwrap();
        w.locked = false;
        w.macro_path.clear();
        w.macro_hash = None;
        w.push_config();
    }
    let settled = sync(&mut g, &baseline);
    let data = |i: usize| settled.nodes[i].data.clone().unwrap();
    assert_eq!(data(1)["locked"], false, "the other instance is unlocked too");
    assert_eq!(data(1)["macro_path"], "");
    assert!(data(1).get("macro_hash").is_none());
    assert_eq!(data(1)["definition"], 77, "they stay linked");
}

#[test]
fn only_the_open_instance_or_the_one_enclosing_it_can_be_the_source() {
    let mut g = with_two_instances();
    assert_eq!(g.editing_path(), None, "the root is inside nothing");
    g.navigate_into_by_index(0);
    assert_eq!(g.editing_path(), Some(vec![1]));
    // A plain subgraph inside the instance: still the instance.
    let outer = g.level_of_subgraph(NodeId(1)).unwrap();
    let plain = g.alloc_id();
    let node = g.create_from_registry("Subgraph", plain).unwrap();
    let at = g.add_node(node, Pos2::ZERO);
    g.navigate_into_by_index(at);
    assert_eq!(g.editing_path(), Some(vec![1]));
    g.navigate_to_level(0);
    // A plain subgraph at the root encloses no instance.
    let lone = g.alloc_id();
    let node = g.create_from_registry("Subgraph", lone).unwrap();
    let at = g.add_node(node, Pos2::ZERO);
    g.navigate_into_by_index(at);
    assert_eq!(g.editing_path(), None);
    g.navigate_to_level(0);
    g.selected_nodes = vec![1];
    assert_eq!(g.selected_instance_path(), Some(vec![root_ids(&g)[1]]));
    g.selected_nodes = vec![at];
    assert_eq!(g.selected_instance_path(), None, "a plain subgraph has no instances");
    let _ = outer;
}

#[test]
fn the_banner_knows_the_open_definition() {
    let mut g = with_two_instances();
    assert!(g.open_definition().is_none());
    g.navigate_into_by_index(0);
    assert_eq!(g.open_definition(), Some(("Strobe".to_string(), 2)));
}

#[test]
fn a_rename_follows_the_definition() {
    let mut g = with_two_instances();
    let definition = def(&g);
    let baseline = project::save_graph(&g);
    {
        let w = g.levels[0].nodes[0].as_any_mut().downcast_mut::<SubgraphWidget>().unwrap();
        w.restore_from_save(Some(&serde_json::json!({"name": "Renamed", "inputs": [], "outputs": [], "definition": definition})), &[]);
    }
    sync(&mut g, &baseline);
    assert_eq!(g.root_level().nodes[1].display_title(), "Renamed");
}

#[test]
fn an_instance_inside_a_subgraph_inside_another_instance_follows_its_own_definition() {
    let mut g = with_two_instances();
    let outer = g.level_of_subgraph(NodeId(1)).unwrap();
    g.active_level = outer;
    let nested = g.alloc_id();
    let node = g.create_from_registry("Subgraph", nested).unwrap();
    let at = g.add_node(node, Pos2::ZERO);
    g.navigate_into_by_index(at);
    add_const_inside(&mut g, nested.0);
    // Ctrl+D inside the outer instance: two instances of a second definition.
    g.active_level = outer;
    g.selected_nodes = vec![at];
    g.duplicate_selected();
    let nested_def = g.definition_at(outer, at).expect("the nested subgraph is linked now");
    g.active_level = 0;
    let base = project::save_graph(&g);
    sync(&mut g, &base);
    let nested_instances = |g: &NodeGraph| -> Vec<u64> {
        let mut out = Vec::new();
        for li in g.reachable_levels() {
            for i in 0..g.levels[li].nodes.len() {
                if g.definition_at(li, i) == Some(nested_def) {
                    out.push(g.levels[li].nodes[i].node_id().0);
                }
            }
        }
        out
    };
    let all = nested_instances(&g);
    assert_eq!(all.len(), 4, "the other outer instance got both nested ones");

    // Editing the deepest level of one reaches all four, and the outer pair stays as it is.
    let base = project::save_graph(&g);
    add_const_inside(&mut g, nested.0);
    sync(&mut g, &base);
    for id in all {
        assert_eq!(inner_ids(&g, id).len(), 2, "instance {id} has the added node");
    }
    assert_eq!(g.instance_counts()[&def(&g)], 2);
}

#[test]
fn a_pasted_instance_starts_from_the_definition_as_it_is_now() {
    let mut g = with_two_instances();
    g.selected_nodes = vec![1];
    g.copy_selected();
    // The definition grows after the copy.
    let base = project::save_graph(&g);
    add_const_inside(&mut g, 1);
    sync(&mut g, &base);
    g.paste(Pos2::ZERO);
    let pasted = *root_ids(&g).last().unwrap();
    assert_eq!(root_ids(&g).len(), 3);
    assert_eq!(inner_ids(&g, pasted).len(), 3, "the added node is in the pasted instance");
    assert_eq!(g.instance_counts()[&def(&g)], 3);
}

#[test]
fn rebuilding_a_sibling_keeps_the_selection_and_the_drag_of_the_level_the_user_is_on() {
    let mut g = with_two_instances();
    // The definition contains a nested subgraph, which the rebuild has to navigate into.
    let outer = g.level_of_subgraph(NodeId(1)).unwrap();
    g.active_level = outer;
    let nested = g.alloc_id();
    let node = g.create_from_registry("Subgraph", nested).unwrap();
    let at = g.add_node(node, Pos2::ZERO);
    g.navigate_into_by_index(at);
    add_const_inside(&mut g, nested.0);
    g.active_level = 0;
    let base = project::save_graph(&g);
    sync(&mut g, &base);
    // The user is on the root with the first instance selected and a drag going.
    let base = project::save_graph(&g);
    add_const_inside(&mut g, 1);
    g.selected_nodes = vec![0];
    g.drag.dragging_nodes = true;
    sync(&mut g, &base);
    assert_eq!(inner_ids(&g, root_ids(&g)[1]).len(), 4, "the sibling was rebuilt with the new node");
    assert_eq!(g.selected_nodes, vec![0]);
    assert!(g.drag.dragging_nodes);
    assert_eq!(g.active_level, 0);
}

fn input_ports(n: usize) -> Vec<serde_json::Value> {
    (0..n).map(|i| serde_json::json!({"name": format!("in {i}"), "port_type_idx": 2})).collect()
}

/// What the app does after an edit, with the port removals carried along.
fn sync_with_removals(g: &mut NodeGraph) {
    g.assign_def_nodes();
    let mut current = project::save_graph(g);
    let names = g.known();
    for _ in 0..8 {
        let mut steps = crate::linked::plan_sync(&current, &crate::linked::first_instances(&current), &names, &mut || g.alloc_id().0);
        if steps.is_empty() {
            break;
        }
        g.attach_port_removals(&mut steps);
        for s in &steps {
            g.apply_sync_step(s);
        }
        crate::linked::apply_steps(&mut current, &steps);
    }
}

#[test]
fn removing_a_port_shifts_the_outer_wires_of_every_instance() {
    let mut g = with_two_instances();
    let definition = def(&g);
    let (a, b) = (g.root_level().nodes[0].node_id(), g.root_level().nodes[1].node_id());
    for w in g.levels[0].nodes.iter_mut().filter_map(|n| n.as_any_mut().downcast_mut::<SubgraphWidget>()) {
        w.restore_from_save(Some(&serde_json::json!({"name": "Strobe", "inputs": input_ports(3), "outputs": [], "definition": definition})), &[]);
    }
    let consts: Vec<NodeId> = (0..6)
        .map(|_| {
            let id = g.alloc_id();
            let node = g.create_from_registry("Const Value", id).unwrap();
            g.add_node(node, Pos2::ZERO);
            id
        })
        .collect();
    for (k, (&to_a, &to_b)) in consts[..3].iter().zip(&consts[3..]).enumerate() {
        g.add_connection(PortId { node: to_a, index: 0, dir: PortDir::Output }, PortId { node: a, index: k, dir: PortDir::Input });
        g.add_connection(PortId { node: to_b, index: 0, dir: PortDir::Output }, PortId { node: b, index: k, dir: PortDir::Input });
    }
    g.assign_def_nodes();
    // The user removes port 0 of the first instance.
    g.levels[0].nodes[0].as_any_mut().downcast_mut::<SubgraphWidget>().unwrap().remove_input(0);
    g.remap_removed_ports();
    sync_with_removals(&mut g);
    let wires_to = |g: &NodeGraph, sub: NodeId| -> Vec<(u64, usize)> {
        let mut w: Vec<(u64, usize)> = g.root_level().connections.iter().filter(|c| c.to.node == sub).map(|c| (c.from.node.0, c.to.index)).collect();
        w.sort_unstable();
        w
    };
    assert_eq!(wires_to(&g, a), vec![(consts[1].0, 0), (consts[2].0, 1)]);
    assert_eq!(wires_to(&g, b), vec![(consts[4].0, 0), (consts[5].0, 1)], "the sibling's wires moved the same way");
}
