use super::*;

/// What the last full run of `reconcile_effect_stacks` saw, and who it told.
pub(super) struct StackSig {
    hash: u64,
    registry_set: bool,
    /// (level, node index, members) of every stack that was told.
    members: Vec<(usize, usize, Vec<crate::widgets::nodes::node::StackMember>)>,
}

impl NodeGraph {
    /// Names of the Portal Sends anywhere in the graph.
    fn portal_send_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.levels.iter()
            .flat_map(|l| l.nodes.iter())
            .filter(|n| n.type_name() == "Portal Send" && !n.title().is_empty())
            .map(|n| n.title().to_string())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    pub(super) fn show_portal_prompt(&mut self, ctx: &egui::Context) {
        let Some(mut prompt) = self.portal_prompt.take() else { return };
        let taken = self.portal_send_names();
        let mut create: Option<String> = None;
        let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new(if prompt.receive { "Portal Receive" } else { "Portal Send" })
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if prompt.receive {
                    if taken.is_empty() {
                        ui.label("There is no Portal Send yet. Add one first and give it a name.");
                    } else {
                        ui.label("Receive from which Portal Send?");
                        for name in &taken {
                            if ui.button(name).clicked() {
                                create = Some(name.clone());
                            }
                        }
                    }
                } else {
                    ui.label("Name of the portal (a Portal Receive finds it by this name):");
                    let r = ui.text_edit_singleline(&mut prompt.name);
                    if !prompt.focused {
                        r.request_focus();
                        prompt.focused = true;
                    }
                    let name = prompt.name.trim().to_string();
                    let duplicate = taken.contains(&name);
                    if duplicate {
                        ui.colored_label(theme::SEM_DANGER, "A Portal Send with this name exists.");
                    }
                    let ok = !name.is_empty() && !duplicate;
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.add_enabled(ok, egui::Button::new("Add")).clicked() || (enter && ok) {
                        create = Some(name);
                    }
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        if let Some(name) = create {
            let id = self.alloc_id();
            let mut node = (self.registry[prompt.registry_idx].factory)(id);
            node.set_name(&name);
            let idx = self.add_node(node, prompt.pos);
            self.selected_nodes = vec![idx];
        } else if !cancel {
            self.portal_prompt = Some(prompt);
        }
    }

    pub fn set_stack_names(&mut self, names: crate::widgets::nodes::output::effect::SharedStackNames) {
        self.stack_names = Some(names);
    }

    /// A hash of everything `reconcile_effect_stacks` reads: who is a stack,
    /// where it is sent, which node links to it, how effects are labelled and
    /// which groups a root writes to.
    fn stack_links_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for level in &self.levels {
            level.nodes.len().hash(&mut h);
            for node in &level.nodes {
                node.node_id().hash(&mut h);
                let stack = node.stack_name();
                let link = node.linked_stack();
                if stack.is_none() && link.is_none() {
                    continue;
                }
                stack.hash(&mut h);
                link.hash(&mut h);
                node.display_title().hash(&mut h);
                node.effect_kind().hash(&mut h);
                if stack.is_some() && link.is_none() {
                    node.root_groups().hash(&mut h);
                }
            }
        }
        h.finish()
    }

