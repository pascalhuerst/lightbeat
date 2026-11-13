use super::*;

impl NodeGraph {
    /// True if `screen_pos` lies inside any currently-selected node's screen rect.
    pub(super) fn is_pos_on_selected_node(&self, screen_pos: Pos2, canvas_rect: Rect) -> bool {
        let level = self.active();
        let z = level.zoom;
        let origin = canvas_rect.min.to_vec2() + level.pan;
        for &i in &self.selected_nodes {
            if i >= level.nodes.len() { continue; }
            let size = node_size(level.nodes[i].as_ref(), level.states[i].size_override);
            let pos = Pos2::new(level.states[i].pos.x * z, level.states[i].pos.y * z) + origin;
            let rect = Rect::from_min_size(pos, size * z);
            if rect.contains(screen_pos) {
                return true;
            }
        }
        false
    }

    /// The port under `pointer`, if any, with the menu to open for it.
    pub(super) fn port_menu_at(&self, pointer: Pos2, node_rects: &[Rect]) -> Option<PortMenu> {
        let level = self.active();
        let z = level.zoom;
        let radius = (PORT_RADIUS + 4.0) * z;
        for i in (0..level.nodes.len()).rev() {
            if i >= node_rects.len() { break; }
            let id = level.states[i].id;
            for dir in [PortDir::Input, PortDir::Output] {
                let ports = match dir {
                    PortDir::Input => level.nodes[i].ui_inputs_full(),
                    PortDir::Output => level.nodes[i].ui_outputs(),
                };
                for (pi, port) in ports.iter().enumerate() {
                    if level.nodes[i].is_port_hidden(dir, pi) { continue; }
                    let pos = level.nodes[i].port_pos(node_rects[i], dir, pi, z);
                    if pos.distance(pointer) < radius {
                        return Some(PortMenu {
                            port: make_port_id(id, dir, pi),
                            name: port.def.name.clone(),
                            port_type: port.def.port_type,
                            pos: pointer,
                        });
                    }
                }
            }
        }
        None
    }

