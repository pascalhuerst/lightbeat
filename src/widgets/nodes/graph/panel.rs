//! Front panels: the controls of a subgraph's inner nodes drawn on the
//! Subgraph node itself, so a subgraph can be played without opening it.
//!
//! The inner widgets keep talking to the engine through their own shared
//! state, so a control drawn here needs no path of its own: nothing a widget
//! does while drawing depends on which level is open (the structural commands
//! that do, `push_engine_cmd`, are only used by the graph's own actions).

use super::*;
use crate::engine::types::PanelItem;
use crate::widgets::nodes::panel::{self as geometry, PanelRequest, PanelRow};
use crate::widgets::setup_ui::Move;

/// Nesting deeper than this is a loop in the data, not a design.
const MAX_PANEL_DEPTH: usize = 32;

impl NodeGraph {
    /// The level inside the Subgraph `id`, if it has been created.
    fn inner_level_of(&self, id: NodeId) -> Option<usize> {
        self.levels.iter().position(|l| l.subgraph_id == Some(id))
    }

    /// The nodes of `level` that put something on the panel, in panel order
    /// (a tie keeps the order on the canvas).
    pub(super) fn panel_members(&self, level: usize) -> Vec<usize> {
        panel_members_of(&self.levels[level].nodes)
    }

    /// Number the items of `level` 0.. in their order, so a freshly flagged
    /// one (`PANEL_LAST`) takes the end and moves can swap neighbours.
    fn normalize_panel_order(&mut self, level: usize) {
        let members = self.panel_members(level);
        for (n, &i) in members.iter().enumerate() {
            let mut shared = self.levels[level].nodes[i].shared_state().lock();
            if let Some(item) = shared.panel.as_mut()
                && item.order != n as u32
            {
                item.order = n as u32;
            }
        }
    }

    /// Tell every Subgraph node under `level` what is on its panel and how wide
    /// it is drawn, innermost first: a Subgraph inside a panel takes one cell
    /// whose size is its own, panel included.
    pub(super) fn refresh_panels(&mut self, level: usize) {
        self.refresh_panels_at(level, 0);
    }

