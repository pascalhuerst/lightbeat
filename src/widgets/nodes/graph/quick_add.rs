use super::*;

/// Registry indices matching `query`, best first. Every word of the query
/// must match. A word with a slash is an address (`math/mul`, `in/mid`): each
/// part has to begin the matching part of the node's path. Any other word
/// ranks by where it hits: the start of the node's name, the start of a part
/// of its path or of a word in its name, then anywhere in the name, its
/// category, and last letters in order.
fn quick_add_matches(registry: &[NodeEntry], query: &str) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    let mut scored: Vec<(usize, usize)> = Vec::new();
    for (i, entry) in registry.iter().enumerate() {
        let mut total = 0;
        let mut all = true;
        for w in &words {
            match word_score(entry, w) {
                Some(s) => total += s,
                None => {
                    all = false;
                    break;
                }
            }
        }
        if all {
            scored.push((total, i));
        }
    }
    scored.sort_by(|a, b| (a.0, registry[a.1].path.as_str()).cmp(&(b.0, registry[b.1].path.as_str())));
    scored.into_iter().map(|(_, i)| i).collect()
}

/// How well one lowercase word matches an entry; lower is better.
fn word_score(entry: &NodeEntry, word: &str) -> Option<usize> {
    let path = entry.path.to_lowercase();
    if word.contains('/') {
        // An address: the parts line up with the path's own, and may stop short.
        let wanted: Vec<&str> = word.split('/').collect();
        let parts: Vec<&str> = path.split('/').collect();
        if wanted.len() > parts.len() {
            return None;
        }
        let lines_up = wanted.iter().zip(&parts).all(|(w, p)| w.is_empty() || p.starts_with(w));
        return lines_up.then_some(0);
    }
    let label = entry.label.to_lowercase();
    let category = entry.category.to_lowercase();
    Some(if label.starts_with(word) {
        0
    } else if path.split('/').any(|part| part.starts_with(word)) || label.split(|c: char| !c.is_alphanumeric()).any(|part| part.starts_with(word)) {
        1
    } else if label.contains(word) {
        2
    } else if category.contains(word) {
        3
    } else if is_subsequence(word, &label) {
        4
    } else {
        return None;
    })
}

/// True when the letters of `needle` appear in `haystack` in that order.
fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut hay = haystack.chars();
    needle.chars().all(|n| hay.any(|h| h == n))
}

impl NodeGraph {
    /// Registry indices matching `query` the way quick add ranks them, for other
    /// node finders (the Add Nodes panel) to share the rules.
    pub fn find_entries(&self, query: &str) -> Vec<usize> {
        quick_add_matches(&self.registry, query)
    }

    pub(super) fn show_quick_add(&mut self, ui: &mut Ui, just_opened: bool) {
        let Some(mut qa) = self.quick_add.take() else { return };
        let matches = quick_add_matches(&self.registry, &qa.query);
        qa.selected = qa.selected.min(matches.len().saturating_sub(1));

        // Arrow keys move through the list; take them before the text box does.
        let (down, up, escape) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if down && !matches.is_empty() {
            qa.selected = (qa.selected + 1) % matches.len();
        }
        if up && !matches.is_empty() {
            qa.selected = (qa.selected + matches.len() - 1) % matches.len();
        }

        let mut chosen: Option<usize> = None;
        let mut query_changed = false;
        let area = egui::Area::new(egui::Id::new("quick_add"))
            .order(egui::Order::Foreground)
            .fixed_pos(qa.screen)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(280.0);
                    let edit = egui::TextEdit::singleline(&mut qa.query)
                        .hint_text("Add node (math/mul, input/midi, ...)")
                        .desired_width(f32::INFINITY);
                    let response = ui.add(edit);
                    if !qa.focused {
                        response.request_focus();
                        qa.focused = true;
                    }
                    query_changed = response.changed();
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        chosen = matches.get(qa.selected).copied();
                    }
                    ui.add_space(2.0);
                    egui::ScrollArea::vertical().max_height(260.0).auto_shrink([false, true]).show(ui, |ui| {
                        if matches.is_empty() {
                            ui.colored_label(theme::TEXT_DIM, "no matching node");
                        }
                        for (row, &idx) in matches.iter().enumerate() {
                            let entry = &self.registry[idx];
                            let resp = ui
                                .selectable_label(row == qa.selected, format!("{}   {}", entry.path, entry.label))
                                .on_hover_text(entry.description);
                            if (down || up) && row == qa.selected {
                                resp.scroll_to_me(None);
                            }
                            if resp.clicked() {
                                chosen = Some(idx);
                            }
                        }
                    });
                });
            });
        if query_changed {
            qa.selected = 0;
        }

        let clicked_outside = !just_opened
            && ui.input(|i| i.pointer.any_pressed())
            && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p));
        if let Some(idx) = chosen {
            if let Some(node) = self.spawn_from_registry(idx, qa.world) {
                self.selected_nodes = vec![node];
            }
            return;
        }
        if escape || clicked_outside {
            return;
        }
        self.quick_add = Some(qa);
    }
}