    /// Node types worth attaching to a port: controls feed an input,
    /// displays read an output. Only those whose port fits are offered.
    fn port_menu_choices(&self, menu: &PortMenu) -> Vec<&'static str> {
        let wanted: &[&str] = match (menu.port.dir, menu.port_type) {
            (PortDir::Input, PortType::Logic) => &["Button"],
            (PortDir::Input, PortType::Untyped) => &["Const Value", "Fader"],
            (PortDir::Input, PortType::Phase) => &["Const Phase", "Fader"],
            (PortDir::Input, PortType::Any) => &["Const Value", "Button", "Fader"],
            (PortDir::Input, PortType::Gradient) => &["Gradient Stops"],
            (PortDir::Input, PortType::Color) => &["Color Mixer"],
            (PortDir::Output, PortType::Logic) => &["LED Display", "Scope"],
            (PortDir::Output, PortType::Untyped | PortType::Phase | PortType::Any) => {
                &["Value Display", "Scope"]
            }
            (PortDir::Output, PortType::Color) => &["Color Display"],
            _ => &[],
        };
        wanted
            .iter()
            .copied()
            .filter(|name| {
                let Some(sample) = self.create_from_registry(name, NodeId(0)) else { return false };
                let facing = match menu.port.dir {
                    PortDir::Input => sample.ui_outputs(),
                    PortDir::Output => sample.ui_inputs_full(),
                };
                facing.first().is_some_and(|p| match menu.port.dir {
                    PortDir::Input => p.def.port_type.can_feed(&menu.port_type),
                    PortDir::Output => menu.port_type.can_feed(&p.def.port_type),
                })
            })
            .collect()
    }

    pub(super) fn show_port_menu(&mut self, ui: &mut Ui, just_opened: bool) {
        let Some(menu) = self.port_menu.as_ref() else { return };
        let choices = self.port_menu_choices(menu);
        if choices.is_empty() {
            self.port_menu = None;
            return;
        }
        let (pos, port, name) = (menu.pos, menu.port, menu.name.clone());
        let mut chosen = None;
        let area = egui::Area::new(egui::Id::new("port_ctx_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(140.0);
                    if !name.is_empty() {
                        ui.label(egui::RichText::new(&name).weak());
                    }
                    for c in &choices {
                        if ui.button(format!("Add {c}")).clicked() {
                            chosen = Some(*c);
                        }
                    }
                });
            });
        if let Some(type_name) = chosen {
            self.attach_to_port(port, &name, type_name);
            self.port_menu = None;
            return;
        }
        if !just_opened
            && ui.input(|i| i.pointer.any_pressed())
            && let Some(p) = ui.input(|i| i.pointer.interact_pos())
            && !area.response.rect.contains(p)
        {
            self.port_menu = None;
        }
    }

    /// Put a new node next to `port`'s node, wire it up and name it after the port.
    fn attach_to_port(&mut self, port: PortId, port_name: &str, type_name: &str) {
        let id = self.alloc_id();
        let Some(mut node) = self.create_from_registry(type_name, id) else { return };
        if !port_name.is_empty() {
            node.set_name(port_name);
        }
        let level = self.active();
        let Some(target) = level.states.iter().position(|s| s.id == port.node) else { return };
        if port.dir == PortDir::Input {
            node.set_initial_value(level.nodes[target].neutral_input_value(port.index));
        }
        let target_pos = level.states[target].pos;
        let target_size = node_size(level.nodes[target].as_ref(), level.states[target].size_override);
        let port_y = |n: &dyn NodeWidget, size: Vec2, dir, index| -> f32 {
            let rect = Rect::from_min_size(Pos2::ZERO, size);
            n.port_pos(rect, dir, index, 1.0).y
        };
        let new_size = node_size(node.as_ref(), None);
        let (new_dir, x) = match port.dir {
            PortDir::Input => (PortDir::Output, target_pos.x - ATTACH_GAP - new_size.x),
            PortDir::Output => (PortDir::Input, target_pos.x + target_size.x + ATTACH_GAP),
        };
        let y = target_pos.y + port_y(level.nodes[target].as_ref(), target_size, port.dir, port.index)
            - port_y(node.as_ref(), new_size, new_dir, 0);
        let new_id = node.node_id();
        self.add_node(node, Pos2::new(x, y));
        let new_port = make_port_id(new_id, new_dir, 0);
        match port.dir {
            PortDir::Input => self.add_connection(new_port, port),
            PortDir::Output => self.add_connection(port, new_port),
        }
    }

    pub(super) fn show_context_menu(&mut self, ui: &mut Ui, _canvas_rect: Rect, just_opened: bool) {
        if self.context_menu_pos.is_none() {
            return;
        }
        let menu_pos = self.context_menu_pos.unwrap();

        if self.selected_nodes.is_empty() {
            self.context_menu_pos = None;
            return;
        }

        // Pre-compute the auto-connect plan (only set when 2 nodes selected
        // and there's at least one valid pairwise connection).
        let plan = self.auto_connect_plan();

        // Macro-context info: when exactly one Subgraph is selected,
        // show "Save as macro..." + "Embed". Embed is a no-op if the
        // subgraph isn't actually locked (cheap and harmless).
        let single_subgraph: Option<(NodeId, bool)> = if self.selected_nodes.len() == 1 {
            let i = self.selected_nodes[0];
            let level = self.active_mut();
            if i < level.nodes.len() && level.nodes[i].type_name() == "Subgraph" {
                let id = level.states[i].id;
                let locked = level.nodes[i].as_any_mut()
                    .downcast_mut::<SubgraphWidget>()
                    .map(|w| w.locked)
                    .unwrap_or(false);
                Some((id, locked))
            } else { None }
        } else { None };

        let hide_label = {
            let level = self.active();
            let all_hidden = self
                .selected_nodes
                .iter()
                .filter_map(|&i| level.nodes.get(i))
                .all(|n| n.shared_state().lock().unwrap().hide_unused_ports);
            if all_hidden { "Show all ports" } else { "Hide unused ports" }
        };
        let mut toggle_hide = false;
        // Compact nodes have no enable switch in their header, so the menu is
        // the one place every node can be frozen from.
        let all_disabled = {
            let level = self.active();
            self.selected_nodes
                .iter()
                .filter_map(|&i| level.nodes.get(i))
                .all(|n| n.shared_state().lock().unwrap().disabled)
        };
        let mut toggle_enabled = false;
        let mut move_to_sub = false;
        let mut do_auto_connect = false;
        let mut do_save_macro: Option<NodeId> = None;
        let mut do_embed: Option<NodeId> = None;
        let mut do_unpack: Option<usize> = None;
        let area_resp = egui::Area::new(egui::Id::new("selection_ctx_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu_pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(160.0);
                    if let Some((_, _, pairs)) = &plan {
                        let label = if pairs.len() == 1 {
                            "Connect 1 port".to_string()
                        } else {
                            format!("Connect {} ports", pairs.len())
                        };
                        if ui.button(label).clicked() {
                            do_auto_connect = true;
                        }
                        ui.separator();
                    }
                    if ui.button(hide_label).on_hover_text("Show only the ports that have a wire (H)").clicked() {
                        toggle_hide = true;
                    }
                    if ui.button(if all_disabled { "Enable" } else { "Disable" })
                        .on_hover_text("A disabled node keeps its last output")
                        .clicked()
                    {
                        toggle_enabled = true;
                    }
                    if ui.button("Move to Subgraph").clicked() {
                        move_to_sub = true;
                    }
                    if let Some((id, locked)) = single_subgraph {
                        ui.separator();
                        if !locked && ui.button("Save as macro...").clicked() {
                            do_save_macro = Some(id);
                        }
                        if locked && ui.button("Embed").clicked() {
                            do_embed = Some(id);
                        }
                        if ui.button("Unpack").on_hover_text(
                            "Replace the subgraph with its inner nodes, \
                             reconnecting inputs and outputs").clicked()
                        {
                            do_unpack = Some(self.selected_nodes[0]);
                        }
                    }
                });
            });
        if do_auto_connect {
            if let Some((src_id, dst_id, pairs)) = plan {
                for (o, i) in pairs {
                    let from = PortId { node: src_id, index: o, dir: PortDir::Output };
                    let to = PortId { node: dst_id, index: i, dir: PortDir::Input };
                    self.add_connection(from, to);
                }
            }
            self.context_menu_pos = None;
            return;
        }
        if toggle_enabled {
            let level = self.active();
            for n in self.selected_nodes.iter().filter_map(|&i| level.nodes.get(i)) {
                n.shared_state().lock().unwrap().disabled = !all_disabled;
            }
            self.context_menu_pos = None;
            return;
        }
        if toggle_hide {
            self.toggle_hide_unused_ports();
            self.context_menu_pos = None;
            return;
        }
        if move_to_sub {
            self.move_selection_to_subgraph();
            self.context_menu_pos = None;
            return;
        }
        if let Some(id) = do_save_macro {
            let path = self.current_subgraph_path();
            self.pending_macro_request = Some(MacroRequest::SaveAs {
                node_id: id,
                subgraph_path: path,
            });
            self.context_menu_pos = None;
            return;
        }
        if let Some(id) = do_embed {
            // Find the subgraph widget and clear its locked flag.
            let level = self.active_mut();
            if let Some(idx) = level.states.iter().position(|s| s.id == id)
                && let Some(w) = level.nodes[idx].as_any_mut().downcast_mut::<SubgraphWidget>() {
                    w.locked = false;
                    w.push_config();
                }
            self.context_menu_pos = None;
            return;
        }
        if let Some(sub_idx) = do_unpack {
            self.unpack_subgraph(sub_idx);
            self.context_menu_pos = None;
            return;
        }
        self.dismiss_on_outside_click(ui, area_resp.response.rect, just_opened);
    }

    // -----------------------------------------------------------------------
    // Interactions
    // -----------------------------------------------------------------------
}