    fn refresh_panels_at(&mut self, level: usize, depth: usize) {
        if depth > MAX_PANEL_DEPTH {
            return;
        }
        let subs: Vec<(usize, usize)> = self.levels[level]
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.type_name() == "Subgraph")
            .filter_map(|(i, n)| Some((i, self.inner_level_of(n.node_id())?)))
            .filter(|&(_, inner)| inner != level)
            .collect();
        for (i, inner) in subs {
            self.refresh_panels_at(inner, depth + 1);
            self.normalize_panel_order(inner);
            let (cells, rows) = self.panel_cells(inner);
            let override_size = self.levels[level].states[i].size_override;
            let node = &mut self.levels[level].nodes[i];
            let Some(w) = node.as_any_mut().downcast_mut::<SubgraphWidget>() else { continue };
            w.set_panel(cells, rows);
            let width = node_size(&*self.levels[level].nodes[i], override_size).x;
            if let Some(w) = self.levels[level].nodes[i].as_any_mut().downcast_mut::<SubgraphWidget>() {
                w.set_panel_width(width);
            }
        }
    }

    /// Cell sizes and inspector rows of the panel inside `level`.
    fn panel_cells(&self, level: usize) -> (Vec<Vec2>, Vec<PanelRow>) {
        let lvl = &self.levels[level];
        let mut cells = Vec::new();
        let mut rows = Vec::new();
        for (i, param) in panel_cell_list(&lvl.nodes) {
            let node = &lvl.nodes[i];
            let title = node.display_title();
            let label = match &param {
                Some(name) => format!("{title}: {}", node.param_display(name).1),
                None => title.to_string(),
            };
            cells.push(cell_size_of(node.as_ref(), lvl.states[i].size_override, param.is_some()));
            rows.push(PanelRow { node_id: node.node_id(), param, label });
        }
        (cells, rows)
    }

    /// Put the node at `index` of the active level on its subgraph's panel, or
    /// take it off.
    pub fn set_panel_whole(&mut self, index: usize, on: bool) {
        let Some(node) = self.active().nodes.get(index) else { return };
        PanelItem::set_whole(&mut node.shared_state().lock().panel, on);
    }

    /// Carry out what the Front panel lists of the Subgraph nodes on the
    /// active level asked for.
    pub(super) fn apply_panel_requests(&mut self) {
        let level = self.active_level;
        let requests: Vec<(NodeId, PanelRequest)> = self.levels[level]
            .nodes
            .iter_mut()
            .filter_map(|n| {
                let id = n.node_id();
                let req = n.as_any_mut().downcast_mut::<SubgraphWidget>()?.take_panel_request()?;
                Some((id, req))
            })
            .collect();
        for (sub, req) in requests {
            let Some(inner) = self.inner_level_of(sub) else { continue };
            match req {
                PanelRequest::Remove(id, param) => self.clear_panel_item(inner, id, param),
                PanelRequest::Move(id, mv) => self.move_panel_item(inner, id, mv),
            }
        }
    }

    fn clear_panel_item(&mut self, level: usize, id: NodeId, param: Option<String>) {
        let Some(node) = self.levels[level].nodes.iter().find(|n| n.node_id() == id) else { return };
        let mut shared = node.shared_state().lock();
        match param {
            Some(name) => PanelItem::set_param(&mut shared.panel, &name, false),
            None => PanelItem::set_whole(&mut shared.panel, false),
        }
    }

    fn move_panel_item(&mut self, level: usize, id: NodeId, mv: Move) {
        self.normalize_panel_order(level);
        let members = self.panel_members(level);
        let Some(pos) = members.iter().position(|&i| self.levels[level].nodes[i].node_id() == id) else { return };
        let other = match mv {
            Move::Up if pos > 0 => pos - 1,
            Move::Down if pos + 1 < members.len() => pos + 1,
            _ => return,
        };
        for (a, b) in [(members[pos], other as u32), (members[other], pos as u32)] {
            if let Some(item) = self.levels[level].nodes[a].shared_state().lock().panel.as_mut() {
                item.order = b;
            }
        }
    }
}

fn panel_members_of(nodes: &[Box<dyn NodeWidget>]) -> Vec<usize> {
    let mut members: Vec<(u32, usize)> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| ![BRIDGE_IN_NODE_ID, BRIDGE_OUT_NODE_ID].contains(&n.node_id()))
        .filter_map(|(i, n)| {
            let shared = n.shared_state().lock();
            shared.panel.as_ref().filter(|p| !p.is_empty()).map(|p| (p.order, i))
        })
        .collect();
    members.sort_unstable();
    members.into_iter().map(|(_, i)| i).collect()
}

/// The cells of a panel in order: a node's whole self first, then its single
/// settings, as (node index, setting).
fn panel_cell_list(nodes: &[Box<dyn NodeWidget>]) -> Vec<(usize, Option<String>)> {
    let mut cells = Vec::new();
    for i in panel_members_of(nodes) {
        let item = nodes[i].shared_state().lock().panel.clone().unwrap_or_default();
        if item.whole {
            cells.push((i, None));
        }
        cells.extend(item.params.into_iter().map(|p| (i, Some(p))));
    }
    cells
}

fn cell_size_of(node: &dyn NodeWidget, size_override: Option<Vec2>, single_setting: bool) -> Vec2 {
    if single_setting {
        geometry::PARAM_CELL
    } else {
        geometry::cell_size(&node.layout(), node_size(node, size_override))
    }
}

/// What drawing needs besides the panel's own rectangle.
pub(super) struct PanelPaint<'a> {
    pub ui: &'a mut Ui,
    pub clip: Rect,
    pub style: &'a Option<std::sync::Arc<egui::Style>>,
    pub zoom: f32,
}

/// The inner level of `node` when it is a Subgraph with something on its panel.
pub(super) fn panel_level(levels: &[GraphLevel], node: &mut dyn NodeWidget) -> Option<usize> {
    let id = panel_owner(node)?;
    levels.iter().position(|l| l.subgraph_id == Some(id))
}

