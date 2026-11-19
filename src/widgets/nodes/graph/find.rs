//! Find (Ctrl+F): search the nodes and frames of every level of the project by
//! type, title and linked stack name, and jump to one.

use super::*;

#[derive(Debug, Clone, PartialEq)]
pub enum FindTarget {
    Node(NodeId),
    Frame(u64),
}

#[derive(Debug, Clone)]
pub struct FindHit {
    pub level: usize,
    pub target: FindTarget,
    /// "LaunchPad / Sequence / Schmitt Trigger": the subgraphs it sits in,
    /// then its own title.
    pub path: String,
    pub title: String,
    pub kind: String,
}

pub(super) struct FindState {
    query: String,
    selected: usize,
    focused: bool,
}

const MAX_HITS: usize = 60;
/// A level that is zoomed out further than this is brought up to it when
/// jumped to, so the hit can be read.
const MIN_JUMP_ZOOM: f32 = 0.6;

/// How well one lowercase word matches one text; lower is better.
fn text_score(text: &str, word: &str) -> Option<usize> {
    let text = text.to_lowercase();
    if text.starts_with(word) {
        Some(0)
    } else if text.split(|c: char| !c.is_alphanumeric()).any(|part| part.starts_with(word)) {
        Some(1)
    } else if text.contains(word) {
        Some(2)
    } else if word.chars().count() >= 3 && quick_add::is_subsequence(word, &text) {
        Some(3)
    } else {
        None
    }
}

/// Score of a query against a node's searchable texts, lower is better: a hit
/// in the title beats one in the linked stack name beats one in the type name.
/// Every word of the query has to match at least one text.
pub(super) fn search_score(query: &str, title: &str, linked: &str, kind: &str) -> Option<usize> {
    let mut total = 0;
    let mut any = false;
    for word in query.split_whitespace() {
        any = true;
        let word = word.to_lowercase();
        let best = [title, linked, kind].iter().enumerate()
            .filter(|(_, t)| !t.is_empty())
            .filter_map(|(field, t)| text_score(t, &word).map(|s| s * 3 + field))
            .min()?;
        total += best;
    }
    any.then_some(total)
}

/// Pan that puts world point `centre` in the middle of a canvas of `canvas`
/// pixels at `zoom`.
pub(super) fn pan_to_centre(canvas: Vec2, zoom: f32, centre: Pos2) -> Vec2 {
    canvas * 0.5 - centre.to_vec2() * zoom
}

impl NodeGraph {
    pub fn open_find(&mut self) {
        // Asking again while it is open only takes the focus back: the
        // query the user typed is theirs.
        match self.find.as_mut() {
            Some(state) => state.focused = false,
            None => self.find = Some(FindState { query: String::new(), selected: 0, focused: false }),
        }
    }

    /// Subgraph node ids from the root down to `level`, if every one of them
    /// still exists. A level whose subgraph node was deleted stays in the
    /// list but cannot be reached any more.
    pub(super) fn level_chain(&self, level: usize) -> Option<Vec<NodeId>> {
        let mut chain = Vec::new();
        let mut cur = level;
        while let Some(parent) = self.levels.get(cur)?.parent_level_idx {
            let sid = self.levels[cur].subgraph_id?;
            self.levels[parent].nodes.iter().position(|n| n.node_id() == sid)?;
            chain.push(sid);
            cur = parent;
            if chain.len() > self.levels.len() { return None; }
        }
        (cur == 0).then(|| { chain.reverse(); chain })
    }

    /// Titles of the subgraphs on the way to `level`, root first.
    pub(super) fn level_titles(&self, level: usize) -> Option<Vec<String>> {
        let mut cur = 0;
        let mut titles = Vec::new();
        for sid in self.level_chain(level)? {
            let node = self.levels[cur].nodes.iter().find(|n| n.node_id() == sid)?;
            titles.push(node.display_title().to_string());
            cur = self.levels.iter().position(|l| l.subgraph_id == Some(sid))?;
        }
        Some(titles)
    }

