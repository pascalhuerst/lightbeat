use egui::{self, Color32, CursorIcon, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::node::*;
use super::types::*;
use crate::engine::types::{Connection, NodeId, ParamDef, ParamValue, PortDir, PortId, PortType};

const MAGNETIC_RADIUS: f32 = 20.0;
const BEZIER_SEGMENTS: usize = 40;
const CONNECTION_THICKNESS: f32 = 2.5;
const NODE_CORNER_RADIUS: f32 = 6.0;
const NODE_BG: Color32 = Color32::from_rgb(38, 38, 42);
const NODE_TITLE_BG: Color32 = Color32::from_rgb(50, 50, 56);
const NODE_BORDER: Color32 = Color32::from_rgb(70, 70, 78);
const GRID_COLOR: Color32 = Color32::from_rgb(30, 30, 34);
const GRID_SPACING: f32 = 30.0;

// ---------------------------------------------------------------------------
// Interaction state
// ---------------------------------------------------------------------------

const RESIZE_HANDLE_SIZE: f32 = 10.0;

#[derive(Default)]
struct DragState {
    /// Dragging selected nodes by title bar.
    dragging_nodes: bool,
    drawing_conn: Option<DrawingConnection>,
    /// Selection rectangle (canvas-space start pos).
    selection_rect_start: Option<Pos2>,
    /// Index of node being resized.
    resizing_node: Option<usize>,
}

struct DrawingConnection {
    from: PortId,
    from_pos: Pos2,
    from_type: PortType,
    to_pos: Pos2,
    snap_target: Option<PortId>,
}

// ---------------------------------------------------------------------------
// NodeGraph
// ---------------------------------------------------------------------------

/// Entry in the node registry — a named factory for spawning nodes.
pub struct NodeEntry {
    pub label: String,
    pub category: String,
    pub factory: Box<dyn Fn(NodeId) -> Box<dyn NodeWidget>>,
}

/// Freshly spawned node, returned by `drain_new_nodes` so the app
/// can set up beat-clock subscriptions or other wiring.
pub struct NewNode {
    pub index: usize,
}

/// A copied node snapshot for clipboard operations.
#[allow(dead_code)]
struct ClipboardNode {
    type_name: String,
    size: Option<Vec2>,
    params: Vec<(usize, ParamValue)>,
    data: Option<serde_json::Value>,
    /// Offset from the first copied node's position.
    offset: Vec2,
}

pub struct NodeGraph {
    nodes: Vec<Box<dyn NodeWidget>>,
    states: Vec<NodeState>,
    connections: Vec<Connection>,
    drag: DragState,
    pan: Vec2,
    zoom: f32,
    next_id: u64,
    registry: Vec<NodeEntry>,
    new_nodes: Vec<NewNode>,
    pending_engine_cmds: Vec<EngineCommand>,
    context_menu_pos: Option<Pos2>,
    context_menu_search: String,
    selected_nodes: Vec<usize>,
    canvas_rect: Rect,
    clipboard: Vec<ClipboardNode>,
}

impl NodeGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            states: Vec::new(),
            connections: Vec::new(),
            drag: DragState::default(),
            pan: Vec2::ZERO,
            zoom: 1.0,
            next_id: 1,
            registry: Vec::new(),
            new_nodes: Vec::new(),
            pending_engine_cmds: Vec::new(),
            context_menu_pos: None,
            context_menu_search: String::new(),
            selected_nodes: Vec::new(),
            canvas_rect: Rect::NOTHING,
            clipboard: Vec::new(),
        }
    }

    /// Drain pending engine commands (called by main.rs each frame).
    pub fn drain_engine_commands(&mut self) -> Vec<EngineCommand> {
        std::mem::take(&mut self.pending_engine_cmds)
    }

    /// Register a node type that can be spawned from the context menu.
    pub fn register_node(&mut self, category: impl Into<String>, label: impl Into<String>, factory: impl Fn(NodeId) -> Box<dyn NodeWidget> + 'static) {
        self.registry.push(NodeEntry {
            label: label.into(),
            category: category.into(),
            factory: Box::new(factory),
        });
    }

    /// Remove all registered nodes in a given category.
    pub fn clear_category(&mut self, category: &str) {
        self.registry.retain(|e| e.category != category);
    }

    fn alloc_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn add_node(&mut self, node: Box<dyn NodeWidget>, pos: Pos2) -> usize {
        let id = node.node_id();
        if id.0 >= self.next_id {
            self.next_id = id.0 + 1;
        }
        self.states.push(NodeState::new(id, pos));
        self.nodes.push(node);
        let idx = self.nodes.len() - 1;
        self.new_nodes.push(NewNode { index: idx });
        idx
    }

    /// Drain the list of newly-added nodes (from the context menu).
    /// Call this each frame to wire up subscriptions.
    pub fn drain_new_nodes(&mut self) -> Vec<NewNode> {
        std::mem::take(&mut self.new_nodes)
    }

    /// Get a mutable reference to a node by index (for wiring up state).
    pub fn node_mut(&mut self, index: usize) -> &mut dyn NodeWidget {
        self.nodes[index].as_mut()
    }

    /// Get mutable references to selected nodes (for inspector).
    /// Returns an iterator of &mut Box<dyn NodeWidget>.
    pub fn selected_nodes_mut(&mut self) -> Vec<&mut Box<dyn NodeWidget>> {
        let indices: Vec<usize> = self.selected_nodes.clone();
        let mut result = Vec::new();
        for (i, node) in self.nodes.iter_mut().enumerate() {
            if indices.contains(&i) {
                result.push(node);
            }
        }
        result
    }

    /// Get all nodes for iteration (e.g. to call show_editor on each).
    pub fn nodes_mut(&mut self) -> &mut [Box<dyn NodeWidget>] {
        &mut self.nodes
    }

    /// Get all nodes as shared references.
    pub fn all_nodes(&self) -> &[Box<dyn NodeWidget>] {
        &self.nodes
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn node_and_state(&self, index: usize) -> (&dyn NodeWidget, &NodeState) {
        (self.nodes[index].as_ref(), &self.states[index])
    }

    pub fn connections(&self) -> &[Connection] {
        &self.connections
    }

    #[allow(dead_code)]
    pub fn set_node_size(&mut self, index: usize, size: egui::Vec2) {
        self.states[index].size_override = Some(size);
    }

    /// Create a node from the registry by type name.
    pub fn create_from_registry(&self, type_name: &str, id: NodeId) -> Option<Box<dyn NodeWidget>> {
        self.registry
            .iter()
            .find(|e| e.label == type_name)
            .map(|e| (e.factory)(id))
    }

    pub fn add_connection(&mut self, from: PortId, to: PortId) {
        let conn = Connection { from, to };
        if !self.connections.contains(&conn) {
            self.connections.push(conn.clone());

            // Notify engine.
            self.pending_engine_cmds
                .push(EngineCommand::AddConnection(conn));

            // Notify target widget (UI side, e.g. scope port colors).
            if let (Some(src_idx), Some(dst_idx)) = (
                self.states.iter().position(|s| s.id == from.node),
                self.states.iter().position(|s| s.id == to.node),
            ) {
                let src_type = self.nodes[src_idx]
                    .ui_outputs()
                    .get(from.index)
                    .map(|p| p.def.port_type)
                    .unwrap_or(PortType::Untyped);
                self.nodes[dst_idx].on_ui_connect(to.index, src_type);

                // Notify engine for on_connect callback.
                self.pending_engine_cmds.push(EngineCommand::NotifyConnect {
                    node_id: to.node,
                    input_port: to.index,
                    source_type: src_type,
                });
            }
        }
    }

    fn remove_connection_to(&mut self, to: PortId) {
        let had = self.connections.iter().any(|c| c.to == to);
        self.connections.retain(|c| c.to != to);
        if had {
            // Notify engine.
            self.pending_engine_cmds
                .push(EngineCommand::RemoveConnectionTo(to));
            self.pending_engine_cmds
                .push(EngineCommand::NotifyDisconnect {
                    node_id: to.node,
                    input_port: to.index,
                });

            if let Some(dst_idx) = self.states.iter().position(|s| s.id == to.node) {
                self.nodes[dst_idx].on_ui_disconnect(to.index);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Main draw
    // -----------------------------------------------------------------------

    /// Remove connections that reference ports beyond a node's current port count.
    fn cleanup_stale_connections(&mut self) {
        let stale: Vec<PortId> = self.connections.iter().filter(|conn| {
            let from_ok = self.nodes.iter()
                .zip(self.states.iter())
                .find(|(_, s)| s.id == conn.from.node)
                .map(|(n, _)| conn.from.index < n.ui_outputs().len())
                .unwrap_or(false);
            let to_ok = self.nodes.iter()
                .zip(self.states.iter())
                .find(|(_, s)| s.id == conn.to.node)
                .map(|(n, _)| conn.to.index < n.ui_inputs().len())
                .unwrap_or(false);
            !from_ok || !to_ok
        }).map(|c| c.to).collect();

        for port_id in stale {
            self.remove_connection_to(port_id);
        }
    }

    pub fn show(&mut self, ui: &mut Ui, snap_to_grid: bool) {
        // Clean up stale connections (ports removed by dynamic nodes like Group Output).
        self.cleanup_stale_connections();

        let (response, painter) =
            ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let canvas_rect = response.rect;
        self.canvas_rect = canvas_rect;

        // Zoom with scroll wheel, centered on mouse position.
        if response.contains_pointer() {
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_delta != 0.0 {
                let zoom_factor = 1.0 + scroll_delta * 0.002;
                let new_zoom = (self.zoom * zoom_factor).clamp(0.2, 3.0);

                // Zoom around mouse position.
                if let Some(mouse) = ui.input(|i| i.pointer.hover_pos()) {
                    let mc = mouse.to_vec2() - canvas_rect.min.to_vec2();
                    // Adjust pan so the point under the mouse stays fixed.
                    self.pan = (self.pan - mc) * (new_zoom / self.zoom) + mc;
                }
                self.zoom = new_zoom;
            }
        }

        // Pan with middle mouse.
        if response.dragged_by(egui::PointerButton::Middle) {
            self.pan += response.drag_delta();
        }

        // Right-click opens context menu (only on empty canvas, not on nodes).
        if response.secondary_clicked() {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                self.context_menu_pos = Some(pos);
                self.context_menu_search.clear();
            }
        }

        let z = self.zoom;
        draw_grid(&painter, canvas_rect, self.pan, z);

        let origin = canvas_rect.min.to_vec2() + self.pan;

        // -- Draw connections --
        for conn in &self.connections {
            if let (Some(from_pos), Some(from_type), Some(to_pos)) = (
                port_screen_pos(&self.nodes, &self.states, conn.from, origin, z),
                port_type_for(&self.nodes, &self.states, conn.from),
                port_screen_pos(&self.nodes, &self.states, conn.to, origin, z),
            ) {
                draw_bezier(&painter, from_pos, to_pos, from_type.color(), CONNECTION_THICKNESS);
            }
        }

        // -- Draw in-progress connection --
        if let Some(dc) = &self.drag.drawing_conn {
            let color = dc.from_type.color().linear_multiply(0.7);
            draw_bezier(&painter, dc.from_pos, dc.to_pos, color, CONNECTION_THICKNESS);
            if dc.snap_target.is_some() {
                painter.circle_filled(dc.to_pos, PORT_RADIUS + 3.0, color.linear_multiply(0.3));
            }
        }

        // -- Compute node rects (with zoom) --
        let node_rects: Vec<Rect> = (0..self.nodes.len())
            .map(|i| {
                let n = &self.nodes[i];
                let min_w = n.min_width();
                let inputs = n.ui_inputs();
                let outputs = n.ui_outputs();
                let port_h = ports_height(inputs.len(), outputs.len());
                let content_h = PORT_START_Y + n.min_content_height() + NODE_PADDING;
                let min_h = port_h.max(content_h);
                let size = self.states[i]
                    .size_override
                    .map(|s| Vec2::new(s.x.max(min_w), s.y.max(min_h)))
                    .unwrap_or(Vec2::new(min_w, min_h));
                let pos = Pos2::new(
                    self.states[i].pos.x * z,
                    self.states[i].pos.y * z,
                ) + origin;
                Rect::from_min_size(pos, size * z)
            })
            .collect();

        // -- Draw node chrome (painter-based, immutable) --
        for i in 0..self.nodes.len() {
            let selected = self.selected_nodes.contains(&i);
            let inputs = self.nodes[i].ui_inputs();
            let outputs = self.nodes[i].ui_outputs();
            draw_node_chrome(
                &painter,
                self.nodes[i].title(),
                self.nodes[i].resizable(),
                &inputs,
                &outputs,
                node_rects[i],
                selected,
                z,
            );
        }

        // -- Draw node content (needs &mut node) --
        for i in 0..self.nodes.len() {
            let rect = node_rects[i];
            let content_rect = node_content_rect(rect, z);
            if content_rect.width() > 0.0 && content_rect.height() > 0.0 {
                let mut content_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(content_rect)
                        .layout(egui::Layout::top_down(egui::Align::LEFT)),
                );
                // Scale text and spacing for zoom.
                if (z - 1.0).abs() > 0.01 {
                    let mut style = (**content_ui.style()).clone();
                    for (_, font_id) in style.text_styles.iter_mut() {
                        font_id.size *= z;
                    }
                    style.spacing.item_spacing *= z;
                    style.spacing.button_padding *= z;
                    content_ui.set_style(style);
                }

                self.nodes[i].show_content(&mut content_ui, z);
            }
        }

        // -- Draw selection rectangle --
        if let Some(start) = self.drag.selection_rect_start {
            if let Some(current) = ui.input(|i| i.pointer.hover_pos()) {
                let sel_rect = Rect::from_two_pos(start, current);
                painter.rect_filled(sel_rect, 0.0, Color32::from_rgba_premultiplied(100, 160, 255, 30));
                painter.rect_stroke(sel_rect, 0.0, Stroke::new(1.0, SELECTED_BORDER), StrokeKind::Inside);
            }
        }

        // -- Handle interactions --
        self.handle_interactions(ui, &response, &node_rects, snap_to_grid);

        let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);

        // Only process keyboard shortcuts when no text field has focus.
        let text_has_focus = ui.ctx().memory(|m| m.focused().is_some());

        // -- Delete selected nodes --
        if !text_has_focus
            && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
            && !self.selected_nodes.is_empty()
        {
            self.delete_selected();
        }

        // -- Duplicate (Ctrl+D) --
        if !text_has_focus && ctrl && ui.input(|i| i.key_pressed(egui::Key::D)) && !self.selected_nodes.is_empty() {
            self.duplicate_selected();
        }

        // -- Copy (Ctrl+C) --
        if !text_has_focus && ctrl && ui.input(|i| i.key_pressed(egui::Key::C)) && !self.selected_nodes.is_empty() {
            self.copy_selected();
        }

        // -- Paste (Ctrl+V) --
        if !text_has_focus && ctrl && ui.input(|i| i.key_pressed(egui::Key::V)) && !self.clipboard.is_empty() {
            // Paste at mouse position or center of canvas.
            let pos = ui.input(|i| i.pointer.hover_pos())
                .map(|p| p - canvas_rect.min.to_vec2() - self.pan)
                .unwrap_or(Pos2::new(100.0, 100.0));
            self.paste(pos);
        }

        // -- Context menu --
        self.show_context_menu(ui, canvas_rect);
    }

    fn delete_selected(&mut self) {
        let mut to_remove = self.selected_nodes.clone();
        to_remove.sort_unstable();
        to_remove.dedup();

        let removed_ids: Vec<NodeId> = to_remove.iter().map(|&i| self.states[i].id).collect();

        // Notify engine to remove these nodes.
        for &id in &removed_ids {
            self.pending_engine_cmds
                .push(EngineCommand::RemoveNode(id));
        }

        self.connections
            .retain(|c| !removed_ids.contains(&c.from.node) && !removed_ids.contains(&c.to.node));

        // Remove nodes in reverse order.
        for &i in to_remove.iter().rev() {
            self.nodes.remove(i);
            self.states.remove(i);
        }

        self.selected_nodes.clear();
    }

    fn copy_selected(&mut self) {
        self.clipboard.clear();
        if self.selected_nodes.is_empty() {
            return;
        }

        // Use first selected node's position as origin.
        let origin = self.states[self.selected_nodes[0]].pos;

        for &i in &self.selected_nodes {
            let node = &self.nodes[i];
            let state = &self.states[i];

            // Read params from shared state.
            let shared = node.shared_state().lock().unwrap();
            let params: Vec<(usize, ParamValue)> = shared
                .current_params
                .iter()
                .enumerate()
                .map(|(pi, p)| {
                    let val = match p {
                        ParamDef::Float { value, .. } => ParamValue::Float(*value),
                        ParamDef::Int { value, .. } => ParamValue::Int(*value),
                        ParamDef::Bool { value, .. } => ParamValue::Bool(*value),
                        ParamDef::Choice { value, .. } => ParamValue::Choice(*value),
                    };
                    (pi, val)
                })
                .collect();

            // Read save_data from shared state display (if the display holds serializable data).
            let data = shared
                .display
                .as_ref()
                .and_then(|d| d.downcast_ref::<serde_json::Value>().cloned());
            drop(shared);

            self.clipboard.push(ClipboardNode {
                type_name: node.type_name().to_string(),
                size: state.size_override,
                params,
                data,
                offset: state.pos - origin,
            });
        }
    }

    fn paste(&mut self, base_pos: Pos2) {
        if self.clipboard.is_empty() {
            return;
        }

        self.selected_nodes.clear();
        let clip = std::mem::take(&mut self.clipboard);

        for cn in &clip {
            let id = self.alloc_id();
            if let Some(node) = self.create_from_registry(&cn.type_name, id) {
                let pos = base_pos + cn.offset;
                let idx = self.add_node(node, pos);

                // Apply params by pushing to shared state pending_params.
                {
                    let shared = self.nodes[idx].shared_state();
                    let mut state = shared.lock().unwrap();
                    for (pi, val) in &cn.params {
                        state.pending_params.push((*pi, val.clone()));
                    }
                }

                if let Some(size) = cn.size {
                    self.states[idx].size_override = Some(size);
                }

                self.selected_nodes.push(idx);
            }
        }

        self.clipboard = clip;
    }

    fn duplicate_selected(&mut self) {
        self.copy_selected();
        // Paste offset from the first selected node.
        let base = if let Some(&i) = self.selected_nodes.first() {
            self.states[i].pos + Vec2::new(GRID_SPACING * 2.0, GRID_SPACING * 2.0)
        } else {
            Pos2::new(100.0, 100.0)
        };
        // Clear selection before paste (paste will set new selection).
        self.paste(base);
    }

    fn show_context_menu(&mut self, ui: &mut Ui, canvas_rect: Rect) {
        if self.context_menu_pos.is_none() {
            return;
        }
        let menu_pos = self.context_menu_pos.unwrap();

        // Group entries by category.
        let mut categories: Vec<String> = Vec::new();
        for e in &self.registry {
            if !categories.contains(&e.category) {
                categories.push(e.category.clone());
            }
        }

        if self.registry.is_empty() {
            self.context_menu_pos = None;
            return;
        }

        let mut spawn: Option<usize> = None;
        let search = self.context_menu_search.to_lowercase();

        // Filter entries by search.
        let filtered: Vec<(usize, &str, &str)> = self.registry.iter().enumerate()
            .filter(|(_, e)| search.is_empty() || e.label.to_lowercase().contains(&search) || e.category.to_lowercase().contains(&search))
            .map(|(i, e)| (i, e.category.as_str(), e.label.as_str()))
            .collect();

        let area_resp = egui::Area::new(egui::Id::new("node_ctx_menu"))
            .fixed_pos(menu_pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(180.0);

                    // Search field — auto-focused.
                    let search_resp = ui.add(
                        egui::TextEdit::singleline(&mut self.context_menu_search)
                            .hint_text("Search nodes...")
                            .desired_width(170.0),
                    );
                    if search_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.context_menu_pos = None;
                        self.context_menu_search.clear();
                        return;
                    }
                    // Enter to add the first match.
                    if search_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if let Some(&(idx, _, _)) = filtered.first() {
                            spawn = Some(idx);
                        }
                    }
                    // Auto-focus search on open.
                    search_resp.request_focus();

                    ui.separator();

                    egui::ScrollArea::vertical().max_height(800.0).show(ui, |ui| {
                        if filtered.is_empty() {
                            ui.colored_label(egui::Color32::from_gray(100), "No matches");
                        } else {
                            let mut last_cat = "";
                            for &(idx, cat, label) in &filtered {
                                if cat != last_cat {
                                    if !last_cat.is_empty() { ui.add_space(2.0); }
                                    ui.colored_label(
                                        egui::Color32::from_gray(120),
                                        egui::RichText::new(cat).size(10.0),
                                    );
                                    last_cat = cat;
                                }
                                if ui.button(label).clicked() {
                                    spawn = Some(idx);
                                }
                            }
                        }
                    });
                });
            });

        // Close if clicked outside the popup area.
        let popup_rect = area_resp.response.rect;
        let clicked_outside = ui.input(|i| {
            (i.pointer.button_pressed(egui::PointerButton::Primary)
                || i.pointer.button_pressed(egui::PointerButton::Secondary))
                && i.pointer.hover_pos().is_some_and(|p| !popup_rect.contains(p))
        });
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));

        if let Some(reg_idx) = spawn {
            let id = self.alloc_id();
            let canvas_pos = (menu_pos - canvas_rect.min.to_vec2() - self.pan) / self.zoom;
            let node = (self.registry[reg_idx].factory)(id);
            self.add_node(node, canvas_pos);
            self.context_menu_search.clear();
            self.context_menu_pos = None;
        } else if clicked_outside || esc {
            self.context_menu_pos = None;
            self.context_menu_search.clear();
        }
    }

    // -----------------------------------------------------------------------
    // Interactions
    // -----------------------------------------------------------------------

    fn handle_interactions(
        &mut self,
        ui: &mut Ui,
        response: &egui::Response,
        node_rects: &[Rect],
        snap_to_grid: bool,
    ) {
        let pointer_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
        // Only process interactions if the canvas layer owns the pointer.
        // This prevents interactions when dragging egui Windows over the canvas.
        let canvas_has_pointer = response.contains_pointer();
        let primary_pressed = canvas_has_pointer
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
        let primary_down = ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary));
        let primary_released =
            ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
        let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
        let drag_delta = response.drag_delta();
        let on_canvas = canvas_has_pointer && self.canvas_rect.contains(pointer_pos);

        // --- Connection drawing ---
        if let Some(ref mut dc) = self.drag.drawing_conn {
            dc.to_pos = pointer_pos;
            dc.snap_target = None;

            let mut best_dist = MAGNETIC_RADIUS;
            for i in 0..self.nodes.len() {
                let (ports, target_dir) = if dc.from.dir == PortDir::Output {
                    (self.nodes[i].ui_inputs(), PortDir::Input)
                } else {
                    (self.nodes[i].ui_outputs(), PortDir::Output)
                };
                for (pi, ui_port) in ports.iter().enumerate() {
                    if !dc.from_type.compatible_with(&ui_port.def.port_type) {
                        continue;
                    }
                    if self.states[i].id == dc.from.node {
                        continue;
                    }
                    let pos =
                        port_pos_z(node_rects[i].min, node_rects[i].width(), target_dir, pi, self.zoom);
                    let dist = pos.distance(pointer_pos);
                    if dist < best_dist {
                        best_dist = dist;
                        dc.to_pos = pos;
                        dc.snap_target =
                            Some(make_port_id(self.states[i].id, target_dir, pi));
                    }
                }
            }

            if primary_released {
                if let Some(target) = dc.snap_target {
                    let (from, to) = if dc.from.dir == PortDir::Output {
                        (dc.from, target)
                    } else {
                        (target, dc.from)
                    };
                    self.remove_connection_to(to);
                    self.add_connection(from, to);
                }
                self.drag.drawing_conn = None;
            }
            return;
        }

        // --- Selection rectangle ---
        if let Some(start) = self.drag.selection_rect_start {
            if primary_down {
                // Update selection based on rectangle.
                let sel_rect = Rect::from_two_pos(start, pointer_pos);
                self.selected_nodes.clear();
                for (i, rect) in node_rects.iter().enumerate() {
                    if sel_rect.intersects(*rect) {
                        self.selected_nodes.push(i);
                    }
                }
            }
            if primary_released {
                self.drag.selection_rect_start = None;
            }
            return;
        }

        // --- Check port clicks to start drawing ---
        if primary_pressed && on_canvas {
            for i in 0..self.nodes.len() {
                let rect = node_rects[i];
                let node_id = self.states[i].id;

                let outputs = self.nodes[i].ui_outputs();
                for (pi, ui_port) in outputs.iter().enumerate() {
                    let pos = port_pos_z(rect.min, rect.width(), PortDir::Output, pi, self.zoom);
                    if pos.distance(pointer_pos) < (PORT_RADIUS + 4.0) * self.zoom {
                        self.drag.drawing_conn = Some(DrawingConnection {
                            from: make_port_id(node_id, PortDir::Output, pi),
                            from_pos: pos,
                            from_type: ui_port.def.port_type,
                            to_pos: pointer_pos,
                            snap_target: None,
                        });
                        return;
                    }
                }
                let input_ports: Vec<(usize, Pos2, PortType)> = self.nodes[i]
                    .ui_inputs()
                    .iter()
                    .enumerate()
                    .map(|(pi, up)| {
                        let pos = port_pos_z(rect.min, rect.width(), PortDir::Input, pi, self.zoom);
                        (pi, pos, up.def.port_type)
                    })
                    .collect();
                for (pi, pos, pt) in input_ports {
                    if pos.distance(pointer_pos) < (PORT_RADIUS + 4.0) * self.zoom {
                        let input_id = make_port_id(node_id, PortDir::Input, pi);
                        if let Some(conn_idx) = self.connections.iter().position(|c| c.to == input_id) {
                            let old_from = self.connections[conn_idx].from;
                            self.remove_connection_to(input_id);
                            if let Some(from_pos) = port_screen_pos_from_rects(
                                &self.states, node_rects, old_from, self.zoom,
                            ) {
                                self.drag.drawing_conn = Some(DrawingConnection {
                                    from: old_from,
                                    from_pos,
                                    from_type: pt,
                                    to_pos: pointer_pos,
                                    snap_target: None,
                                });
                            }
                        } else {
                            self.drag.drawing_conn = Some(DrawingConnection {
                                from: input_id,
                                from_pos: pos,
                                from_type: pt,
                                to_pos: pointer_pos,
                                snap_target: None,
                            });
                        }
                        return;
                    }
                }
            }
        }

        // --- Resize handle ---
        if let Some(idx) = self.drag.resizing_node {
            if primary_down {
                let n = &self.nodes[idx];
                let min_w = n.min_width();
                let inputs = n.ui_inputs();
                let outputs = n.ui_outputs();
                let port_h = ports_height(inputs.len(), outputs.len());
                let content_h = PORT_START_Y + n.min_content_height() + NODE_PADDING;
                let min_h = port_h.max(content_h);
                let current = self.states[idx].size_override.unwrap_or(Vec2::new(min_w, min_h));
                let new_size = Vec2::new(
                    (current.x + drag_delta.x / self.zoom).max(min_w),
                    (current.y + drag_delta.y / self.zoom).max(min_h),
                );
                self.states[idx].size_override = Some(new_size);
                ui.ctx().set_cursor_icon(CursorIcon::ResizeNwSe);
            } else {
                self.drag.resizing_node = None;
            }
            // Don't process other interactions while resizing.
        } else {
            // --- Check resize handle click ---
            if primary_pressed && on_canvas {
                for i in (0..self.nodes.len()).rev() {
                    if self.nodes[i].resizable() {
                        let handle_rect = Rect::from_min_size(
                            node_rects[i].max - Vec2::splat(RESIZE_HANDLE_SIZE),
                            Vec2::splat(RESIZE_HANDLE_SIZE),
                        );
                        if handle_rect.contains(pointer_pos) {
                            self.drag.resizing_node = Some(i);
                            if !self.selected_nodes.contains(&i) {
                                self.selected_nodes.clear();
                                self.selected_nodes.push(i);
                            }
                            // Skip normal selection/drag below.
                        }
                    }
                }
            }

            // --- Node selection and dragging ---
            if primary_pressed && on_canvas && !self.drag.dragging_nodes && self.drag.resizing_node.is_none() {
                let mut clicked_node = None;
                for i in (0..self.nodes.len()).rev() {
                    if node_rects[i].contains(pointer_pos) {
                        clicked_node = Some(i);
                        break;
                    }
                }

                if let Some(i) = clicked_node {
                    if ctrl {
                        if let Some(pos) = self.selected_nodes.iter().position(|&x| x == i) {
                            self.selected_nodes.remove(pos);
                        } else {
                            self.selected_nodes.push(i);
                        }
                    } else if !self.selected_nodes.contains(&i) {
                        self.selected_nodes.clear();
                        self.selected_nodes.push(i);
                    }
                    let title_rect = Rect::from_min_size(
                        node_rects[i].min,
                        Vec2::new(node_rects[i].width(), NODE_TITLE_HEIGHT * self.zoom),
                    );
                    if title_rect.contains(pointer_pos) {
                        self.drag.dragging_nodes = true;
                    }
                } else {
                    if !ctrl {
                        self.selected_nodes.clear();
                    }
                    self.drag.selection_rect_start = Some(pointer_pos);
                }
            }

            if self.drag.dragging_nodes {
                if primary_down {
                    for &idx in &self.selected_nodes {
                        self.states[idx].pos += drag_delta / self.zoom;
                        if snap_to_grid {
                            self.states[idx].pos.x = (self.states[idx].pos.x / GRID_SPACING).round() * GRID_SPACING;
                            self.states[idx].pos.y = (self.states[idx].pos.y / GRID_SPACING).round() * GRID_SPACING;
                        }
                    }
                    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                } else {
                    self.drag.dragging_nodes = false;
                }
            }
        }

        // Hover cursor for resize handles.
        for i in 0..self.nodes.len() {
            if self.nodes[i].resizable() {
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
        for i in 0..self.nodes.len() {
            let rect = node_rects[i];
            for (pi, ui_port) in self.nodes[i].ui_outputs().iter().enumerate() {
                let pos = port_pos_z(rect.min, rect.width(), PortDir::Output, pi, self.zoom);
                if pos.distance(pointer_pos) < (PORT_RADIUS + 4.0) * self.zoom {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    egui::show_tooltip_at(ui.ctx(), ui.layer_id(), egui::Id::new(("port_tip", i, pi, 1)), pos + egui::vec2(10.0, -10.0), |ui| {
                        ui.label(&ui_port.def.name);
                    });
                }
            }
            for (pi, ui_port) in self.nodes[i].ui_inputs().iter().enumerate() {
                let pos = port_pos_z(rect.min, rect.width(), PortDir::Input, pi, self.zoom);
                if pos.distance(pointer_pos) < (PORT_RADIUS + 4.0) * self.zoom {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    egui::show_tooltip_at(ui.ctx(), ui.layer_id(), egui::Id::new(("port_tip", i, pi, 0)), pos + egui::vec2(10.0, -10.0), |ui| {
                        ui.label(&ui_port.def.name);
                    });
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Free functions (no &self needed)
// ---------------------------------------------------------------------------

/// Get port screen pos using precomputed node rects (for use during interaction handling).
fn port_screen_pos_from_rects(
    states: &[NodeState],
    node_rects: &[Rect],
    port_id: PortId,
    zoom: f32,
) -> Option<Pos2> {
    let idx = states.iter().position(|s| s.id == port_id.node)?;
    let rect = node_rects[idx];
    Some(port_pos_z(rect.min, rect.width(), port_id.dir, port_id.index, zoom))
}

fn node_content_rect(rect: Rect, zoom: f32) -> Rect {
    Rect::from_min_max(
        Pos2::new(rect.min.x + (PORT_RADIUS + 8.0) * zoom, rect.min.y + PORT_START_Y * zoom),
        Pos2::new(rect.max.x - (PORT_RADIUS + 8.0) * zoom, rect.max.y - NODE_PADDING * zoom),
    )
}

const SELECTED_BORDER: Color32 = Color32::from_rgb(100, 160, 255);

fn draw_node_chrome(
    painter: &Painter,
    title: &str,
    resizable: bool,
    inputs: &[UiPortDef],
    outputs: &[UiPortDef],
    rect: Rect,
    selected: bool,
    zoom: f32,
) {
    // Shadow
    let shadow_rect = rect.translate(Vec2::new(3.0, 3.0));
    painter.rect_filled(shadow_rect, NODE_CORNER_RADIUS, Color32::from_black_alpha(60));

    // Body
    painter.rect_filled(rect, NODE_CORNER_RADIUS, NODE_BG);
    let border = if selected { Stroke::new(2.0, SELECTED_BORDER) } else { Stroke::new(1.0, NODE_BORDER) };
    painter.rect_stroke(rect, NODE_CORNER_RADIUS, border, StrokeKind::Inside);

    // Title bar
    let title_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), NODE_TITLE_HEIGHT * zoom));
    painter.rect_filled(
        title_rect,
        egui::CornerRadius { nw: NODE_CORNER_RADIUS as u8, ne: NODE_CORNER_RADIUS as u8, sw: 0, se: 0 },
        NODE_TITLE_BG,
    );
    painter.text(
        title_rect.center(),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(13.0 * zoom),
        Color32::WHITE,
    );

    // Input ports
    for (i, ui_port) in inputs.iter().enumerate() {
        let pos = port_pos_z(rect.min, rect.width(), PortDir::Input, i, zoom);
        draw_port(painter, pos, ui_port, zoom);
    }

    // Output ports
    for (i, ui_port) in outputs.iter().enumerate() {
        let pos = port_pos_z(rect.min, rect.width(), PortDir::Output, i, zoom);
        draw_port(painter, pos, ui_port, zoom);
    }

    // Resize handle (small triangle in bottom-right corner)
    if resizable {
        let s = RESIZE_HANDLE_SIZE;
        let br = rect.max;
        let handle_color = Color32::from_gray(80);
        painter.line_segment(
            [Pos2::new(br.x - s, br.y), Pos2::new(br.x, br.y - s)],
            Stroke::new(1.0, handle_color),
        );
        painter.line_segment(
            [Pos2::new(br.x - s * 0.5, br.y), Pos2::new(br.x, br.y - s * 0.5)],
            Stroke::new(1.0, handle_color),
        );
    }
}

fn draw_port(painter: &Painter, pos: Pos2, ui_port: &UiPortDef, zoom: f32) {
    let type_color = ui_port.def.port_type.color();
    let fill = ui_port.fill_color.unwrap_or(type_color);
    let stroke_width = if ui_port.fill_color.is_some() { 3.0 } else { 1.5 };
    let r = PORT_RADIUS * zoom;
    painter.circle_filled(pos, r, fill);
    painter.circle_stroke(pos, r, Stroke::new(stroke_width, type_color));
}

fn draw_grid(painter: &Painter, rect: Rect, pan: Vec2, zoom: f32) {
    painter.rect_filled(rect, 0.0, Color32::from_rgb(22, 22, 26));

    let spacing = GRID_SPACING * zoom;
    let offset_x = pan.x.rem_euclid(spacing);
    let offset_y = pan.y.rem_euclid(spacing);

    let mut x = rect.min.x + offset_x;
    while x < rect.max.x {
        painter.line_segment(
            [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
            Stroke::new(1.0, GRID_COLOR),
        );
        x += spacing;
    }
    let mut y = rect.min.y + offset_y;
    while y < rect.max.y {
        painter.line_segment(
            [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
            Stroke::new(1.0, GRID_COLOR),
        );
        y += spacing;
    }
}

fn draw_bezier(painter: &Painter, from: Pos2, to: Pos2, color: Color32, thickness: f32) {
    let dx = (to.x - from.x).abs() * 0.5;
    let ctrl1 = Pos2::new(from.x + dx, from.y);
    let ctrl2 = Pos2::new(to.x - dx, to.y);

    let mut points = Vec::with_capacity(BEZIER_SEGMENTS + 1);
    for i in 0..=BEZIER_SEGMENTS {
        let t = i as f32 / BEZIER_SEGMENTS as f32;
        let inv = 1.0 - t;
        let x = inv * inv * inv * from.x
            + 3.0 * inv * inv * t * ctrl1.x
            + 3.0 * inv * t * t * ctrl2.x
            + t * t * t * to.x;
        let y = inv * inv * inv * from.y
            + 3.0 * inv * inv * t * ctrl1.y
            + 3.0 * inv * t * t * ctrl2.y
            + t * t * t * to.y;
        points.push(Pos2::new(x, y));
    }

    for w in points.windows(2) {
        painter.line_segment([w[0], w[1]], Stroke::new(thickness, color));
    }
}

fn port_screen_pos(
    nodes: &[Box<dyn NodeWidget>],
    states: &[NodeState],
    port_id: PortId,
    origin: Vec2,
    zoom: f32,
) -> Option<Pos2> {
    let idx = states.iter().position(|s| s.id == port_id.node)?;
    let pos = Pos2::new(states[idx].pos.x * zoom, states[idx].pos.y * zoom) + origin;
    let width = states[idx].size_override
        .map(|s| s.x)
        .unwrap_or(nodes[idx].min_width()) * zoom;
    Some(port_pos_z(pos, width, port_id.dir, port_id.index, zoom))
}

fn port_type_for(
    nodes: &[Box<dyn NodeWidget>],
    states: &[NodeState],
    port_id: PortId,
) -> Option<PortType> {
    let idx = states.iter().position(|s| s.id == port_id.node)?;
    let ports = match port_id.dir {
        PortDir::Input => nodes[idx].ui_inputs(),
        PortDir::Output => nodes[idx].ui_outputs(),
    };
    ports.get(port_id.index).map(|p| p.def.port_type)
}
