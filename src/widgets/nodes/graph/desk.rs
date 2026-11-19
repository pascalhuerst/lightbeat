//! What the Desk keeps of the graph: which nodes are on it (from every level),
//! putting a node on it and taking it off, and giving a node that was just
//! put there a page, a place and a size. The drawing is in `desk_view.rs`.

use super::*;
use crate::engine::types::{DeskItem, DeskRect};
use crate::widgets::nodes::desk::{self as geometry, DeskDoc};

/// What of a node is on the Desk: the node itself, or its snapshot list as a
/// row of buttons. Each is its own item with its own page and place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeskKind {
    Node,
    Snapshots,
}

impl DeskKind {
    fn slot(self, item: &DeskItem) -> Option<&DeskRect> {
        match self {
            DeskKind::Node => item.node.as_ref(),
            DeskKind::Snapshots => item.snapshots.as_ref(),
        }
    }

    pub(super) fn slot_mut(self, item: &mut DeskItem) -> &mut Option<DeskRect> {
        match self {
            DeskKind::Node => &mut item.node,
            DeskKind::Snapshots => &mut item.snapshots,
        }
    }
}

/// One item on the Desk, as the Desk sees it this frame.
pub(super) struct DeskEntry {
    pub level: usize,
    pub index: usize,
    pub id: NodeId,
    pub kind: DeskKind,
    /// The page it is drawn on: the one it names, or the first when that is gone.
    pub page: u64,
    pub rect: Rect,
}

/// The nodes on the Desk across `levels` (only the `reachable` ones: the
/// level of a deleted subgraph stays in the list). Nodes that were just put
/// there and wait for a place are not in it.
pub(super) fn collect_desk(levels: &[GraphLevel], reachable: &[usize], doc: &DeskDoc) -> Vec<DeskEntry> {
    let mut out = Vec::new();
    for &level in reachable {
        for (index, node) in levels[level].nodes.iter().enumerate() {
            let id = node.node_id();
            if [BRIDGE_IN_NODE_ID, BRIDGE_OUT_NODE_ID].contains(&id) {
                continue;
            }
            let shared = node.shared_state().lock();
            for kind in [DeskKind::Node, DeskKind::Snapshots] {
                let Some(r) = shared.desk.as_ref().and_then(|d| kind.slot(d)) else { continue };
                if r.is_unplaced() {
                    continue;
                }
                out.push(DeskEntry {
                    level,
                    index,
                    id,
                    kind,
                    page: doc.resolve(r.page).unwrap_or(0),
                    rect: geometry::rect_of(r),
                });
            }
        }
    }
    out
}

impl NodeGraph {
    /// The page the Desk shows: the one chosen, or the first.
    pub fn current_desk_page(&self) -> Option<u64> {
        self.desk_ui.page.and_then(|p| self.desk.resolve(p)).or_else(|| self.desk.pages.first().map(|p| p.id))
    }

    fn desk_entries(&self) -> Vec<DeskEntry> {
        collect_desk(&self.levels, &self.reachable_levels(), &self.desk)
    }

    /// How many nodes are on `page`.
    pub fn desk_items_on(&self, page: u64) -> usize {
        self.desk_entries().iter().filter(|e| e.page == page).count()
    }

    /// Every selected node (on the open level) is on the Desk.
    pub(super) fn selection_all_on_desk(&self) -> bool {
        let level = self.active();
        let mut any = false;
        for n in self.selected_nodes.iter().filter_map(|&i| level.nodes.get(i)) {
            any = true;
            if !n.shared_state().lock().desk.as_ref().is_some_and(|d| d.node.is_some()) {
                return false;
            }
        }
        any
    }

    /// Put the selected nodes of the open level on the Desk, or take them off.
    pub(super) fn set_selection_on_desk(&mut self, on: bool) {
        let level = self.active();
        for &i in &self.selected_nodes {
            let Some(node) = level.nodes.get(i) else { continue };
            if [BRIDGE_IN_NODE_ID, BRIDGE_OUT_NODE_ID].contains(&node.node_id()) {
                continue;
            }
            set_node_on_desk(node.as_ref(), on);
        }
    }

