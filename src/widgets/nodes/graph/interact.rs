use super::*;

impl NodeGraph {
    /// Find the port under the pointer (if any), push a fresh sample to
    /// the hover ring buffer, and render a tooltip next to the cursor
    /// showing the port's name + current value + a small scope.
    pub(super) fn update_and_show_hover_tooltip(
        &mut self,
        ui: &mut Ui,
        response: &egui::Response,
        node_rects: &[Rect],
    ) {
        self.hover_tooltip_inner(ui, response, node_rects);
        if self.hover_port.is_none() {
            crate::engine::probe::set_target(None);
        }
    }

    fn hover_tooltip_inner(
        &mut self,
        ui: &mut Ui,
        response: &egui::Response,
        node_rects: &[Rect],
    ) {
        // Pointer must be over the canvas and not over a floating area.
        if ui.ctx().is_pointer_over_area() || !response.contains_pointer() {
            self.hover_port = None;
            self.hover_samples.clear();
            return;
        }
        let Some(pointer) = ui.ctx().input(|i| i.pointer.hover_pos()) else {
            self.hover_port = None;
            self.hover_samples.clear();
            return;
        };

        let z = self.active().zoom;
        let radius = (PORT_RADIUS + 4.0) * z;

        // Find hit port (try input first, then output).
        let hit = {
            let level = self.active();
            let mut found: Option<(PortId, String, PortType, usize)> = None;
            'outer: for i in 0..level.nodes.len() {
                if i >= node_rects.len() { break; }
                let rect = node_rects[i];
                let node_id = level.states[i].id;

                let inputs = level.nodes[i].ui_inputs_full();
                let mut base = 0usize;
                for (pi, up) in inputs.iter().enumerate() {
                    if level.nodes[i].is_port_hidden(PortDir::Input, pi) {
                        base += up.def.port_type.channel_count();
                        continue;
                    }
                    let pos = level.nodes[i].port_pos(rect, PortDir::Input, pi, z);
                    if pos.distance(pointer) < radius {
                        found = Some((
                            make_port_id(node_id, PortDir::Input, pi),
                            up.def.name.clone(),
                            up.def.port_type,
                            base,
                        ));
                        break 'outer;
                    }
                    base += up.def.port_type.channel_count();
                }
                let outputs = level.nodes[i].ui_outputs();
                let mut base = 0usize;
                for (pi, up) in outputs.iter().enumerate() {
                    if level.nodes[i].is_port_hidden(PortDir::Output, pi) {
                        base += up.def.port_type.channel_count();
                        continue;
                    }
                    let pos = level.nodes[i].port_pos(rect, PortDir::Output, pi, z);
                    if pos.distance(pointer) < radius {
                        found = Some((
                            make_port_id(node_id, PortDir::Output, pi),
                            up.def.name.clone(),
                            up.def.port_type,
                            base,
                        ));
                        break 'outer;
                    }
                    base += up.def.port_type.channel_count();
                }
            }
            found
        };

        let Some((port_id, port_name, port_type, channel_base)) = hit else {
            self.hover_port = None;
            self.hover_samples.clear();
            return;
        };

        // An unwired input has nothing to show, and the tooltip would sit on
        // top of the port's right-click menu.
        if port_id.dir == PortDir::Input
            && !self.active().connections.iter().any(|c| c.to == port_id)
        {
            self.hover_port = None;
            self.hover_samples.clear();
            return;
        }

        // Reset history if hover moved to a different port.
        if self.hover_port != Some(port_id) {
            self.hover_port = Some(port_id);
            self.hover_samples.clear();
        }

        // Read all channels of this port's current value from shared state.
        let values: Vec<f32> = {
            let level = self.active();
            let Some(idx) = level.states.iter().position(|s| s.id == port_id.node) else {
                return;
            };
            let shared = level.nodes[idx].shared_state().lock().unwrap();
            let arr = match port_id.dir {
                PortDir::Input => &shared.inputs,
                PortDir::Output => &shared.outputs,
            };
            let n = port_type.channel_count();
            (0..n).map(|c| arr.get(channel_base + c).copied().unwrap_or(0.0)).collect()
        };