/// The id of `node` when it is a Subgraph with something on its panel.
pub(super) fn panel_owner(node: &mut dyn NodeWidget) -> Option<NodeId> {
    let id = node.node_id();
    node.as_any_mut().downcast_mut::<SubgraphWidget>()?.has_panel().then_some(id)
}

/// The room below a Subgraph's header row, where its panel goes.
pub(super) fn panel_region(content: Rect, zoom: f32) -> Rect {
    Rect::from_min_max(
        Pos2::new(content.min.x, content.min.y + crate::widgets::nodes::meta::subgraph::PANEL_TOP * zoom),
        content.max,
    )
}

/// Draw the controls of `inner` (whose widgets are `nodes`, taken out of the
/// level so the rest of `levels` stays borrowable for nested panels) into
/// `region`, in a grid that wraps at the region's width.
pub(super) fn draw_panel(
    levels: &mut [GraphLevel],
    inner: usize,
    nodes: &mut [Box<dyn NodeWidget>],
    region: Rect,
    paint: &mut PanelPaint,
    depth: usize,
) {
    let z = paint.zoom;
    let list = panel_cell_list(nodes);
    let sizes: Vec<Vec2> = list
        .iter()
        .map(|(i, param)| cell_size_of(nodes[*i].as_ref(), levels[inner].states[*i].size_override, param.is_some()))
        .collect();
    let flow = geometry::flow(&sizes, region.width() / z);
    for (n, (i, param)) in list.iter().enumerate() {
        let cell = Rect::from_min_size(region.min + flow.origins[n].to_vec2() * z, sizes[n] * z);
        if !cell.intersects(paint.clip) {
            continue;
        }
        draw_cell(levels, nodes, *i, param.as_deref(), cell, "front_panel", paint, depth);
    }
}