    /// Effects belong to a stack by name, and the stack lists them. Every
    /// frame: move effects along when a stack is renamed, tell each stack
    /// which effects point at it (in any level), and publish the names.
    pub(super) fn reconcile_effect_stacks(&mut self) {
        use crate::widgets::nodes::node::StackMember;

        let mut renames = Vec::new();
        for level in self.levels.iter_mut() {
            for node in level.nodes.iter_mut() {
                if let Some(rename) = node.take_stack_rename() {
                    renames.push(rename);
                }
            }
        }
        for (from, to) in &renames {
            for level in self.levels.iter_mut() {
                for node in level.nodes.iter_mut() {
                    node.rename_linked_stack(from, to);
                }
            }
        }

        let hash = self.stack_links_hash();
        let registry_set = self.stack_names.is_some();
        if let Some(prev) = self.stack_sig.take() {
            if prev.hash == hash && prev.registry_set == registry_set {
                // Nothing that links or lists changed. A stack still gets its
                // members again: that call also notices edits to the groups it
                // writes to, which are not part of the signature.
                for (li, ni, members) in &prev.members {
                    self.levels[*li].nodes[*ni].set_stack_members(members.clone());
                }
                self.stack_sig = Some(prev);
                return;
            }
        }

        let mut stacks: Vec<String> = Vec::new();
        let mut parent_of: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        // (stack it is sent to, what it is, and its own name if it is a stack)
        let mut effects: Vec<(String, StackMember, Option<String>)> = Vec::new();
        for level in &self.levels {
            for node in &level.nodes {
                let stack = node.stack_name();
                if let Some(name) = &stack {
                    stacks.push(name.clone());
                }
                if let Some(link) = node.linked_stack() {
                    if let Some(name) = &stack {
                        parent_of.insert(name.clone(), link.clone());
                    }
                    effects.push((
                        link,
                        StackMember {
                            node_id: node.node_id(),
                            label: node.display_title().to_string(),
                            kind: node.effect_kind(),
                        },
                        stack.clone(),
                    ));
                }
            }
        }
        // A stack sent (even through others) to itself would never end; let it go.
        let looping: Vec<String> = parent_of
            .keys()
            .filter(|start| {
                let mut at = (*start).clone();
                for _ in 0..=parent_of.len() {
                    match parent_of.get(&at) {
                        Some(up) if up == *start => return true,
                        Some(up) => at = up.clone(),
                        None => return false,
                    }
                }
                true
            })
            .cloned()
            .collect();
        for level in self.levels.iter_mut() {
            for node in level.nodes.iter_mut() {
                if node.stack_name().is_some_and(|n| looping.contains(&n)) {
                    node.break_stack_link();
                }
            }
        }
        for name in &looping {
            parent_of.remove(name);
        }
        effects.retain(|(_, _, own)| own.as_ref().is_none_or(|n| !looping.contains(n)));

        // Roots that write to the same group would blend in an arbitrary
        // order; tell each of them who it shares with.
        let roots: Vec<(String, Vec<u32>)> = self
            .levels
            .iter()
            .flat_map(|l| l.nodes.iter())
            .filter(|n| n.linked_stack().is_none())
            .filter_map(|n| n.stack_name().map(|name| (name, n.root_groups())))
            .collect();
        for level in self.levels.iter_mut() {
            for node in level.nodes.iter_mut() {
                let Some(name) = node.stack_name() else { continue };
                let others: Vec<String> = match roots.iter().find(|(n, _)| *n == name) {
                    Some((_, mine)) if node.linked_stack().is_none() => roots
                        .iter()
                        .filter(|(other, groups)| *other != name && groups.iter().any(|g| mine.contains(g)))
                        .map(|(other, _)| other.clone())
                        .collect(),
                    _ => Vec::new(),
                };
                node.set_stack_conflicts(others);
            }
        }

        // A loop was just broken, so the next frame must look again.
        let remember = looping.is_empty();
        if stacks.is_empty() && effects.is_empty() {
            if let Some(reg) = &self.stack_names {
                *reg.lock() = Default::default();
            }
            if remember {
                self.stack_sig = Some(StackSig { hash, registry_set, members: Vec::new() });
            }
            return;
        }
        let mut told = Vec::new();
        for (li, level) in self.levels.iter_mut().enumerate() {
            for (ni, node) in level.nodes.iter_mut().enumerate() {
                if let Some(name) = node.stack_name() {
                    let members: Vec<StackMember> = effects.iter().filter(|(l, _, _)| *l == name).map(|(_, m, _)| m.clone()).collect();
                    told.push((li, ni, members.clone()));
                    node.set_stack_members(members);
                }
            }
        }
        if remember {
            self.stack_sig = Some(StackSig { hash, registry_set, members: told });
        }
        stacks.sort();
        stacks.dedup();
        if let Some(reg) = &self.stack_names {
            let mut reg = reg.lock();
            reg.names = stacks;
            reg.parent_of = parent_of;
        }
    }
}

#[cfg(test)]
mod stack_tree_tests {
    use super::*;
    use crate::engine::types::new_shared_state;
    use crate::widgets::group_context::new_shared_group_context;
    use crate::widgets::nodes::output::effect::new_shared_stack_names;
    use crate::widgets::nodes::output::effect_stack::EffectStackWidget;

    fn graph() -> (NodeGraph, crate::widgets::nodes::output::effect::SharedStackNames) {
        let mut g = NodeGraph::new();
        let names = new_shared_stack_names();
        g.set_stack_names(names.clone());
        let (n, ctx) = (names.clone(), new_shared_group_context());
        g.register_node("Effect", "Effect Stack", move |id| {
            Box::new(EffectStackWidget::new(id, new_shared_state(3, 0), ctx.clone(), n.clone()))
        });
        (g, names)
    }