#[cfg(test)]
mod quick_add_tests {
    use super::*;
    use crate::engine::types::new_shared_state;
    use crate::widgets::nodes::ui::button::ButtonWidget;

    fn registry(entries: &[(&str, &str)]) -> Vec<NodeEntry> {
        entries
            .iter()
            .map(|(label, category)| NodeEntry {
                path: crate::widgets::nodes::paths::node_path(category, label),
                label: label.to_string(),
                category: category.to_string(),
                description: "",
                factory: Box::new(|id| Box::new(ButtonWidget::new(id, new_shared_state(1, 1)))),
            })
            .collect()
    }

    fn labels(r: &[NodeEntry], query: &str) -> Vec<String> {
        quick_add_matches(r, query).into_iter().map(|i| r[i].label.clone()).collect()
    }

    #[test]
    fn matches_start_with_names_that_begin_with_the_query_and_every_word_must_match() {
        let r = registry(&[
            ("Sample & Hold", "Signal"),
            ("Scope", "Display"),
            ("Level Meter", "Display"),
            ("Gradient Source", "Color"),
            ("Palette to Gradient", "Color"),
            ("Color Display", "Display"),
        ]);
        assert_eq!(labels(&r, "grad"), ["Gradient Source", "Palette to Gradient"]);
        assert_eq!(labels(&r, "meter"), ["Level Meter"]);
        assert_eq!(labels(&r, "grad pal"), ["Palette to Gradient"], "both words");
        assert_eq!(labels(&r, "COLOR")[0], "Color Display", "the name before the category");
        assert_eq!(labels(&r, "color").len(), 3, "the category matches too");
        assert_eq!(labels(&r, "").len(), 6, "an empty query lists everything");
        assert!(labels(&r, "zzz").is_empty());
    }

    fn paths(r: &[NodeEntry], query: &str) -> Vec<String> {
        quick_add_matches(r, query).into_iter().map(|i| r[i].path.clone()).collect()
    }

    fn nodes() -> Vec<NodeEntry> {
        registry(&[
            ("MIDI Input", "Input"),
            ("MIDI Output", "Output"),
            ("Button", "Input"),
            ("Fader", "Input"),
            ("Mul", "Math"),
            ("Div", "Math"),
            ("Multiplexer", "Route"),
            ("Add", "Math"),
            ("Scope", "Display"),
            ("Clock Divider", "Clock"),
        ])
    }

    #[test]
    fn an_address_finds_a_node_by_its_path() {
        let r = nodes();
        assert_eq!(paths(&r, "math/mul"), ["math/mul"]);
        assert_eq!(paths(&r, "input/midi"), ["input/midi"]);
        assert_eq!(paths(&r, "input/button"), ["input/button"]);
        assert_eq!(paths(&r, "math/div"), ["math/div"]);
        // Each part only has to begin.
        assert_eq!(paths(&r, "in/mid"), ["input/midi"]);
        assert_eq!(paths(&r, "m/d"), ["math/div"]);
    }

