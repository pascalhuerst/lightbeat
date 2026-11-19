#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::engine::types::{DeskItem, DeskRect, new_shared_state};
    use crate::widgets::nodes::desk::{DeskDoc, DeskPage};
    use crate::widgets::nodes::math::constant::ConstantWidget;
    use crate::widgets::nodes::meta::subgraph::SubgraphWidget;

    fn registry() -> NodeGraph {
        let mut g = NodeGraph::new();
        g.register_node("Route", "Subgraph", |id| Box::new(SubgraphWidget::new(id, new_shared_state(12, 12))));
        g.register_node("Signal", "Const Value", |id| {
            Box::new(ConstantWidget::new(id, PortType::Untyped, new_shared_state(0, 1)))
        });
        g
    }

    /// A Const Value (1) on page 7, a Subgraph (2) holding a Const Value (3)
    /// on page 8 and one (4) that is not on the Desk.
    const PROJECT: &str = r#"{
        "nodes": [
            {"type_name": "Const Value", "id": 1, "pos": [0.0, 0.0], "params": [],
             "desk": {"node": {"page": 7, "pos": [20.0, 40.0], "size": [100.0, 60.0]}}},
            {"type_name": "Subgraph", "id": 2, "pos": [0.0, 100.0], "params": [],
             "data": {"name": "S", "inputs": [], "outputs": []},
             "inner_graph": {
                "nodes": [
                    {"type_name": "Const Value", "id": 3, "pos": [0.0, 0.0], "params": [],
                     "desk": {"node": {"page": 8, "pos": [0.0, 0.0], "size": [80.0, 40.0]},
                              "snapshots": {"page": 8, "pos": [80.0, 0.0], "size": [120.0, 40.0]}}},
                    {"type_name": "Const Value", "id": 4, "pos": [0.0, 50.0], "params": []}
                ],
                "connections": []
             }}
        ],
        "connections": [],
        "desk": {"pages": [{"id": 7, "name": "Main"}, {"id": 8, "name": "Strobe"}], "locked": true, "issued": 8}
    }"#;

    fn loaded() -> NodeGraph {
        let mut g = registry();
        crate::project::load_graph(&mut g, &serde_json::from_str(PROJECT).unwrap());
        g
    }

    fn desk_of(g: &NodeGraph, level: usize, id: u64) -> Option<DeskItem> {
        let l = &g.levels[level];
        l.nodes.iter().find(|n| n.node_id() == NodeId(id)).unwrap().shared_state().lock().desk.clone()
    }

    fn inner_level(g: &NodeGraph) -> usize {
        g.levels.iter().position(|l| l.subgraph_id == Some(NodeId(2))).unwrap()
    }

    #[test]
    fn placements_and_pages_survive_a_save_and_a_load() {
        let g = loaded();
        let inner = inner_level(&g);
        assert_eq!(desk_of(&g, 0, 1).unwrap().node.unwrap().page, 7);
        let deep = desk_of(&g, inner, 3).unwrap();
        assert_eq!(deep.node.unwrap().size, [80.0, 40.0]);
        assert_eq!(deep.snapshots.unwrap().pos, [80.0, 0.0]);
        assert!(desk_of(&g, inner, 4).is_none());
        assert_eq!(g.desk().pages.len(), 2);
        assert!(g.desk().locked);

        let json = serde_json::to_string(&crate::project::save_graph(&g)).unwrap();
        let mut again = registry();
        crate::project::load_graph(&mut again, &serde_json::from_str(&json).unwrap());
        assert_eq!(desk_of(&again, 0, 1), desk_of(&g, 0, 1));
        assert_eq!(desk_of(&again, inner_level(&again), 3), desk_of(&g, inner, 3));
        assert_eq!(again.desk(), g.desk());
        // Only what is on the Desk writes anything.
        assert_eq!(json.matches("\"desk\":{\"node\"").count(), 2);
    }

    #[test]
    fn a_project_without_a_desk_loads_and_writes_nothing_about_it() {
        let mut g = registry();
        let old = r#"{"nodes": [{"type_name": "Const Value", "id": 1, "pos": [0.0, 0.0], "params": []}], "connections": []}"#;
        crate::project::load_graph(&mut g, &serde_json::from_str(old).unwrap());
        assert!(desk_of(&g, 0, 1).is_none());
        assert!(g.desk().is_default());
        let json = serde_json::to_string(&crate::project::save_graph(&g)).unwrap();
        assert!(!json.contains("desk"), "{json}");
    }

    #[test]
    fn desk_edits_change_the_saved_form_so_the_project_undo_sees_them() {
        let g = loaded();
        let before = crate::project::save_graph(&g);
        let moved = DeskRect { page: 7, pos: [60.0, 40.0], size: [100.0, 60.0] };
        g.levels[0].nodes[0].shared_state().lock().desk = Some(DeskItem { node: Some(moved), snapshots: None });
        assert!(before != crate::project::save_graph(&g), "a move is a project edit");
        let mut g2 = loaded();
        let mut renamed = g2.desk().clone();
        renamed.pages[0].name = "Other".into();
        g2.set_desk(renamed);
        assert!(before != crate::project::save_graph(&g2), "so is a page rename");
        let mut g3 = loaded();
        let mut unlocked = g3.desk().clone();
        unlocked.locked = false;
        g3.set_desk(unlocked);
        assert!(before != crate::project::save_graph(&g3), "and the lock");

        // Undo applies the earlier saved form to a fresh graph, as `apply_project` does.
        let mut restored = registry();
        crate::project::load_graph(&mut restored, &before);
        assert!(crate::project::save_graph(&restored) == before);
        assert_eq!(desk_of(&restored, 0, 1).unwrap().node.unwrap().pos, [20.0, 40.0]);
    }

    #[test]
    fn a_copy_is_not_on_the_desk() {
        let mut g = loaded();
        g.selected_nodes = vec![0, 1];
        g.duplicate_selected();
        assert_eq!(g.levels[0].nodes.len(), 4);
        let copies: Vec<NodeId> = g.levels[0].nodes[2..].iter().map(|n| n.node_id()).collect();
        for id in &copies {
            assert!(desk_of(&g, 0, id.0).is_none(), "{id:?} was copied onto the Desk");
        }
        // Nor is anything inside the copied subgraph.
        let copy_sub = copies
            .into_iter()
            .find(|id| g.levels[0].nodes.iter().any(|n| n.node_id() == *id && n.type_name() == "Subgraph"))
            .unwrap();
        let inside = g.levels.iter().find(|l| l.subgraph_id == Some(copy_sub)).unwrap();
        assert!(inside.nodes.iter().all(|n| n.shared_state().lock().desk.is_none()));
        // The originals keep theirs.
        assert!(desk_of(&g, 0, 1).is_some());
        assert!(desk_of(&g, inner_level(&g), 3).is_some());
    }

    #[test]
    fn a_pasted_node_is_not_on_the_desk() {
        let mut g = loaded();
        g.selected_nodes = vec![0];
        g.copy_selected();
        g.paste(Pos2::new(300.0, 300.0));
        let pasted = g.levels[0].nodes.last().unwrap().node_id();
        assert!(desk_of(&g, 0, pasted.0).is_none());
        assert!(desk_of(&g, 0, 1).is_some());
    }

    #[test]
    fn a_node_moved_into_a_subgraph_stays_on_the_desk() {
        let mut g = loaded();
        g.selected_nodes = vec![0];
        g.move_selection_to_subgraph();
        let moved = g.levels.iter().flat_map(|l| l.nodes.iter()).find(|n| n.node_id() == NodeId(1)).unwrap();
        assert!(moved.shared_state().lock().desk.is_some(), "it is the same node, only on another level");
    }

    #[test]
    fn a_macro_carries_no_placements() {
        let g = loaded();
        let mut inner = crate::project::save_level(&g.levels[inner_level(&g)], &g);
        assert!(inner.nodes.iter().any(|n| n.desk.is_some()));
        crate::project::strip_desk(&mut inner);
        assert!(inner.nodes.iter().all(|n| n.desk.is_none()));
    }

    #[test]
    fn pages_are_named_by_id() {
        let doc: DeskDoc = serde_json::from_str(r#"{"pages": [{"id": 3, "name": "A"}]}"#).unwrap();
        assert_eq!(doc.pages, vec![DeskPage { id: 3, name: "A".into() }]);
        assert!(!doc.locked);
    }
}