/// One cell: node `i` of `nodes` (or one setting of it) drawn into `cell`, the
/// way a front panel does it. The Desk draws its items through here too.
/// `salt` keeps the egui ids of the same node apart between the places it is
/// drawn.
pub(super) fn draw_cell(
    levels: &mut [GraphLevel],
    nodes: &mut [Box<dyn NodeWidget>],
    i: usize,
    param: Option<&str>,
    cell: Rect,
    salt: &'static str,
    paint: &mut PanelPaint,
    depth: usize,
) {
    let z = paint.zoom;
    let layout = if param.is_some() { geometry::param_layout() } else { nodes[i].layout() };
    let disabled = nodes[i].shared_state().lock().disabled;
    let painter = paint.ui.painter().with_clip_rect(paint.clip);
    painter.rect_filled(cell, NODE_CORNER_RADIUS, theme::BG);
    if layout.show_title && layout.title_h > 0.0 {
        painter.text(
            cell.min + Vec2::new(6.0, layout.title_h * 0.5) * z,
            egui::Align2::LEFT_CENTER,
            nodes[i].display_title(),
            egui::FontId::proportional(if layout.bare { 10.0 } else { 12.0 } * z),
            theme::TEXT_DIM,
        );
    }
    painter.rect_stroke(cell, NODE_CORNER_RADIUS, Stroke::new(1.0_f32, theme::STROKE), StrokeKind::Inside);

    let content = geometry::cell_content_rect(cell, &layout, z);
    let clip = content.intersect(paint.clip);
    if content.width() > 0.0 && content.height() > 0.0 && clip.width() > 0.0 && clip.height() > 0.0 {
        let node_id = nodes[i].node_id();
        let mut child = paint.ui.new_child(
            egui::UiBuilder::new()
                .id_salt((salt, node_id.0, param))
                .max_rect(content)
                .layout(egui::Layout::top_down(egui::Align::LEFT)),
        );
        child.set_clip_rect(clip);
        if let Some(style) = paint.style {
            child.set_style(style.clone());
        }
        if let Some(name) = param {
            crate::widgets::inspector::param_control(&mut child, nodes[i].as_ref(), name);
        } else {
            // A Subgraph in a panel shows its own panel in its cell.
            let nested = if depth < MAX_PANEL_DEPTH { panel_level(levels, nodes[i].as_mut()) } else { None };
            let mut taken = nested.map(|k| std::mem::take(&mut levels[k].nodes));
            if let Some(w) = nodes[i].as_any_mut().downcast_mut::<SubgraphWidget>() {
                w.set_in_cell(true);
            }
            nodes[i].show_content(&mut child, z);
            if let Some(w) = nodes[i].as_any_mut().downcast_mut::<SubgraphWidget>() {
                w.set_in_cell(false);
            }
            if let (Some(k), Some(inner_nodes)) = (nested, taken.as_mut()) {
                let sub_region = panel_region(content, z);
                if let Some(w) = nodes[i].as_any_mut().downcast_mut::<SubgraphWidget>() {
                    w.set_panel_screen(Some(sub_region.intersect(clip)));
                }
                draw_panel(levels, k, inner_nodes, sub_region, paint, depth + 1);
            }
            if let (Some(k), Some(restored)) = (nested, taken) {
                levels[k].nodes = restored;
            }
        }
    }
    if disabled {
        let body = Rect::from_min_max(cell.min + Vec2::new(0.0, layout.title_h * z), cell.max);
        painter.rect_filled(body, NODE_CORNER_RADIUS, theme::DISABLED_WASH);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::new_shared_state;
    use crate::widgets::nodes::meta::subgraph::SubgraphWidget;

    /// A node that only exists to have a size and a panel flag.
    struct Probe {
        id: NodeId,
        shared: crate::engine::types::SharedState,
        height: f32,
    }

    impl Probe {
        fn boxed(id: u64, height: f32, panel: Option<PanelItem>) -> Box<dyn NodeWidget> {
            let shared = new_shared_state(0, 0);
            shared.lock().panel = panel;
            Box::new(Probe { id: NodeId(id), shared, height })
        }
    }

    impl NodeWidget for Probe {
        fn node_id(&self) -> NodeId { self.id }
        fn type_name(&self) -> &'static str { "Probe" }
        fn shared_state(&self) -> &crate::engine::types::SharedState { &self.shared }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn ui_inputs(&self) -> Vec<UiPortDef> { Vec::new() }
        fn ui_outputs(&self) -> Vec<UiPortDef> { Vec::new() }
        fn show_content(&mut self, _ui: &mut Ui, _zoom: f32) {}
        fn min_content_height(&self) -> f32 { self.height }
        fn resizable(&self) -> bool { true }
    }

    /// Draw a Subgraph's content in a 300 x 100 window and click where its
    /// open arrow sits; whether that asked to open it.
    fn clicking_the_open_arrow_asks_to_open(in_cell: bool) -> bool {
        let ctx = egui::Context::default();
        let mut w = SubgraphWidget::new(NodeId(1), new_shared_state(12, 12));
        w.set_in_cell(in_cell);
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 100.0));
        let at = Pos2::new(282.0, 16.0);
        let steps = [
            vec![egui::Event::PointerMoved(at)],
            vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }],
            vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }],
        ];
        let _ = ctx.run(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| w.show_content(ui, 1.0));
        });
        for events in steps {
            let _ = ctx.run(egui::RawInput { screen_rect: Some(screen), events, ..Default::default() }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| w.show_content(ui, 1.0));
            });
        }
        w.wants_open
    }

    #[test]
    fn a_subgraph_drawn_as_a_cell_has_no_open_arrow() {
        assert!(clicking_the_open_arrow_asks_to_open(false), "the click lands on the arrow when it is drawn");
        assert!(!clicking_the_open_arrow_asks_to_open(true), "in a cell nothing is left set for a later frame");
    }

    fn whole(order: u32) -> Option<PanelItem> {
        Some(PanelItem { order, whole: true, params: Vec::new() })
    }

    /// A graph whose root holds one Subgraph (id 100) whose inner level holds
    /// the given nodes.
    fn graph_with_inner(inner: Vec<Box<dyn NodeWidget>>) -> NodeGraph {
        let mut g = NodeGraph::new();
        let sub = SubgraphWidget::new(NodeId(100), new_shared_state(12, 12));
        g.add_node(Box::new(sub), Pos2::ZERO);
        g.navigate_into_by_index(0);
        for (k, n) in inner.into_iter().enumerate() {
            g.add_node(n, Pos2::new(k as f32 * 10.0, 0.0));
        }
        g.navigate_to_level(0);
        g
    }

    fn inner_ids(g: &NodeGraph) -> Vec<u64> {
        g.panel_members(1).into_iter().map(|i| g.levels[1].nodes[i].node_id().0).collect()
    }

    #[test]
    fn members_come_in_panel_order_and_unflagged_nodes_are_left_out() {
        let g = graph_with_inner(vec![
            Probe::boxed(1, 10.0, whole(2)),
            Probe::boxed(2, 10.0, None),
            Probe::boxed(3, 10.0, whole(0)),
            Probe::boxed(4, 10.0, whole(1)),
        ]);
        assert_eq!(inner_ids(&g), vec![3, 4, 1]);
    }

    #[test]
    fn a_node_just_flagged_goes_last_and_the_orders_are_renumbered() {
        let mut g = graph_with_inner(vec![
            Probe::boxed(1, 10.0, whole(0)),
            Probe::boxed(2, 10.0, None),
            Probe::boxed(3, 10.0, whole(5)),
        ]);
        g.navigate_to_level(1);
        let idx = g.levels[1].nodes.iter().position(|n| n.node_id() == NodeId(2)).unwrap();
        g.set_panel_whole(idx, true);
        g.normalize_panel_order(1);
        assert_eq!(inner_ids(&g), vec![1, 3, 2]);
        let orders: Vec<u32> = g.panel_members(1).iter()
            .map(|&i| g.levels[1].nodes[i].shared_state().lock().panel.as_ref().unwrap().order)
            .collect();
        assert_eq!(orders, vec![0, 1, 2]);
    }

    #[test]
    fn unflagging_clears_the_item() {
        let mut g = graph_with_inner(vec![Probe::boxed(1, 10.0, whole(0))]);
        g.navigate_to_level(1);
        let idx = g.levels[1].nodes.iter().position(|n| n.node_id() == NodeId(1)).unwrap();
        g.set_panel_whole(idx, false);
        assert!(g.levels[1].nodes[idx].shared_state().lock().panel.is_none());
    }

    #[test]
    fn moving_swaps_neighbours_and_stops_at_the_ends() {
        let mut g = graph_with_inner(vec![
            Probe::boxed(1, 10.0, whole(0)),
            Probe::boxed(2, 10.0, whole(1)),
            Probe::boxed(3, 10.0, whole(2)),
        ]);
        g.move_panel_item(1, NodeId(3), Move::Up);
        assert_eq!(inner_ids(&g), vec![1, 3, 2]);
        g.move_panel_item(1, NodeId(1), Move::Up);
        assert_eq!(inner_ids(&g), vec![1, 3, 2]);
        g.move_panel_item(1, NodeId(1), Move::Down);
        assert_eq!(inner_ids(&g), vec![3, 1, 2]);
    }

    #[test]
    fn the_subgraph_node_grows_to_hold_its_panel_and_wraps_to_its_width() {
        let mut g = graph_with_inner(vec![
            Probe::boxed(1, 40.0, whole(0)),
            Probe::boxed(2, 40.0, whole(1)),
        ]);
        g.refresh_panels(0);
        let plain = {
            let mut g0 = graph_with_inner(vec![]);
            g0.refresh_panels(0);
            node_size(&*g0.levels[0].nodes[0], None)
        };
        let one_row = node_size(&*g.levels[0].nodes[0], None);
        assert!(one_row.y > plain.y, "the panel adds height");
        let sub = g.levels[0].nodes[0].as_any_mut().downcast_mut::<SubgraphWidget>().unwrap();
        assert!(sub.has_panel());

        // A node wide enough for both cells side by side is shorter than the
        // minimum width, where they stack.
        let wide = Vec2::new(one_row.x * 3.0, 10.0);
        g.levels[0].states[0].size_override = Some(wide);
        g.refresh_panels(0);
        let side_by_side = node_size(&*g.levels[0].nodes[0], Some(wide));
        assert!(side_by_side.y < one_row.y, "wider means fewer rows: {} vs {}", side_by_side.y, one_row.y);
    }

    #[test]
    fn a_panel_replaces_nothing_when_no_node_is_flagged() {
        let mut g = graph_with_inner(vec![Probe::boxed(1, 10.0, None)]);
        g.refresh_panels(0);
        let sub = g.levels[0].nodes[0].as_any_mut().downcast_mut::<SubgraphWidget>().unwrap();
        assert!(!sub.has_panel());
    }

    #[test]
    fn requests_from_the_inspector_reach_the_inner_nodes() {
        let mut g = graph_with_inner(vec![Probe::boxed(1, 10.0, whole(0)), Probe::boxed(2, 10.0, whole(1))]);
        g.refresh_panels(0);
        {
            let sub = g.levels[0].nodes[0].as_any_mut().downcast_mut::<SubgraphWidget>().unwrap();
            sub.request_panel(PanelRequest::Remove(NodeId(1), None));
        }
        g.apply_panel_requests();
        assert_eq!(inner_ids(&g), vec![2]);
    }

    #[test]
    fn panels_nest_a_subgraph_in_a_panel_is_one_cell_with_its_own_panel() {
        let mut g = NodeGraph::new();
        g.add_node(Box::new(SubgraphWidget::new(NodeId(100), new_shared_state(12, 12))), Pos2::ZERO);
        g.navigate_into_by_index(0);
        let inner_sub = SubgraphWidget::new(NodeId(200), new_shared_state(12, 12));
        inner_sub.shared_state().lock().panel = whole(0);
        let at = g.add_node(Box::new(inner_sub), Pos2::ZERO);
        g.navigate_into_by_index(at);
        g.add_node(Probe::boxed(1, 60.0, whole(0)), Pos2::ZERO);
        g.navigate_to_level(0);

        g.refresh_panels(0);
        let outer = g.levels[0].nodes[0].as_any_mut().downcast_mut::<SubgraphWidget>().unwrap();
        assert!(outer.has_panel(), "the grandparent shows the inner subgraph as a cell");
        let (cells, _) = g.panel_cells(1);
        let (leaf_cells, _) = g.panel_cells(2);
        assert_eq!(cells.len(), 1);
        assert!(cells[0].y > leaf_cells[0].y, "the cell includes the nested panel");
    }

    fn registry() -> NodeGraph {
        use crate::widgets::nodes::math::constant::ConstantWidget;
        let mut g = NodeGraph::new();
        g.register_node("Route", "Subgraph", |id| Box::new(SubgraphWidget::new(id, new_shared_state(12, 12))));
        g.register_node("Signal", "Const Value", |id| {
            Box::new(ConstantWidget::new(id, crate::engine::types::PortType::Untyped, new_shared_state(0, 1)))
        });
        g
    }

    const PROJECT: &str = r#"{
        "nodes": [{
            "type_name": "Subgraph", "id": 1, "pos": [0.0, 0.0], "params": [],
            "data": {"name": "Strobe", "inputs": [], "outputs": []},
            "inner_graph": {
                "nodes": [
                    {"type_name": "Const Value", "id": 2, "pos": [0.0, 0.0], "params": [], "panel": {"order": 1, "whole": true}},
                    {"type_name": "Const Value", "id": 3, "pos": [0.0, 50.0], "params": [], "panel": {"order": 0, "whole": true}},
                    {"type_name": "Const Value", "id": 4, "pos": [0.0, 90.0], "params": []}
                ],
                "connections": []
            }
        }],
        "connections": []
    }"#;

    #[test]
    fn the_flags_survive_a_save_and_a_load() {
        let mut g = registry();
        crate::project::load_graph(&mut g, &serde_json::from_str(PROJECT).unwrap());
        let inner = g.find_level_for_subgraph(NodeId(1)).expect("the project load opens every subgraph");
        let flagged: Vec<u64> = (0..inner.nodes.len())
            .filter(|&i| inner.nodes[i].shared_state().lock().panel.is_some())
            .map(|i| inner.nodes[i].node_id().0)
            .collect();
        assert_eq!(flagged, vec![2, 3]);

        let saved = crate::project::save_graph(&g);
        let json = serde_json::to_string(&saved).unwrap();
        let mut g2 = registry();
        crate::project::load_graph(&mut g2, &serde_json::from_str(&json).unwrap());
        let level = g2.levels.iter().position(|l| l.subgraph_id == Some(NodeId(1))).unwrap();
        let order: Vec<u64> = g2.panel_members(level).iter().map(|&i| g2.levels[level].nodes[i].node_id().0).collect();
        assert_eq!(order, vec![3, 2]);
        // A node that is not on the panel writes nothing.
        assert_eq!(json.matches("\"panel\"").count(), 2);
    }

    #[test]
    fn single_settings_get_their_own_cells_after_the_whole_node_and_round_trip() {
        let mut g = registry();
        let project = PROJECT.replace(
            r#""panel": {"order": 0, "whole": true}"#,
            r#""panel": {"order": 0, "whole": true, "params": ["Value", "Rate"]}"#,
        );
        crate::project::load_graph(&mut g, &serde_json::from_str(&project).unwrap());
        let level = g.levels.iter().position(|l| l.subgraph_id == Some(NodeId(1))).unwrap();
        let list: Vec<(u64, Option<String>)> = panel_cell_list(&g.levels[level].nodes)
            .into_iter()
            .map(|(i, p)| (g.levels[level].nodes[i].node_id().0, p))
            .collect();
        assert_eq!(list, vec![
            (3, None),
            (3, Some("Value".to_string())),
            (3, Some("Rate".to_string())),
            (2, None),
        ]);

        let (cells, rows) = g.panel_cells(level);
        assert_eq!(cells[1], geometry::PARAM_CELL);
        assert_eq!(rows[1].param.as_deref(), Some("Value"));
        assert!(rows[1].label.ends_with(": Value"), "{}", rows[1].label);

        let json = serde_json::to_string(&crate::project::save_graph(&g)).unwrap();
        assert!(json.contains(r#""params":["Value","Rate"]"#));
    }

    #[test]
    fn a_setting_is_taken_off_alone_and_the_item_goes_with_its_last_part() {
        let mut slot = None;
        PanelItem::set_param(&mut slot, "Rate", true);
        PanelItem::set_param(&mut slot, "Rate", true);
        PanelItem::set_whole(&mut slot, true);
        assert_eq!(slot.as_ref().unwrap().params, vec!["Rate".to_string()]);
        PanelItem::set_param(&mut slot, "Rate", false);
        assert!(slot.as_ref().unwrap().whole);
        PanelItem::set_whole(&mut slot, false);
        assert!(slot.is_none());
    }

    #[test]
    fn a_setting_row_is_removed_without_touching_the_node_on_the_panel() {
        let both = Some(PanelItem { order: 0, whole: true, params: vec!["Rate".into()] });
        let mut g = graph_with_inner(vec![Probe::boxed(1, 10.0, both)]);
        g.clear_panel_item(1, NodeId(1), Some("Rate".into()));
        let item = g.levels[1].nodes.iter().find(|n| n.node_id() == NodeId(1)).unwrap()
            .shared_state().lock().panel.clone().unwrap();
        assert!(item.whole && item.params.is_empty());
    }

    #[test]
    fn a_duplicated_subgraph_has_its_own_panel_without_being_opened() {
        let mut g = registry();
        crate::project::load_graph(&mut g, &serde_json::from_str(PROJECT).unwrap());
        g.selected_nodes = vec![0];
        g.duplicate_selected();
        assert_eq!(g.root_level().nodes.len(), 2);
        let copy_id = g.root_level().nodes[1].node_id();
        assert_ne!(copy_id, NodeId(1));
        let level = g.levels.iter().position(|l| l.subgraph_id == Some(copy_id)).expect("the copy's level exists");
        assert_eq!(g.panel_members(level).len(), 2);

        g.refresh_panels(0);
        for (i, node) in g.levels[0].nodes.iter_mut().enumerate().take(2) {
            let sub = node.as_any_mut().downcast_mut::<SubgraphWidget>().unwrap();
            assert!(sub.has_panel(), "subgraph {i} shows a panel");
        }
    }
}