    /// Take one item (a node, or its snapshot row) off the Desk, wherever the
    /// node is. What else of the node is on the Desk stays.
    pub(super) fn remove_from_desk(&mut self, id: NodeId, kind: DeskKind) {
        for level in &self.levels {
            if let Some(node) = level.nodes.iter().find(|n| n.node_id() == id) {
                DeskItem::edit(&mut node.shared_state().lock().desk, |d| *kind.slot_mut(d) = None);
            }
        }
    }

    /// Give every node that was just put on the Desk (by a menu or an
    /// inspector checkbox, which know no geometry) its page, its place and
    /// its size: the current page, the first free spot, what the node asks for.
    pub(super) fn place_new_desk_items(&mut self) {
        let reachable = self.reachable_levels();
        let mut waiting: Vec<(usize, usize, DeskKind)> = Vec::new();
        for &li in &reachable {
            for (i, n) in self.levels[li].nodes.iter().enumerate() {
                let shared = n.shared_state().lock();
                for kind in [DeskKind::Node, DeskKind::Snapshots] {
                    if shared.desk.as_ref().and_then(|d| kind.slot(d)).is_some_and(DeskRect::is_unplaced) {
                        waiting.push((li, i, kind));
                    }
                }
            }
        }
        if waiting.is_empty() {
            return;
        }
        let first = self.desk.ensure_page();
        let page = self.current_desk_page().unwrap_or(first);
        let mut taken: Vec<Rect> = collect_desk(&self.levels, &reachable, &self.desk)
            .into_iter()
            .filter(|e| e.page == page)
            .map(|e| e.rect)
            .collect();
        for (li, i, kind) in waiting {
            let level = &self.levels[li];
            let node = level.nodes[i].as_ref();
            let size = match kind {
                DeskKind::Node => {
                    let cell = crate::widgets::nodes::panel::cell_size(&node.layout(), node_size(node, level.states[i].size_override));
                    geometry::default_size(cell, node.layout().title_h)
                }
                DeskKind::Snapshots => geometry::snapshot_row_size(node.shared_state().lock().snapshots.snapshots.len()),
            };
            let rect = Rect::from_min_size(geometry::free_spot(&taken, size), size);
            taken.push(rect);
            DeskItem::edit(&mut node.shared_state().lock().desk, |d| *kind.slot_mut(d) = Some(geometry::desk_rect(page, rect)));
        }
    }
}