    /// Nodes and frames of every reachable level matching `query`, best first.
    /// Subgraphs that were never opened have no level yet and nothing to find.
    pub fn find(&self, query: &str) -> Vec<FindHit> {
        let mut scored: Vec<(usize, FindHit)> = Vec::new();
        for (li, level) in self.levels.iter().enumerate() {
            let Some(titles) = self.level_titles(li) else { continue };
            let prefix: String = titles.iter().map(|t| format!("{t} / ")).collect();
            for node in &level.nodes {
                if node.node_id() == BRIDGE_IN_NODE_ID || node.node_id() == BRIDGE_OUT_NODE_ID { continue; }
                let linked = node.linked_stack().unwrap_or_default();
                let Some(score) = search_score(query, node.title(), &linked, node.type_name()) else { continue };
                let title = node.display_title().to_string();
                scored.push((score, FindHit {
                    level: li,
                    target: FindTarget::Node(node.node_id()),
                    path: format!("{prefix}{title}"),
                    title,
                    kind: node.type_name().to_string(),
                }));
            }
            for frame in &level.frames {
                let Some(score) = search_score(query, &frame.title, "", "Frame") else { continue };
                let title = if frame.title.is_empty() { "Frame".to_string() } else { frame.title.clone() };
                scored.push((score, FindHit {
                    level: li,
                    target: FindTarget::Frame(frame.id),
                    path: format!("{prefix}{title}"),
                    title,
                    kind: "Frame".to_string(),
                }));
            }
        }
        scored.sort_by(|a, b| (a.0, a.1.path.as_str()).cmp(&(b.0, b.1.path.as_str())));
        scored.into_iter().take(MAX_HITS).map(|(_, h)| h).collect()
    }

    /// Open `level` the way a double-click on each subgraph on the way would
    /// (so the breadcrumb and the bridge ports are right), select the node or
    /// frame and put it in the middle of the canvas.
    pub fn go_to(&mut self, level: usize, target: &FindTarget) {
        let Some(chain) = self.level_chain(level) else { return };
        self.active_level = 0;
        for sid in chain {
            let Some(idx) = self.active().nodes.iter().position(|n| n.node_id() == sid) else { return };
            self.navigate_into(idx);
        }
        self.selected_nodes.clear();
        self.selected_frames.clear();
        self.selected_connections.clear();

        let canvas = self.canvas_rect;
        let lvl = &self.levels[self.active_level];
        let centre = match target {
            FindTarget::Node(id) => {
                let Some(i) = lvl.states.iter().position(|s| s.id == *id) else { return };
                let size = node_size(lvl.nodes[i].as_ref(), lvl.states[i].size_override);
                let centre = lvl.states[i].pos + size * 0.5;
                self.selected_nodes = vec![i];
                centre
            }
            FindTarget::Frame(id) => {
                let Some(f) = lvl.frames.iter().find(|f| f.id == *id) else { return };
                let centre = f.rect().center();
                self.selected_frames.insert(*id);
                centre
            }
        };
        let lvl = self.active_mut();
        lvl.zoom = lvl.zoom.max(MIN_JUMP_ZOOM);
        if canvas.is_positive() {
            lvl.pan = pan_to_centre(canvas.size(), lvl.zoom, centre);
        }
    }