        // Scope history only makes sense for scalar ports; for composites
        // (Color, Palette, Gradient, Position) the preview renders the
        // whole value shape instead.
        let is_scalar = matches!(port_type,
            PortType::Logic | PortType::Phase | PortType::Untyped | PortType::Any);
        if is_scalar {
            let mut first = values.first().copied().unwrap_or(0.0);
            // Every tick of the engine for the hovered port, not the 16th
            // that the mirror holds, so a pulse is not lost. Only the root
            // level can be probed (inner nodes reuse the ids).
            if self.active_level == 0 {
                crate::engine::probe::set_target(Some(crate::engine::probe::ProbeTarget {
                    node: port_id.node,
                    output: port_id.dir == PortDir::Output,
                    channel: channel_base,
                }));
                let reference = self.hover_samples.back().copied().unwrap_or(first);
                if let Some(v) = crate::engine::probe::most_extreme(&crate::engine::probe::drain(), reference) {
                    first = v;
                }
            } else {
                crate::engine::probe::set_target(None);
            }
            self.hover_samples.push_back(first);
            const SCOPE_SAMPLES: usize = 120;
            while self.hover_samples.len() > SCOPE_SAMPLES {
                self.hover_samples.pop_front();
            }
        } else {
            self.hover_samples.clear();
        }