fn set_node_on_desk(node: &dyn NodeWidget, on: bool) {
    let mut shared = node.shared_state().lock();
    let placed = shared.desk.as_ref().is_some_and(|d| d.node.is_some());
    if on == placed {
        return;
    }
    DeskItem::edit(&mut shared.desk, |d| d.node = on.then_some(DeskRect::UNPLACED));
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::{DeskKind, collect_desk};
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
        renamed.rename(7, "Other".into());
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

    fn entries(g: &NodeGraph) -> Vec<(u64, u64)> {
        let mut v: Vec<(u64, u64)> = collect_desk(&g.levels, &g.reachable_levels(), g.desk())
            .iter()
            .filter(|e| e.kind == DeskKind::Node)
            .map(|e| (e.id.0, e.page))
            .collect();
        v.sort_unstable();
        v
    }

    #[test]
    fn the_desk_collects_nodes_from_every_level() {
        let g = loaded();
        assert_eq!(entries(&g), vec![(1, 7), (3, 8)]);
    }

    #[test]
    fn the_level_of_a_deleted_subgraph_is_not_collected() {
        let mut g = loaded();
        g.levels[0].nodes.remove(1);
        g.levels[0].states.remove(1);
        assert_eq!(entries(&g), vec![(1, 7)]);
    }

    #[test]
    fn an_item_on_a_page_that_is_gone_is_drawn_on_the_first() {
        let g = loaded();
        let node = &g.levels[0].nodes[0];
        node.shared_state().lock().desk.as_mut().unwrap().node.as_mut().unwrap().page = 99;
        assert_eq!(entries(&g), vec![(1, 7), (3, 8)]);
        let first = g.desk().pages[0].id;
        assert_eq!(first, 7);
        node.shared_state().lock().desk.as_mut().unwrap().node.as_mut().unwrap().page = 8;
        assert_eq!(entries(&g), vec![(1, 8), (3, 8)]);
    }

    fn fresh() -> NodeGraph {
        let mut g = registry();
        let old = r#"{"nodes": [
            {"type_name": "Const Value", "id": 1, "pos": [0.0, 0.0], "params": []},
            {"type_name": "Const Value", "id": 2, "pos": [0.0, 50.0], "params": []}
        ], "connections": []}"#;
        crate::project::load_graph(&mut g, &serde_json::from_str(old).unwrap());
        g
    }

    #[test]
    fn a_node_put_on_the_desk_lands_on_the_current_page_at_a_free_spot() {
        let mut g = fresh();
        g.selected_nodes = vec![0, 1];
        assert!(!g.selection_all_on_desk());
        g.set_selection_on_desk(true);
        assert!(g.selection_all_on_desk());
        // Nothing is placed until the graph has had its frame.
        assert!(entries(&g).is_empty());
        g.place_new_desk_items();
        let page = g.desk().pages[0].id;
        assert_eq!(entries(&g).len(), 2);
        let rects: Vec<Rect> = collect_desk(&g.levels, &g.reachable_levels(), g.desk()).iter().map(|e| e.rect).collect();
        assert!(!crate::widgets::nodes::desk::overlaps(&rects[0], &rects[1]), "{rects:?}");
        assert!(rects.iter().all(|r| r.width() >= 60.0 && r.height() >= 40.0 && r.min.x >= 0.0));
        assert!(entries(&g).iter().all(|e| e.1 == page));

        // A second time changes nothing.
        let before = entries(&g);
        g.place_new_desk_items();
        assert_eq!(entries(&g), before);
    }

    #[test]
    fn the_page_shown_is_where_new_items_go() {
        let mut g = loaded();
        g.desk_ui.page = Some(8);
        let inner = inner_level(&g);
        g.active_level = inner;
        g.selected_nodes = vec![g.levels[inner].nodes.iter().position(|n| n.node_id() == NodeId(4)).unwrap()];
        g.set_selection_on_desk(true);
        g.active_level = 0;
        g.place_new_desk_items();
        assert_eq!(entries(&g), vec![(1, 7), (3, 8), (4, 8)]);
    }

    #[test]
    fn removing_from_the_desk_keeps_the_node_and_its_snapshot_row() {
        let mut g = loaded();
        let inner = inner_level(&g);
        g.remove_from_desk(NodeId(3), DeskKind::Node);
        let left = desk_of(&g, inner, 3).expect("the snapshot row is still on the Desk");
        assert!(left.node.is_none() && left.snapshots.is_some());
        g.remove_from_desk(NodeId(1), DeskKind::Node);
        assert!(desk_of(&g, 0, 1).is_none(), "an item with nothing left is dropped");
        assert_eq!(entries(&g), vec![]);
    }

    #[test]
    fn deleting_a_page_moves_its_items_to_the_first_page_without_overlap() {
        let mut g = loaded();
        let inner = inner_level(&g);
        // Both on page 7 and 8 at the same size, so a naive move would stack them.
        g.levels[0].nodes[0].shared_state().lock().desk.as_mut().unwrap().node.as_mut().unwrap().pos = [0.0, 0.0];
        let item = |g: &NodeGraph| {
            let n = &g.levels[inner].nodes.iter().find(|n| n.node_id() == NodeId(3)).unwrap();
            n.shared_state().lock().desk.as_mut().unwrap().node.as_mut().unwrap().pos = [0.0, 0.0];
        };
        item(&g);
        g.desk_ui.page = Some(8);
        g.delete_desk_page(8);
        assert_eq!(g.desk().pages.len(), 1);
        assert_eq!(entries(&g), vec![(1, 7), (3, 7)]);
        assert_eq!(g.desk_ui.page, Some(7), "the page shown is the neighbour");
        let rects: Vec<Rect> = collect_desk(&g.levels, &g.reachable_levels(), g.desk()).iter().map(|e| e.rect).collect();
        assert_eq!(rects.len(), 3, "the snapshot row of node 3 moved with it");
        for (a, b) in [(0, 1), (0, 2), (1, 2)] {
            assert!(!crate::widgets::nodes::desk::overlaps(&rects[a], &rects[b]), "{rects:?}");
        }
        assert_eq!(g.desk_items_on(7), 3);
    }

    /// One frame of the Desk with a real egui context, as the app runs it.
    fn frame(g: &mut NodeGraph) -> super::super::desk_view::DeskOutcome {
        let ctx = egui::Context::default();
        let mut outcome = None;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| outcome = Some(g.show_desk(ui)));
        });
        outcome.unwrap()
    }

    #[test]
    fn a_frame_of_the_desk_draws_every_level_and_puts_all_widgets_back() {
        let mut g = loaded();
        let counts: Vec<usize> = g.levels.iter().map(|l| l.nodes.len()).collect();
        let outcome = frame(&mut g);
        assert!(!outcome.to_patch);
        assert_eq!(g.levels.iter().map(|l| l.nodes.len()).collect::<Vec<_>>(), counts);
        // Items on a page that is not shown are not drawn, but nothing is lost.
        g.desk_ui.page = Some(8);
        frame(&mut g);
        assert_eq!(g.levels.iter().map(|l| l.nodes.len()).collect::<Vec<_>>(), counts);
    }

    #[test]
    fn a_frame_of_the_desk_places_a_new_item_and_keeps_the_graph_maintained() {
        let mut g = fresh();
        g.selected_nodes = vec![0];
        g.set_selection_on_desk(true);
        frame(&mut g);
        assert_eq!(entries(&g).len(), 1, "maintain ran in the Desk view");
        assert_eq!(g.desk().pages.len(), 1);
    }

    /// A pointer driving the Desk frame by frame, as a window would deliver it.
    struct Pointer {
        ctx: egui::Context,
        time: f64,
        at: Pos2,
    }

    impl Pointer {
        fn new() -> Self {
            Pointer { ctx: egui::Context::default(), time: 0.0, at: Pos2::new(1.0, 1.0) }
        }

        fn frame(&mut self, g: &mut NodeGraph, events: Vec<egui::Event>) {
            self.time += 0.05;
            let input = egui::RawInput { time: Some(self.time), events, ..Default::default() };
            let _ = self.ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    g.show_desk(ui);
                });
            });
        }

        fn go(&mut self, g: &mut NodeGraph, to: Pos2) {
            self.at = to;
            self.frame(g, vec![egui::Event::PointerMoved(to)]);
        }

        fn button(&mut self, g: &mut NodeGraph, button: egui::PointerButton, pressed: bool) {
            let event = egui::Event::PointerButton { pos: self.at, button, pressed, modifiers: egui::Modifiers::NONE };
            self.frame(g, vec![event]);
        }

        /// Press at `from`, drag by `by` in a few steps and let go.
        fn drag(&mut self, g: &mut NodeGraph, from: Pos2, by: Vec2) {
            self.go(g, from);
            self.button(g, egui::PointerButton::Primary, true);
            for step in 1..=4 {
                self.go(g, from + by * (step as f32 / 4.0));
            }
            self.button(g, egui::PointerButton::Primary, false);
        }

        fn right_click(&mut self, g: &mut NodeGraph, at: Pos2) {
            self.go(g, at);
            self.button(g, egui::PointerButton::Secondary, true);
            self.button(g, egui::PointerButton::Secondary, false);
        }
    }

    /// The root node 1 on page 7, drawn once so that its place on screen is known.
    fn drawn() -> (NodeGraph, Pointer, Rect, Rect) {
        let mut g = loaded();
        g.desk_ui.page = Some(7);
        let mut p = Pointer::new();
        p.frame(&mut g, vec![]);
        let (_, _, strip, item) = *g.desk_ui.shown.iter().find(|(id, kind, _, _)| *id == NodeId(1) && *kind == DeskKind::Node).expect("drawn");
        (g, p, strip, item)
    }

    fn rect_of_node_1(g: &NodeGraph) -> Rect {
        let r = g.levels[0].nodes[0].shared_state().lock().desk.clone().unwrap().node.unwrap();
        crate::widgets::nodes::desk::rect_of(&r)
    }

    #[test]
    fn dragging_the_title_moves_an_item_on_the_grid() {
        let (mut g, mut p, strip, _) = drawn();
        g.desk.locked = false;
        let before = rect_of_node_1(&g);
        p.drag(&mut g, strip.center(), Vec2::new(47.0, 33.0));
        let after = rect_of_node_1(&g);
        assert_eq!(after.size(), before.size(), "a move does not resize");
        assert_eq!(after.min, Pos2::new(60.0, 80.0), "from (20, 40) by (47, 33), snapped");
    }

    #[test]
    fn dragging_the_corner_resizes_an_item_on_the_grid() {
        let (mut g, mut p, _, item) = drawn();
        g.desk.locked = false;
        let before = rect_of_node_1(&g);
        p.drag(&mut g, item.max - Vec2::splat(4.0), Vec2::new(80.0, 60.0));
        let after = rect_of_node_1(&g);
        assert_eq!(after.min, before.min);
        assert!(after.width() > before.width() && after.height() > before.height(), "{before:?} -> {after:?}");
        assert_eq!((after.width() % 20.0, after.height() % 20.0), (0.0, 0.0));
    }

    #[test]
    fn a_locked_desk_cannot_move_or_resize_or_open_the_menu() {
        let (mut g, mut p, strip, item) = drawn();
        assert!(g.desk().locked, "the project was saved locked");
        let before = rect_of_node_1(&g);
        p.drag(&mut g, strip.center(), Vec2::new(47.0, 33.0));
        p.drag(&mut g, item.max - Vec2::splat(4.0), Vec2::new(80.0, 60.0));
        assert_eq!(rect_of_node_1(&g), before);
        p.right_click(&mut g, item.center());
        assert!(g.desk_ui.menu.is_none());
        p.frame(&mut g, vec![]);
        assert!(g.desk().locked);
    }

    #[test]
    fn right_clicking_an_item_of_an_unlocked_desk_opens_its_menu() {
        let (mut g, mut p, _, item) = drawn();
        g.desk.locked = false;
        p.right_click(&mut g, item.center());
        assert_eq!(g.desk_ui.menu.map(|m| (m.0, m.1)), Some((NodeId(1), DeskKind::Node)));
        g.remove_from_desk(NodeId(1), DeskKind::Node);
        assert!(entries(&g).iter().all(|e| e.0 != 1));
    }

    /// A node with three snapshots whose row is on the Desk, drawn once.
    fn with_a_snapshot_row() -> (NodeGraph, Pointer) {
        let mut g = fresh();
        {
            let mut shared = g.levels[0].nodes[0].shared_state().lock();
            for name in ["Intro", "Drop", "Outro"] {
                shared.snapshots.add(Some(name.to_string()), vec![]);
            }
            shared.snapshot_active = Some(shared.snapshots.snapshots[1].id);
            DeskItem::edit(&mut shared.desk, |d| d.snapshots = Some(DeskRect::UNPLACED));
        }
        let mut p = Pointer::new();
        p.frame(&mut g, vec![]);
        p.frame(&mut g, vec![]);
        (g, p)
    }

    fn snapshot_item(g: &NodeGraph) -> (Rect, Rect) {
        let (_, _, strip, item) = *g.desk_ui.shown.iter().find(|(id, kind, _, _)| *id == NodeId(1) && *kind == DeskKind::Snapshots).expect("the row is drawn");
        (strip, item)
    }

    #[test]
    fn a_snapshot_row_is_its_own_item_placed_by_the_graph() {
        let (g, _) = with_a_snapshot_row();
        let all = collect_desk(&g.levels, &g.reachable_levels(), g.desk());
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].kind, DeskKind::Snapshots);
        assert!(all[0].rect.width() >= 3.0 * crate::widgets::nodes::desk::SNAP_BUTTON.x, "all three buttons in a line: {:?}", all[0].rect);
        assert!(entries(&g).is_empty(), "the node itself is not on the Desk");
    }

    #[test]
    fn a_click_on_a_snapshot_button_recalls_it_even_on_a_locked_desk() {
        let (mut g, mut p) = with_a_snapshot_row();
        g.desk.locked = true;
        p.frame(&mut g, vec![]);
        let (_, item) = snapshot_item(&g);
        let body = crate::widgets::nodes::desk::strip_and_cell(item, 0.0).1;
        let buttons = crate::widgets::nodes::desk::snapshot_buttons(body, 3);
        p.go(&mut g, buttons[2].center());
        p.button(&mut g, egui::PointerButton::Primary, true);
        p.button(&mut g, egui::PointerButton::Primary, false);
        assert_eq!(g.levels[0].nodes[0].shared_state().lock().pending_recall, Some(2));
    }

    #[test]
    fn the_snapshot_row_moves_alone_and_leaves_the_node_where_it_is() {
        let (mut g, mut p) = with_a_snapshot_row();
        g.desk.locked = false;
        p.frame(&mut g, vec![]);
        let (strip, item) = snapshot_item(&g);
        let before = g.levels[0].nodes[0].shared_state().lock().desk.clone().unwrap().snapshots.unwrap();
        p.drag(&mut g, strip.center() + Vec2::new(40.0, 0.0), Vec2::new(100.0, 60.0));
        let after = g.levels[0].nodes[0].shared_state().lock().desk.clone().unwrap();
        assert_eq!(after.snapshots.unwrap().size, before.size);
        assert_eq!(after.snapshots.unwrap().pos, [snap_to(before.pos[0] + 100.0), snap_to(before.pos[1] + 60.0)]);
        assert!(after.node.is_none());
        let _ = item;
    }

    fn snap_to(v: f32) -> f32 {
        crate::widgets::nodes::desk::snap(v)
    }

    #[test]
    fn the_snapshot_row_can_be_taken_off_alone() {
        let (mut g, _) = with_a_snapshot_row();
        g.remove_from_desk(NodeId(1), DeskKind::Snapshots);
        assert!(desk_of(&g, 0, 1).is_none());
    }

    #[test]
    fn a_jump_asked_for_on_the_desk_opens_the_node_and_asks_for_the_patch() {
        let mut g = loaded();
        g.pending_jump = Some(NodeId(3));
        let outcome = frame(&mut g);
        assert!(outcome.to_patch);
        assert_eq!(g.active_level, inner_level(&g));
        assert_eq!(g.selected_nodes.len(), 1);
    }

    #[test]
    fn pages_are_named_by_id() {
        let doc: DeskDoc = serde_json::from_str(r#"{"pages": [{"id": 3, "name": "A"}]}"#).unwrap();
        assert_eq!(doc.pages, vec![DeskPage { id: 3, name: "A".into() }]);
        assert!(!doc.locked);
    }
}