    #[test]
    fn a_group_lists_its_nodes_in_order() {
        let r = nodes();
        assert_eq!(paths(&r, "math/"), ["math/add", "math/div", "math/mul"]);
        assert_eq!(paths(&r, "input/"), ["input/button", "input/fader", "input/midi"]);
        assert_eq!(paths(&r, "/mid"), ["input/midi", "output/midi"], "the group may be left open");
        assert_eq!(paths(&r, "math"), ["math/add", "math/div", "math/mul"], "typing just a group works too");
        assert!(paths(&r, "nothing/here").is_empty());
        assert!(paths(&r, "math/mul/extra").is_empty(), "a path has no more parts than it has");
    }

    #[test]
    fn an_address_and_a_plain_word_can_be_mixed() {
        let r = nodes();
        assert_eq!(paths(&r, "input/ mid"), ["input/midi"]);
        // The name still finds a node without its group; the group first on a tie.
        assert_eq!(paths(&r, "mul"), ["math/mul", "route/multiplexer"]);
        assert_eq!(paths(&r, "").len(), r.len());
        assert_eq!(paths(&r, "")[0], "clock/divider", "everything, grouped and sorted by path");
    }

    #[test]
    fn letters_in_order_find_a_node_when_nothing_else_does() {
        let r = registry(&[("Gradient Source", "Color"), ("Scope", "Display")]);
        assert_eq!(labels(&r, "gsrc"), ["Gradient Source"], "an abbreviation");
        assert_eq!(labels(&r, "scp"), ["Scope"]);
        assert!(labels(&r, "gcs").is_empty(), "the letters have to come in order");
    }
}

#[cfg(test)]
mod quick_add_ui_tests {
    use super::*;
    use crate::engine::types::new_shared_state;
    use crate::widgets::nodes::ui::button::ButtonWidget;

    fn frame(g: &mut NodeGraph, ctx: &egui::Context, events: Vec<egui::Event>, just_opened: bool) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 400.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| g.show_quick_add(ui, just_opened));
        });
    }

    fn key(k: egui::Key) -> egui::Event {
        egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }
    }

    #[test]
    fn typing_a_name_and_pressing_enter_adds_that_node_where_it_was_opened() {
        let mut g = NodeGraph::new();
        g.register_node("Input", "Button", |id| Box::new(ButtonWidget::new(id, new_shared_state(1, 1))));
        g.register_node("Input", "Fader", |id| Box::new(crate::widgets::nodes::ui::fader::FaderWidget::new(id, new_shared_state(1, 1))));
        g.quick_add = Some(QuickAdd {
            screen: Pos2::new(100.0, 100.0),
            world: Pos2::new(40.0, 60.0),
            query: String::new(),
            selected: 0,
            focused: false,
        });
        let ctx = egui::Context::default();
        frame(&mut g, &ctx, vec![], true);
        frame(&mut g, &ctx, vec![], false);
        frame(&mut g, &ctx, vec![egui::Event::Text("fad".into())], false);
        assert_eq!(g.quick_add.as_ref().map(|q| q.query.as_str()), Some("fad"), "the text box has the keyboard");
        frame(&mut g, &ctx, vec![key(egui::Key::Enter)], false);
        assert!(g.quick_add.is_none(), "closed after adding");
        let level = g.active();
        assert_eq!(level.nodes.len(), 1);
        assert_eq!(level.nodes[0].type_name(), "Fader");
        assert_eq!(level.states[0].pos, Pos2::new(40.0, 60.0));
        assert_eq!(g.selected_nodes, vec![0]);
    }

    #[test]
    fn escape_closes_it_without_adding_anything() {
        let mut g = NodeGraph::new();
        g.register_node("Input", "Button", |id| Box::new(ButtonWidget::new(id, new_shared_state(1, 1))));
        g.quick_add = Some(QuickAdd { screen: Pos2::new(10.0, 10.0), world: Pos2::ZERO, query: String::new(), selected: 0, focused: false });
        let ctx = egui::Context::default();
        frame(&mut g, &ctx, vec![], true);
        frame(&mut g, &ctx, vec![key(egui::Key::Escape)], false);
        assert!(g.quick_add.is_none());
        assert!(g.active().nodes.is_empty());
    }
}