#[cfg(test)]
mod port_menu_tests {
    use super::*;
    use crate::engine::types::new_shared_state;
    use crate::widgets::nodes::display::led_display::LedDisplayWidget;
    use crate::widgets::nodes::math::constant::ConstantWidget;
    use crate::widgets::nodes::ui::button::ButtonWidget;

    fn graph() -> NodeGraph {
        let mut g = NodeGraph::new();
        g.register_node("UI", "Button", |id| Box::new(ButtonWidget::new(id, new_shared_state(1, 1))));
        g.register_node("Math", "Const Value", |id| {
            Box::new(ConstantWidget::new(id, PortType::Untyped, new_shared_state(0, 1)))
        });
        g.register_node("Display", "LED Display", |id| {
            Box::new(LedDisplayWidget::new(id, new_shared_state(1, 0)))
        });
        g
    }

    fn menu(port: PortId, ty: PortType) -> PortMenu {
        PortMenu { port, name: "go".into(), port_type: ty, pos: Pos2::ZERO }
    }

    #[test]
    fn a_trigger_input_offers_a_button_and_an_output_an_led() {
        let g = graph();
        let id = PortId { node: NodeId(1), index: 0, dir: PortDir::Input };
        assert_eq!(g.port_menu_choices(&menu(id, PortType::Logic)), vec!["Button"]);
        assert_eq!(g.port_menu_choices(&menu(id, PortType::Untyped)), vec!["Const Value"]);
        assert!(g.port_menu_choices(&menu(id, PortType::Color)).is_empty());
        let out = PortId { dir: PortDir::Output, ..id };
        assert_eq!(g.port_menu_choices(&menu(out, PortType::Logic)), vec!["LED Display"]);
    }

    #[test]
    fn attaching_a_button_wires_it_to_the_left_and_names_it_after_the_port() {
        let mut g = graph();
        let target = g.alloc_id();
        let led = g.create_from_registry("LED Display", target).unwrap();
        g.add_node(led, Pos2::new(400.0, 100.0));
        let port = PortId { node: target, index: 0, dir: PortDir::Input };
        g.attach_to_port(port, "go", "Button");
        let level = g.active();
        assert_eq!(level.nodes.len(), 2);
        assert_eq!(level.nodes[1].title(), "go");
        assert!(level.states[1].pos.x < 400.0);
        assert_eq!(level.connections.len(), 1);
        assert_eq!(level.connections[0].to, port);
        assert_eq!(level.connections[0].from.node, level.states[1].id);
    }
}