    /// A stack with a name, an optional parent and some groups.
    fn add(g: &mut NodeGraph, id: u64, name: &str, parent: &str, groups: &[u32]) -> usize {
        let node = g.create_from_registry("Effect Stack", NodeId(id)).unwrap();
        let i = g.add_node(node, Pos2::ZERO);
        let w = g.node_mut(i).as_any_mut().downcast_mut::<EffectStackWidget>().unwrap();
        let mut config = w.config().clone();
        config.name = name.into();
        config.parent = parent.into();
        config.group_ids = groups.to_vec();
        w.set_config(config);
        i
    }

    #[test]
    fn a_stack_is_listed_by_the_stack_it_is_sent_to_and_the_registry_knows_the_tree() {
        let (mut g, names) = graph();
        add(&mut g, 1, "Master", "", &[1]);
        add(&mut g, 2, "Colour", "Master", &[]);
        add(&mut g, 3, "Sparks", "Colour", &[]);
        g.reconcile_effect_stacks();

        let reg = names.lock().clone();
        assert_eq!(reg.parent_of.get("Sparks").map(String::as_str), Some("Colour"));
        // Master may be sent to neither of the stacks below it, which would loop.
        assert_eq!(reg.parents_for("Master"), Vec::<String>::new());
        assert_eq!(reg.parents_for("Colour"), vec!["Master".to_string()]);
        assert_eq!(reg.parents_for("Sparks"), vec!["Colour".to_string(), "Master".to_string()]);
    }

    #[test]
    fn a_loop_is_broken_by_letting_the_stacks_go_of_their_parent() {
        let (mut g, _) = graph();
        let a = add(&mut g, 1, "A", "B", &[1]);
        let b = add(&mut g, 2, "B", "A", &[1]);
        g.reconcile_effect_stacks();
        let parent = |g: &mut NodeGraph, i: usize| g.node_mut(i).as_any_mut().downcast_mut::<EffectStackWidget>().unwrap().config().parent.clone();
        assert_eq!((parent(&mut g, a).as_str(), parent(&mut g, b).as_str()), ("", ""));
        assert_eq!(add(&mut g, 3, "C", "C", &[]), 2);
        g.reconcile_effect_stacks();
        assert_eq!(parent(&mut g, 2), "", "a stack sent to itself too");
    }

    #[test]
    fn roots_on_the_same_group_are_told_about_each_other_but_children_are_not() {
        let (mut g, _) = graph();
        let a = add(&mut g, 1, "A", "", &[1, 2]);
        let b = add(&mut g, 2, "B", "", &[2]);
        let c = add(&mut g, 3, "C", "", &[3]);
        let child = add(&mut g, 4, "D", "A", &[2]);
        g.reconcile_effect_stacks();
        let conflicts = |g: &mut NodeGraph, i: usize| {
            // Read back through the warning the widget would draw.
            g.node_mut(i).as_any_mut().downcast_mut::<EffectStackWidget>().unwrap().conflicts().to_vec()
        };
        assert_eq!(conflicts(&mut g, a), vec!["B".to_string()]);
        assert_eq!(conflicts(&mut g, b), vec!["A".to_string()]);
        assert!(conflicts(&mut g, c).is_empty());
        assert!(conflicts(&mut g, child).is_empty(), "a stack sent elsewhere writes through its parent");
    }

    #[test]
    fn renaming_a_stack_moves_the_stacks_sent_to_it() {
        let (mut g, _) = graph();
        let master = add(&mut g, 1, "Master", "", &[1]);
        let child = add(&mut g, 2, "Colour", "Master", &[]);
        g.node_mut(master).as_any_mut().downcast_mut::<EffectStackWidget>().unwrap().rename("Shelf");
        g.reconcile_effect_stacks();
        let w = g.node_mut(child).as_any_mut().downcast_mut::<EffectStackWidget>().unwrap();
        assert_eq!(w.config().parent, "Shelf");
    }

    #[test]
    fn a_rename_is_still_followed_after_an_unchanged_frame() {
        let (mut g, names) = graph();
        let master = add(&mut g, 1, "Master", "", &[1]);
        let child = add(&mut g, 2, "Colour", "Master", &[]);
        g.reconcile_effect_stacks();
        g.reconcile_effect_stacks();
        assert!(g.stack_sig.is_some(), "the second frame had nothing to do");
        g.node_mut(master).as_any_mut().downcast_mut::<EffectStackWidget>().unwrap().rename("Shelf");
        g.reconcile_effect_stacks();
        let w = g.node_mut(child).as_any_mut().downcast_mut::<EffectStackWidget>().unwrap();
        assert_eq!(w.config().parent, "Shelf");
        assert_eq!(names.lock().parent_of.get("Colour").map(String::as_str), Some("Shelf"));
    }
}