    pub(super) fn show_find(&mut self, ui: &mut Ui) {
        let Some(mut state) = self.find.take() else { return };
        let hits = self.find(&state.query);
        state.selected = state.selected.min(hits.len().saturating_sub(1));

        let (down, up, escape) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if down && !hits.is_empty() {
            state.selected = (state.selected + 1) % hits.len();
        }
        if up && !hits.is_empty() {
            state.selected = (state.selected + hits.len() - 1) % hits.len();
        }

        let mut chosen: Option<usize> = None;
        let mut query_changed = false;
        let top_left = self.canvas_rect.center_top() + Vec2::new(-190.0, 40.0);
        let area = egui::Area::new(egui::Id::new("find_popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(top_left)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(380.0);
                    let edit = egui::TextEdit::singleline(&mut state.query)
                        .hint_text("Find a node, subgraph, portal, stack or frame")
                        .desired_width(f32::INFINITY);
                    let response = ui.add(edit);
                    if !state.focused {
                        response.request_focus();
                        state.focused = true;
                    }
                    query_changed = response.changed();
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        chosen = Some(state.selected);
                    }
                    ui.add_space(2.0);
                    egui::ScrollArea::vertical().max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
                        if hits.is_empty() {
                            let hint = if state.query.trim().is_empty() { "type part of a name" } else { "nothing found" };
                            ui.colored_label(theme::TEXT_DIM, hint);
                        }
                        for (row, hit) in hits.iter().enumerate() {
                            let text = if hit.title == hit.kind {
                                hit.path.clone()
                            } else {
                                format!("{}    ({})", hit.path, hit.kind)
                            };
                            let resp = ui.selectable_label(row == state.selected, text);
                            if (down || up) && row == state.selected {
                                resp.scroll_to_me(None);
                            }
                            if resp.clicked() {
                                chosen = Some(row);
                            }
                        }
                    });
                });
            });
        if query_changed {
            state.selected = 0;
        }
        let clicked_outside = ui.input(|i| i.pointer.any_pressed())
            && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p));
        if let Some(row) = chosen {
            if let Some(hit) = hits.get(row) {
                self.go_to(hit.level, &hit.target);
            }
            return;
        }
        if escape || clicked_outside {
            return;
        }
        self.find = Some(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::new_shared_state;
    use crate::widgets::nodes::ui::fader::FaderWidget;

    fn subgraph(id: u64, name: &str) -> Box<dyn NodeWidget> {
        let mut s = SubgraphWidget::new(NodeId(id), new_shared_state(0, 0));
        s.restore_from_save(Some(&serde_json::json!({ "name": name })), &[]);
        Box::new(s)
    }

    fn fader(id: u64, name: &str) -> Box<dyn NodeWidget> {
        let mut f = FaderWidget::new(NodeId(id), new_shared_state(0, 1));
        f.set_name(name);
        Box::new(f)
    }

    /// Root: LaunchPad (1) > Sequence (2) > Master fader (3), plus a fader
    /// "Master Dimmer" (4) and a frame "Stage" at the root. Active level: root.
    fn project() -> NodeGraph {
        let mut g = NodeGraph::new();
        g.add_node(subgraph(1, "LaunchPad"), Pos2::ZERO);
        g.add_node(fader(4, "Master Dimmer"), Pos2::new(400.0, 300.0));
        g.levels[0].frames.push(GraphFrame {
            id: 7, title: "Stage".into(), color: Color32::WHITE,
            notes: String::new(), pos: Pos2::new(1000.0, 1000.0), size: Vec2::new(200.0, 100.0),
        });
        g.navigate_into_by_index(0);
        g.add_node(subgraph(2, "Sequence"), Pos2::ZERO);
        g.navigate_into_by_index(2);
        g.add_node(fader(3, "Master"), Pos2::new(600.0, 200.0));
        g.navigate_to_level(0);
        g
    }

    #[test]
    fn finds_nodes_on_every_level_with_their_path() {
        let g = project();
        let hits = g.find("master");
        let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
        assert_eq!(paths, ["LaunchPad / Sequence / Master", "Master Dimmer"]);
        assert_eq!(hits[0].level, 2);
        assert_eq!(hits[0].target, FindTarget::Node(NodeId(3)));
    }

    #[test]
    fn opening_find_again_keeps_the_query() {
        let mut g = project();
        g.open_find();
        g.find.as_mut().unwrap().query = "master".into();
        g.find.as_mut().unwrap().focused = true;
        g.open_find();
        let state = g.find.as_ref().unwrap();
        assert_eq!(state.query, "master");
        assert!(!state.focused, "the box asks for the focus again");
    }

    #[test]
    fn finds_by_type_and_by_part_of_a_title() {
        let g = project();
        assert_eq!(g.find("subgraph").len(), 2, "both subgraphs by their type name");
        assert!(g.find("seq").iter().any(|h| h.path == "LaunchPad / Sequence"));
        assert!(g.find("zzz").is_empty());
        assert!(g.find("  ").is_empty(), "an empty query finds nothing");
    }

    #[test]
    fn finds_frames_by_title() {
        let g = project();
        let hits = g.find("stage");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].target, FindTarget::Frame(7));
    }

    #[test]
    fn every_word_must_match() {
        let g = project();
        assert_eq!(g.find("master dim").len(), 1);
        assert!(g.find("master stage").is_empty());
    }

    #[test]
    fn jumping_opens_the_nested_level_and_selects_the_node() {
        let mut g = project();
        let hit = g.find("master").remove(0);
        g.go_to(hit.level, &hit.target);
        assert_eq!(g.active_level_index(), 2);
        assert_eq!(g.current_subgraph_path(), vec![NodeId(1), NodeId(2)]);
        let sel = g.selected_nodes.clone();
        assert_eq!(sel.len(), 1);
        assert_eq!(g.active().states[sel[0]].id, NodeId(3));
    }

    #[test]
    fn jumping_back_to_the_root_from_a_nested_level() {
        let mut g = project();
        g.go_to(2, &FindTarget::Node(NodeId(3)));
        g.go_to(0, &FindTarget::Node(NodeId(4)));
        assert_eq!(g.active_level_index(), 0);
        assert_eq!(g.active().states[g.selected_nodes[0]].id, NodeId(4));
    }

    #[test]
    fn a_level_whose_subgraph_was_deleted_is_not_searched() {
        let mut g = project();
        g.levels[0].nodes.remove(0);
        g.levels[0].states.remove(0);
        assert!(g.find("master").iter().all(|h| h.level == 0));
    }

    #[test]
    fn centring_puts_the_point_in_the_middle() {
        let pan = pan_to_centre(Vec2::new(800.0, 600.0), 2.0, Pos2::new(100.0, 50.0));
        assert_eq!(pan, Vec2::new(200.0, 200.0));
    }
}