        let samples: Vec<f32> = self.hover_samples.iter().copied().collect();
        let line_color = port_type.color();
        egui::show_tooltip_at_pointer(
            ui.ctx(),
            ui.layer_id(),
            egui::Id::new("port_scope_tooltip"),
            |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(port_type.color(), "●");
                    ui.strong(&port_name);
                    ui.colored_label(
                        Color32::from_gray(140),
                        match port_id.dir {
                            PortDir::Input => "(in)",
                            PortDir::Output => "(out)",
                        },
                    );
                });
                match port_type {
                    PortType::Color => {
                        ui.label(format!("RGB {:.2} {:.2} {:.2}",
                            values.first().copied().unwrap_or(0.0),
                            values.get(1).copied().unwrap_or(0.0),
                            values.get(2).copied().unwrap_or(0.0)));
                        draw_color_preview(ui, &values);
                    }
                    PortType::Palette => {
                        ui.label("Palette (4 × RGB)");
                        draw_palette_preview(ui, &values);
                    }
                    PortType::Gradient => {
                        ui.label("Gradient (8 stops, α/pos)");
                        draw_gradient_preview(ui, &values);
                    }
                    PortType::Position => {
                        let pan = values.first().copied().unwrap_or(0.0);
                        let tilt = values.get(1).copied().unwrap_or(0.0);
                        ui.label(format!("Pan {:.2}  Tilt {:.2}", pan, tilt));
                        draw_position_preview(ui, pan, tilt);
                    }
                    _ => {
                        let first = values.first().copied().unwrap_or(0.0);
                        ui.label(format!("{:+.4}", first));
                        draw_scope(ui, &samples, line_color);
                    }
                }
            },
        );

        // Keep repainting while hovering so the scope animates without
        // needing any other event to drive the UI.
        ui.ctx().request_repaint();
    }

    /// Set the OS cursor icon based on what the pointer is currently over,
    /// giving pre-action feedback: a port suggests "drag me to spin a
    /// wire", a title bar suggests "grab to move", a resize handle shows
    /// the diagonal arrows, etc. Priority is highest-specificity first
    /// (ports before titles before frame areas before wires) so hovers
    /// over clustered widgets pick the most actionable affordance.
    pub(super) fn update_cursor_icon(
        &self,
        ui: &mut Ui,
        response: &egui::Response,
        node_rects: &[Rect],
    ) {
        if ui.ctx().is_pointer_over_area() || !response.contains_pointer() {
            return;
        }
        let Some(pointer) = ui.ctx().input(|i| i.pointer.hover_pos()) else { return; };
        let z = self.active().zoom;

        // --- Ports on any node: Crosshair = "pull a wire from here".
        let level = self.active();
        for i in 0..level.nodes.len() {
            if i >= node_rects.len() { break; }
            let rect = node_rects[i];
            let radius = (PORT_RADIUS + 4.0) * z;
            let outs = level.nodes[i].ui_outputs();
            for (pi, _) in outs.iter().enumerate() {
                if level.nodes[i].is_port_hidden(PortDir::Output, pi) { continue; }
                let pos = level.nodes[i].port_pos(rect, PortDir::Output, pi, z);
                if pos.distance(pointer) < radius {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                    return;
                }
            }
            let ins = level.nodes[i].ui_inputs_full();
            for (pi, _) in ins.iter().enumerate() {
                if level.nodes[i].is_port_hidden(PortDir::Input, pi) { continue; }
                let pos = level.nodes[i].port_pos(rect, PortDir::Input, pi, z);
                if pos.distance(pointer) < radius {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                    return;
                }
            }
        }

        // --- Node resize handle (bottom-right corner of any resizable node).
        for i in (0..level.nodes.len()).rev() {
            if i >= node_rects.len() { break; }
            if !level.nodes[i].resizable() { continue; }
            let handle_rect = Rect::from_min_size(
                node_rects[i].max - Vec2::splat(RESIZE_HANDLE_SIZE),
                Vec2::splat(RESIZE_HANDLE_SIZE),
            );
            if handle_rect.contains(pointer) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
                return;
            }
        }

        // --- Node title bar: Grab = "drag to move".
        for i in (0..level.nodes.len()).rev() {
            if i >= node_rects.len() { break; }
            let title_rect = title_bar_rect(node_rects[i], &level.nodes[i].layout(), z);
            if title_rect.contains(pointer) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                return;
            }
        }

        // --- Frame chrome: Corner → resize, TitleBar → grab.
        if let Some(hit) = self.frame_hit_at(pointer, self.canvas_rect) {
            match hit {
                FrameHit::Corner(_) => {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
                }
                FrameHit::TitleBar(_) => {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
                FrameHit::Body(_) => {}
            }
            return;
        }

        // --- Wire under pointer: PointingHand = "click to select".
        if self.connection_at(pointer).is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }

    /// Return the connection whose bezier passes near `pointer_pos`, if
    /// any. Tolerance scales with zoom so hit-tests feel consistent at any
    /// zoom level.
    fn connection_at(&self, pointer: Pos2) -> Option<Connection> {
        let level = self.active();
        let z = level.zoom;
        let origin = self.canvas_rect.min.to_vec2() + level.pan;
        let tolerance = (6.0 * z).max(4.0);

        let mut best: Option<(f32, Connection)> = None;
        for conn in &level.connections {
            let (Some(from_pos), Some(to_pos)) = (
                port_screen_pos(&level.nodes, &level.states, conn.from, origin, z),
                port_screen_pos(&level.nodes, &level.states, conn.to, origin, z),
            ) else { continue };
            let pts = bezier_sample(from_pos, to_pos, 20);
            let d = point_to_polyline_distance(pointer, &pts);
            if d <= tolerance
                && best.as_ref().map(|(db, _)| d < *db).unwrap_or(true) {
                    best = Some((d, conn.clone()));
                }
        }
        best.map(|(_, c)| c)
    }

    pub(super) fn handle_interactions(
        &mut self,
        ui: &mut Ui,
        response: &egui::Response,
        node_rects: &[Rect],
        snap_to_grid: bool,
    ) {
        let pointer_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
        // Don't accept canvas interactions while the pointer is over a
        // floating area (Window, Area, popup). This prevents the selection
        // rect from being started when the user is resizing a window whose
        // resize handle sits over the canvas.
        let pointer_over_area = ui.ctx().is_pointer_over_area();
        let canvas_has_pointer = response.contains_pointer() && !pointer_over_area;
        let primary_pressed = canvas_has_pointer
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
        let double_clicked = canvas_has_pointer
            && ui.input(|i| i.pointer.button_double_clicked(egui::PointerButton::Primary));
        let primary_down = ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary));
        let primary_released =
            ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
        let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
        let shift = ui.input(|i| i.modifiers.shift);
        let drag_delta = response.drag_delta();
        let on_canvas = canvas_has_pointer
            && self.canvas_rect.contains(pointer_pos)
            && !self.minimap_rect(self.canvas_rect).contains(pointer_pos);
        let z = self.active().zoom;

        // A click anywhere but on a selected node (ports stick out past the
        // edge) drops a multi-selection, whatever that click goes on to do.
        if primary_pressed
            && on_canvas
            && !ctrl
            && !shift
            && self.drag.drawing_conn.is_none()
            && self.selected_nodes.len() > 1
        {
            let slack = Vec2::new((PORT_RADIUS + 4.0) * z, 0.0);
            let on_selected = self
                .selected_nodes
                .iter()
                .any(|&i| node_rects.get(i).is_some_and(|r| r.expand2(slack).contains(pointer_pos)));
            if !on_selected {
                self.selected_nodes.clear();
            }
        }

        // --- Connection drawing ---
        if self.drag.drawing_conn.is_some() {
            // Bundle-arming: pressing 2..9 (or 0 = 10) while a wire is being
            // drawn marks the drop as a "connect N consecutive ports" action.
            // Esc resets back to single. Subsequent digit keys overwrite.
            let bundle_key = ui.input(|i| {
                use egui::Key::*;
                if i.key_pressed(Escape) { return Some(1); }
                for (k, n) in [
                    (Num2, 2), (Num3, 3), (Num4, 4), (Num5, 5),
                    (Num6, 6), (Num7, 7), (Num8, 8), (Num9, 9), (Num0, 10),
                ] {
                    if i.key_pressed(k) { return Some(n); }
                }
                None
            });
            if let Some(n) = bundle_key
                && let Some(dc) = self.drag.drawing_conn.as_mut() {
                    dc.bundle_size = n;
                }

            let dc = self.drag.drawing_conn.as_mut().unwrap();
            dc.to_pos = pointer_pos;
            dc.snap_target = None;
            let dc_from_dir = dc.from.dir;
            let dc_from_type = dc.from_type;
            let dc_from_node = dc.from.node;

            let level = self.active();
            let mut best_dist = MAGNETIC_RADIUS;
            let mut best_pos = pointer_pos;
            let mut best_target = None;

            for i in 0..level.nodes.len() {
                let (ports, target_dir) = if dc_from_dir == PortDir::Output {
                    (level.nodes[i].ui_inputs_full(), PortDir::Input)
                } else {
                    (level.nodes[i].ui_outputs(), PortDir::Output)
                };
                for (pi, ui_port) in ports.iter().enumerate() {
                    if ui_port.disabled {
                        continue;
                    }
                    let fits = if dc_from_dir == PortDir::Output {
                        dc_from_type.can_feed(&ui_port.def.port_type)
                    } else {
                        ui_port.def.port_type.can_feed(&dc_from_type)
                    };
                    if !fits {
                        continue;
                    }
                    if level.states[i].id == dc_from_node {
                        continue;
                    }
                    if level.nodes[i].is_port_hidden(target_dir, pi) {
                        continue;
                    }
                    let pos = level.nodes[i].port_pos(node_rects[i], target_dir, pi, z);
                    let dist = pos.distance(pointer_pos);
                    if dist < best_dist {
                        best_dist = dist;
                        best_pos = pos;
                        best_target =
                            Some(make_port_id(level.states[i].id, target_dir, pi));
                    }
                }
            }

            let dc = self.drag.drawing_conn.as_mut().unwrap();
            dc.to_pos = best_pos;
            dc.snap_target = best_target;

            if primary_released {
                let snap = dc.snap_target;
                let dc_from = dc.from;
                let dc_from_dir = dc.from.dir;
                let bundle = dc.bundle_size.max(1);
                let unwired_to = dc.unwired_to;
                self.drag.drawing_conn = None;
                if let Some(target) = snap {
                    if bundle <= 1 {
                        let (from, to) = if dc_from_dir == PortDir::Output {
                            (dc_from, target)
                        } else {
                            (target, dc_from)
                        };
                        self.remove_connection_to(to);
                        self.add_connection(from, to);
                    } else {
                        self.connect_bundle(dc_from, target, bundle);
                    }
                } else if bundle > 1 {
                    // Bundle drop on empty canvas after grabbing a connected
                    // input → remove the next N-1 wires too. The grabbed wire
                    // was already removed by the click handler.
                    if let Some(start) = unwired_to {
                        for k in 1..bundle {
                            let port_id = PortId {
                                node: start.node,
                                index: start.index + k,
                                dir: PortDir::Input,
                            };
                            self.remove_connection_to(port_id);
                        }
                    }
                }
            }
            return;
        }

        // --- Selection rectangle (direction-aware) ---
        // Forward drag (start top-left → drag bottom-right): *contain* mode —
        // nodes / wires must be fully inside the rect to be selected.
        // Backward drag (start right, pointer moves left): *crossing* mode —
        // anything the rect touches is selected. Rendered with a dashed
        // outline to flag the different semantic.
        if let Some(start) = self.drag.selection_rect_start {
            if primary_down {
                let sel_rect = Rect::from_two_pos(start, pointer_pos);
                let crossing = pointer_pos.x < start.x;
                self.selected_nodes.clear();
                self.selected_connections.clear();
                for (i, rect) in node_rects.iter().enumerate() {
                    let hit = if crossing {
                        sel_rect.intersects(*rect)
                    } else {
                        rect_contains_rect(sel_rect, *rect)
                    };
                    if hit {
                        self.selected_nodes.push(i);
                    }
                }
                // Connection selection via bezier sampling.
                let z_local = self.active().zoom;
                let origin = self.canvas_rect.min.to_vec2() + self.active().pan;
                let hits: Vec<Connection> = {
                    let level = self.active();
                    let mut out = Vec::new();
                    for conn in &level.connections {
                        let (Some(from_pos), Some(to_pos)) = (
                            port_screen_pos(&level.nodes, &level.states, conn.from, origin, z_local),
                            port_screen_pos(&level.nodes, &level.states, conn.to, origin, z_local),
                        ) else { continue };
                        let pts = bezier_sample(from_pos, to_pos, 20);
                        let hit = if crossing {
                            pts.iter().any(|p| sel_rect.contains(*p))
                        } else {
                            pts.iter().all(|p| sel_rect.contains(*p))
                        };
                        if hit { out.push(conn.clone()); }
                    }
                    out
                };
                for c in hits {
                    self.selected_connections.insert(c);
                }
            }
            if primary_released {
                self.drag.selection_rect_start = None;
            }
            return;
        }

        // --- Check port clicks to start drawing ---
        if primary_pressed && on_canvas {
            let level = self.active();
            for i in 0..level.nodes.len() {
                let rect = node_rects[i];
                let node_id = level.states[i].id;

                let outputs = level.nodes[i].ui_outputs();
                for (pi, ui_port) in outputs.iter().enumerate() {
                    if level.nodes[i].is_port_hidden(PortDir::Output, pi) { continue; }
                    let pos = level.nodes[i].port_pos(rect, PortDir::Output, pi, z);
                    if pos.distance(pointer_pos) < (PORT_RADIUS + 4.0) * z {
                        self.drag.drawing_conn = Some(DrawingConnection {
                            from: make_port_id(node_id, PortDir::Output, pi),
                            from_pos: pos,
                            from_type: ui_port.def.port_type,
                            to_pos: pointer_pos,
                            snap_target: None,
                            bundle_size: 1,
                            unwired_to: None,
                        });
                        return;
                    }
                }
                let in_list = level.nodes[i].ui_inputs_full();
                let input_ports: Vec<(usize, Pos2, PortType)> = in_list
                    .iter()
                    .enumerate()
                    .filter(|(pi, _)| !level.nodes[i].is_port_hidden(PortDir::Input, *pi))
                    .map(|(pi, up)| {
                        let pos = level.nodes[i].port_pos(rect, PortDir::Input, pi, z);
                        (pi, pos, up.def.port_type)
                    })
                    .collect();
                for (pi, pos, pt) in input_ports {
                    if pos.distance(pointer_pos) < (PORT_RADIUS + 4.0) * z {
                        let input_id = make_port_id(node_id, PortDir::Input, pi);
                        if let Some(conn_idx) = level.connections.iter().position(|c| c.to == input_id) {
                            let old_from = level.connections[conn_idx].from;
                            self.remove_connection_to(input_id);
                            let level = self.active();
                            if let Some(from_pos) = port_screen_pos_from_rects(
                                &level.nodes, &level.states, node_rects, old_from, z,
                            ) {
                                self.drag.drawing_conn = Some(DrawingConnection {
                                    from: old_from,
                                    from_pos,
                                    from_type: pt,
                                    to_pos: pointer_pos,
                                    snap_target: None,
                                    bundle_size: 1,
                                    // Track which input we just unwired so a
                                    // bundle-armed drop on empty canvas can
                                    // remove the next N-1 wires too.
                                    unwired_to: Some(input_id),
                                });
                            }
                        } else {
                            self.drag.drawing_conn = Some(DrawingConnection {
                                from: input_id,
                                from_pos: pos,
                                from_type: pt,
                                to_pos: pointer_pos,
                                snap_target: None,
                                bundle_size: 1,
                                unwired_to: None,
                            });
                        }
                        return;
                    }
                }
            }
        }

        // --- Frame drag (move / resize) — must run before the node resize/
        //     selection blocks so an in-progress drag continues across the
        //     frames where the pointer briefly leaves the frame's screen rect.
        if self.frame_drag.is_some() {
            if primary_down {
                let (frame_id, mode, hitched) = {
                    let fd = self.frame_drag.as_ref().unwrap();
                    (fd.frame_id, fd.mode, fd.hitched_nodes.clone())
                };
                let delta = drag_delta / z;
                let level = self.active_mut();
                let frame_exists = level.frames.iter().any(|f| f.id == frame_id);
                if !frame_exists {
                    self.frame_drag = None;
                } else {
                    match mode {
                        FrameDragMode::Move => {
                            if let Some(frame) = level.frames.iter_mut().find(|f| f.id == frame_id) {
                                frame.pos += delta;
                            }
                            // Drag hitched nodes by the same world-space delta.
                            for idx in hitched {
                                if let Some(state) = level.states.get_mut(idx) {
                                    state.pos += delta;
                                }
                            }
                        }
                        FrameDragMode::Resize => {
                            if let Some(frame) = level.frames.iter_mut().find(|f| f.id == frame_id) {
                                frame.size.x = (frame.size.x + delta.x).max(80.0);
                                frame.size.y = (frame.size.y + delta.y).max(40.0);
                            }
                            ui.ctx().set_cursor_icon(CursorIcon::ResizeNwSe);
                        }
                    }
                }
            } else {
                self.frame_drag = None;
            }
        }

        // --- Resize handle ---
        if let Some(idx) = self.drag.resizing_node {
            if primary_down {
                let level = self.active();
                let min = node_size(level.nodes[idx].as_ref(), None);
                let (min_w, min_h) = (min.x, min.y);
                let current = level.states[idx].size_override.unwrap_or(min);
                let new_size = Vec2::new(
                    (current.x + drag_delta.x / z).max(min_w),
                    (current.y + drag_delta.y / z).max(min_h),
                );
                self.active_mut().states[idx].size_override = Some(new_size);
                ui.ctx().set_cursor_icon(CursorIcon::ResizeNwSe);
            } else {
                self.drag.resizing_node = None;
            }
        } else {
            // --- Check resize handle click ---
            if primary_pressed && on_canvas {
                let mut resize_hit = None;
                let level = self.active();
                for i in (0..level.nodes.len()).rev() {
                    if level.nodes[i].resizable() {
                        let handle_rect = Rect::from_min_size(
                            node_rects[i].max - Vec2::splat(RESIZE_HANDLE_SIZE),
                            Vec2::splat(RESIZE_HANDLE_SIZE),
                        );
                        if handle_rect.contains(pointer_pos) {
                            resize_hit = Some(i);
                            break;
                        }
                    }
                }
                if let Some(i) = resize_hit {
                    self.drag.resizing_node = Some(i);
                    if !self.selected_nodes.contains(&i) {
                        self.selected_nodes.clear();
                        self.selected_nodes.push(i);
                    }
                }
            }

            // --- Node selection and dragging ---
            if primary_pressed && on_canvas && !self.drag.dragging_nodes && self.drag.resizing_node.is_none() {
                // --- Title-bar enable/disable toggle hit-test ---
                // Runs before the selection / drag logic so clicks on the
                // toggle never start a title drag.
                {
                    let mut toggle_hit: Option<usize> = None;
                    for i in (0..self.active().nodes.len()).rev() {
                        if i >= node_rects.len() { break; }
                        let layout = self.active().nodes[i].layout();
                        if !layout.toggle { continue; }
                        let t = title_toggle_rect(node_rects[i], layout.title_h, z);
                        if t.contains(pointer_pos) {
                            toggle_hit = Some(i);
                            break;
                        }
                    }
                    if let Some(i) = toggle_hit {
                        let shared = self.active().nodes[i].shared_state().clone();
                        let mut s = shared.lock().unwrap();
                        s.disabled = !s.disabled;
                        return;
                    }
                }

                let mut clicked_node = None;
                let level = self.active();
                for i in (0..level.nodes.len()).rev() {
                    if node_rects[i].contains(pointer_pos) {
                        clicked_node = Some(i);
                        break;
                    }
                }

                if let Some(i) = clicked_node {
                    if ctrl || shift {
                        // Toggle this node's membership in the selection.
                        if let Some(pos) = self.selected_nodes.iter().position(|&x| x == i) {
                            self.selected_nodes.remove(pos);
                        } else {
                            self.selected_nodes.push(i);
                        }
                    } else if !self.selected_nodes.contains(&i) {
                        self.selected_nodes.clear();
                        self.selected_nodes.push(i);
                    }
                    // Double-click on a subgraph node to open it (unless
                    // the subgraph is locked — i.e. a macro instance).
                    if double_clicked
                        && self.active().nodes[i].type_name() == "Subgraph"
                            && let Some(sub) = self.active_mut().nodes[i].as_any_mut().downcast_mut::<SubgraphWidget>()
                                && !sub.locked {
                                    sub.wants_open = true;
                                }

                    let title_rect = title_bar_rect(node_rects[i], &self.active().nodes[i].layout(), z);
                    // Don't start a drag when modifier-clicking — the user is
                    // just toggling selection membership.
                    if title_rect.contains(pointer_pos) && !double_clicked && !shift && !ctrl {
                        self.drag.dragging_nodes = true;
                    } else if self.active().nodes[i].layout().drag_by_body && !double_clicked && !shift && !ctrl {
                        self.drag.body_press = Some(pointer_pos);
                    }
                } else if let Some(hit) = self.frame_hit_at(pointer_pos, self.canvas_rect) {
                    // Click landed on a decorative frame (and no node was hit
                    // first). Selection follows ctrl/shift conventions; drag
                    // semantics depend on which part of the frame was clicked.
                    let id = match hit {
                        FrameHit::TitleBar(i) | FrameHit::Corner(i) | FrameHit::Body(i) => i,
                    };
                    if !ctrl && !shift {
                        self.selected_frames.clear();
                        self.selected_nodes.clear();
                    }
                    self.selected_frames.insert(id);
                    match hit {
                        FrameHit::TitleBar(_) if !double_clicked && !shift && !ctrl => {
                            // Snapshot the nodes currently sitting on top of
                            // this frame; they ride along with the drag.
                            let hitched = self.nodes_on_frame(id, node_rects);
                            self.frame_drag = Some(FrameDrag {
                                frame_id: id,
                                mode: FrameDragMode::Move,
                                hitched_nodes: hitched,
                            });
                        }
                        FrameHit::Corner(_) => {
                            self.frame_drag = Some(FrameDrag {
                                frame_id: id,
                                mode: FrameDragMode::Resize,
                                hitched_nodes: Vec::new(),
                            });
                        }
                        _ => {}
                    }
                } else {
                    // Manual double-click detection on empty canvas:
                    // egui's double_clicked may not fire reliably when the first click
                    // started a drag (e.g., panning). We track our own timer.
                    let now = std::time::Instant::now();
                    let is_double = self.drag.last_canvas_click.is_some_and(|(t, p)| {
                        now.duration_since(t) < std::time::Duration::from_millis(400)
                            && pointer_pos.distance(p) < 6.0
                    });

                    if is_double && self.active_level > 0 {
                        self.drag.panning = false;
                        self.drag.selection_rect_start = None;
                        self.drag.last_canvas_click = None;
                        self.navigate_up();
                        return;
                    }

                    self.drag.last_canvas_click = Some((now, pointer_pos));

                    // Wire click-to-select: if the click lands on (or very
                    // near) a bezier wire, select it and skip the normal
                    // empty-canvas gesture. Ctrl/Shift toggles membership
                    // for compound selections, matching node semantics.
                    let wire_hit = self.connection_at(pointer_pos);
                    if let Some(conn) = wire_hit {
                        if !ctrl && !shift {
                            self.selected_nodes.clear();
                            self.selected_frames.clear();
                            self.selected_connections.clear();
                        }
                        if self.selected_connections.contains(&conn) {
                            self.selected_connections.remove(&conn);
                        } else {
                            self.selected_connections.insert(conn);
                        }
                        return;
                    }

                    // Empty canvas: shift or space + drag = pan, plain drag = selection rect.
                    let space = ui.input(|i| i.key_down(egui::Key::Space));
                    if shift || space {
                        self.drag.panning = true;
                    } else {
                        if !ctrl {
                            self.selected_nodes.clear();
                            self.selected_frames.clear();
                            self.selected_connections.clear();
                        }
                        self.drag.selection_rect_start = Some(pointer_pos);
                    }
                }
            }

            if let Some(start) = self.drag.body_press {
                if !primary_down {
                    self.drag.body_press = None;
                } else if pointer_pos.distance(start) > BODY_DRAG_THRESHOLD {
                    self.drag.body_press = None;
                    self.drag.dragging_nodes = true;
                }
            }

            if self.drag.dragging_nodes {
                if primary_down {
                    let sel = self.selected_nodes.clone();
                    let level = self.active_mut();
                    for &idx in &sel {
                        level.states[idx].pos += drag_delta / z;
                        if snap_to_grid {
                            level.states[idx].pos.x = (level.states[idx].pos.x / GRID_SPACING).round() * GRID_SPACING;
                            level.states[idx].pos.y = (level.states[idx].pos.y / GRID_SPACING).round() * GRID_SPACING;
                        }
                    }
                    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                } else {
                    self.drag.dragging_nodes = false;
                }
            }

            // Panning the canvas via left-click drag.
            if self.drag.panning {
                if primary_down {
                    self.active_mut().pan += drag_delta;
                    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                } else {
                    self.drag.panning = false;
                }
            }
        }

        // Hover cursor for resize handles.
        let level = self.active();
        for i in 0..level.nodes.len() {
            if level.nodes[i].resizable() {
                let handle_rect = Rect::from_min_size(
                    node_rects[i].max - Vec2::splat(RESIZE_HANDLE_SIZE),
                    Vec2::splat(RESIZE_HANDLE_SIZE),
                );
                if handle_rect.contains(pointer_pos) {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeNwSe);
                }
            }
        }

        // Hover cursor and tooltips over ports.
        for i in 0..level.nodes.len() {
            let rect = node_rects[i];
            let outs = level.nodes[i].ui_outputs();
            for (pi, ui_port) in outs.iter().enumerate() {
                if level.nodes[i].is_port_hidden(PortDir::Output, pi) { continue; }
                let pos = level.nodes[i].port_pos(rect, PortDir::Output, pi, z);
                if pos.distance(pointer_pos) < (PORT_RADIUS + 4.0) * z {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    egui::show_tooltip_at(ui.ctx(), ui.layer_id(), egui::Id::new(("port_tip", i, pi, 1)), pos + egui::vec2(10.0, -10.0), |ui| {
                        ui.label(&ui_port.def.name);
                    });
                }
            }
            let ins = level.nodes[i].ui_inputs_full();
            for (pi, ui_port) in ins.iter().enumerate() {
                if level.nodes[i].is_port_hidden(PortDir::Input, pi) { continue; }
                let pos = level.nodes[i].port_pos(rect, PortDir::Input, pi, z);
                if pos.distance(pointer_pos) < (PORT_RADIUS + 4.0) * z {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    egui::show_tooltip_at(ui.ctx(), ui.layer_id(), egui::Id::new(("port_tip", i, pi, 0)), pos + egui::vec2(10.0, -10.0), |ui| {
                        ui.label(&ui_port.def.name);
                    });
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Navigation
    // -----------------------------------------------------------------------
}
