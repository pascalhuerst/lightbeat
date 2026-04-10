use egui::{
    self, Color32, CursorIcon, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use super::node::*;
use super::types::*;

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

#[derive(Default)]
struct DragState {
    dragging_node: Option<usize>,
    drawing_conn: Option<DrawingConnection>,
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
    pub label: &'static str,
    pub factory: Box<dyn Fn(NodeId) -> Box<dyn NodeWidget>>,
}

/// Freshly spawned node, returned by `drain_new_nodes` so the app
/// can set up beat-clock subscriptions or other wiring.
pub struct NewNode {
    pub index: usize,
    pub node_id: NodeId,
}

pub struct NodeGraph {
    nodes: Vec<Box<dyn NodeWidget>>,
    states: Vec<NodeState>,
    connections: Vec<Connection>,
    drag: DragState,
    pan: Vec2,
    next_id: u64,
    registry: Vec<NodeEntry>,
    new_nodes: Vec<NewNode>,
    context_menu_pos: Option<Pos2>,
    selected_node: Option<usize>,
}

impl NodeGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            states: Vec::new(),
            connections: Vec::new(),
            drag: DragState::default(),
            pan: Vec2::ZERO,
            next_id: 1,
            registry: Vec::new(),
            new_nodes: Vec::new(),
            context_menu_pos: None,
            selected_node: None,
        }
    }

    /// Register a node type that can be spawned from the context menu.
    pub fn register_node(&mut self, label: &'static str, factory: impl Fn(NodeId) -> Box<dyn NodeWidget> + 'static) {
        self.registry.push(NodeEntry { label, factory: Box::new(factory) });
    }

    fn alloc_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn add_node(&mut self, node: Box<dyn NodeWidget>, pos: Pos2) -> usize {
        let id = node.node_id();
        // Keep next_id above any manually-assigned IDs.
        if id.0 >= self.next_id {
            self.next_id = id.0 + 1;
        }
        self.states.push(NodeState::new(id, pos));
        self.nodes.push(node);
        self.nodes.len() - 1
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

    /// Get a mutable reference to the selected node, if any.
    pub fn selected_node_mut(&mut self) -> Option<&mut Box<dyn NodeWidget>> {
        self.selected_node.and_then(|i| self.nodes.get_mut(i))
    }

    /// Title of the selected node, if any.
    pub fn selected_node_title(&self) -> Option<&str> {
        self.selected_node.map(|i| self.nodes[i].title())
    }

    pub fn add_connection(&mut self, from: PortId, to: PortId) {
        let conn = Connection { from, to };
        if !self.connections.contains(&conn) {
            self.connections.push(conn);
        }
    }

    // -----------------------------------------------------------------------
    // Main draw
    // -----------------------------------------------------------------------

    pub fn show(&mut self, ui: &mut Ui) {
        // Propagate signals through connections before drawing.
        self.propagate_signals();

        let (response, painter) =
            ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let canvas_rect = response.rect;

        // Pan with middle mouse.
        if response.dragged_by(egui::PointerButton::Middle) {
            self.pan += response.drag_delta();
        }

        // Right-click opens context menu (only on empty canvas, not on nodes).
        if response.secondary_clicked() {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                self.context_menu_pos = Some(pos);
            }
        }

        draw_grid(&painter, canvas_rect, self.pan);

        let origin = canvas_rect.min.to_vec2() + self.pan;

        // -- Draw connections --
        for conn in &self.connections {
            if let (Some(from_pos), Some(from_type), Some(to_pos)) = (
                port_screen_pos(&self.nodes, &self.states, conn.from, origin),
                port_type_for(&self.nodes, &self.states, conn.from),
                port_screen_pos(&self.nodes, &self.states, conn.to, origin),
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

        // -- Compute node rects --
        let node_rects: Vec<Rect> = (0..self.nodes.len())
            .map(|i| {
                let n = &self.nodes[i];
                let w = n.min_width();
                let port_h = ports_height(n.inputs().len(), n.outputs().len());
                let content_h = PORT_START_Y + n.min_content_height() + NODE_PADDING;
                let h = port_h.max(content_h);
                let pos = self.states[i].pos + origin;
                Rect::from_min_size(pos, Vec2::new(w, h))
            })
            .collect();

        // -- Draw node chrome (painter-based, immutable) --
        for i in 0..self.nodes.len() {
            let selected = self.selected_node == Some(i);
            draw_node_chrome(&painter, self.nodes[i].as_ref(), node_rects[i], selected);
        }

        // -- Draw node content (needs &mut node) --
        for i in 0..self.nodes.len() {
            let rect = node_rects[i];
            let content_rect = node_content_rect(rect);
            if content_rect.width() > 0.0 && content_rect.height() > 0.0 {
                let mut content_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(content_rect)
                        .layout(egui::Layout::top_down(egui::Align::LEFT)),
                );
                self.nodes[i].show_content(&mut content_ui);
            }
        }

        // -- Handle interactions --
        self.handle_interactions(ui, &response, &node_rects);

        // -- Context menu --
        self.show_context_menu(ui, canvas_rect);
    }

    fn show_context_menu(&mut self, ui: &mut Ui, canvas_rect: Rect) {
        if self.context_menu_pos.is_none() {
            return;
        }
        let menu_pos = self.context_menu_pos.unwrap();

        let entries: Vec<(usize, &'static str)> = self.registry
            .iter()
            .enumerate()
            .map(|(i, e)| (i, e.label))
            .collect();

        if entries.is_empty() {
            self.context_menu_pos = None;
            return;
        }

        let mut spawn: Option<usize> = None;

        let area_resp = egui::Area::new(egui::Id::new("node_ctx_menu"))
            .fixed_pos(menu_pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(140.0);
                    ui.label(egui::RichText::new("Add node").strong().size(11.0));
                    ui.separator();
                    for (idx, label) in &entries {
                        if ui.button(*label).clicked() {
                            spawn = Some(*idx);
                        }
                    }
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
            let canvas_pos = menu_pos - canvas_rect.min.to_vec2() - self.pan;
            let node = (self.registry[reg_idx].factory)(id);
            let index = self.add_node(node, canvas_pos);
            self.new_nodes.push(NewNode { index, node_id: id });
            self.context_menu_pos = None;
        } else if clicked_outside || esc {
            self.context_menu_pos = None;
        }
    }

    // -----------------------------------------------------------------------
    // Signal propagation
    // -----------------------------------------------------------------------

    fn propagate_signals(&mut self) {
        // 1. Process all nodes (lets them update outputs based on previous inputs).
        for node in self.nodes.iter_mut() {
            node.process();
        }

        // 2. Read outputs and write to connected inputs.
        //    Collect values first to avoid borrow issues.
        let values: Vec<(PortId, f32)> = self.connections
            .iter()
            .filter_map(|conn| {
                let src_idx = self.states.iter().position(|s| s.id == conn.from.node)?;
                let val = self.nodes[src_idx].read_output(conn.from.index);
                Some((conn.to, val))
            })
            .collect();

        for (target, val) in values {
            if let Some(dst_idx) = self.states.iter().position(|s| s.id == target.node) {
                self.nodes[dst_idx].write_input(target.index, val);
            }
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
    ) {
        let pointer_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
        let primary_pressed =
            ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
        let primary_down = ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary));
        let primary_released =
            ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
        let drag_delta = response.drag_delta();

        // --- Connection drawing ---
        if let Some(ref mut dc) = self.drag.drawing_conn {
            dc.to_pos = pointer_pos;
            dc.snap_target = None;

            let mut best_dist = MAGNETIC_RADIUS;
            for i in 0..self.nodes.len() {
                let (ports, target_dir) = if dc.from.dir == PortDir::Output {
                    (self.nodes[i].inputs(), PortDir::Input)
                } else {
                    (self.nodes[i].outputs(), PortDir::Output)
                };
                for (pi, port_def) in ports.iter().enumerate() {
                    if !dc.from_type.compatible_with(&port_def.port_type) {
                        continue;
                    }
                    if self.states[i].id == dc.from.node {
                        continue;
                    }
                    let pos =
                        port_pos(node_rects[i].min, node_rects[i].width(), target_dir, pi);
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
                    self.connections.retain(|c| c.to != to);
                    self.add_connection(from, to);
                }
                self.drag.drawing_conn = None;
            }
            return;
        }

        // --- Check port clicks to start drawing ---
        if primary_pressed {
            for i in 0..self.nodes.len() {
                let rect = node_rects[i];
                let node_id = self.states[i].id;

                for (pi, port_def) in self.nodes[i].outputs().iter().enumerate() {
                    let pos = port_pos(rect.min, rect.width(), PortDir::Output, pi);
                    if pos.distance(pointer_pos) < PORT_RADIUS + 4.0 {
                        self.drag.drawing_conn = Some(DrawingConnection {
                            from: make_port_id(node_id, PortDir::Output, pi),
                            from_pos: pos,
                            from_type: port_def.port_type,
                            to_pos: pointer_pos,
                            snap_target: None,
                        });
                        return;
                    }
                }
                for (pi, port_def) in self.nodes[i].inputs().iter().enumerate() {
                    let pos = port_pos(rect.min, rect.width(), PortDir::Input, pi);
                    if pos.distance(pointer_pos) < PORT_RADIUS + 4.0 {
                        let input_id = make_port_id(node_id, PortDir::Input, pi);
                        // If this input already has a connection, disconnect it
                        // and start dragging from the original output end.
                        if let Some(conn_idx) = self.connections.iter().position(|c| c.to == input_id) {
                            let old_from = self.connections[conn_idx].from;
                            self.connections.remove(conn_idx);
                            // Start dragging from the now-disconnected output.
                            if let Some(from_pos) = port_screen_pos_from_rects(
                                &self.states, node_rects, old_from,
                            ) {
                                self.drag.drawing_conn = Some(DrawingConnection {
                                    from: old_from,
                                    from_pos,
                                    from_type: port_def.port_type,
                                    to_pos: pointer_pos,
                                    snap_target: None,
                                });
                            }
                        } else {
                            self.drag.drawing_conn = Some(DrawingConnection {
                                from: input_id,
                                from_pos: pos,
                                from_type: port_def.port_type,
                                to_pos: pointer_pos,
                                snap_target: None,
                            });
                        }
                        return;
                    }
                }
            }
        }

        // --- Node selection and dragging ---
        if primary_pressed && self.drag.dragging_node.is_none() {
            let mut clicked_node = None;
            for i in (0..self.nodes.len()).rev() {
                if node_rects[i].contains(pointer_pos) {
                    clicked_node = Some(i);
                    break;
                }
            }

            if let Some(i) = clicked_node {
                self.selected_node = Some(i);
                // Only drag by title bar.
                let title_rect = Rect::from_min_size(
                    node_rects[i].min,
                    Vec2::new(node_rects[i].width(), NODE_TITLE_HEIGHT),
                );
                if title_rect.contains(pointer_pos) {
                    self.drag.dragging_node = Some(i);
                }
            } else {
                self.selected_node = None;
            }
        }

        if let Some(idx) = self.drag.dragging_node {
            if primary_down {
                self.states[idx].pos += drag_delta;
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            } else {
                self.drag.dragging_node = None;
            }
        }

        // Hover cursor and tooltips over ports.
        for i in 0..self.nodes.len() {
            let rect = node_rects[i];
            for (pi, port_def) in self.nodes[i].outputs().iter().enumerate() {
                let pos = port_pos(rect.min, rect.width(), PortDir::Output, pi);
                if pos.distance(pointer_pos) < PORT_RADIUS + 4.0 {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    egui::show_tooltip_at(ui.ctx(), ui.layer_id(), egui::Id::new(("port_tip", i, pi, 1)), pos + egui::vec2(10.0, -10.0), |ui| {
                        ui.label(&port_def.name);
                    });
                }
            }
            for (pi, port_def) in self.nodes[i].inputs().iter().enumerate() {
                let pos = port_pos(rect.min, rect.width(), PortDir::Input, pi);
                if pos.distance(pointer_pos) < PORT_RADIUS + 4.0 {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    egui::show_tooltip_at(ui.ctx(), ui.layer_id(), egui::Id::new(("port_tip", i, pi, 0)), pos + egui::vec2(10.0, -10.0), |ui| {
                        ui.label(&port_def.name);
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
) -> Option<Pos2> {
    let idx = states.iter().position(|s| s.id == port_id.node)?;
    let rect = node_rects[idx];
    Some(port_pos(rect.min, rect.width(), port_id.dir, port_id.index))
}

fn node_content_rect(rect: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(rect.min.x + PORT_RADIUS + 8.0, rect.min.y + PORT_START_Y),
        Pos2::new(rect.max.x - PORT_RADIUS - 8.0, rect.max.y - NODE_PADDING),
    )
}

const SELECTED_BORDER: Color32 = Color32::from_rgb(100, 160, 255);

fn draw_node_chrome(painter: &Painter, node: &dyn NodeWidget, rect: Rect, selected: bool) {
    // Shadow
    let shadow_rect = rect.translate(Vec2::new(3.0, 3.0));
    painter.rect_filled(shadow_rect, NODE_CORNER_RADIUS, Color32::from_black_alpha(60));

    // Body
    painter.rect_filled(rect, NODE_CORNER_RADIUS, NODE_BG);
    let border = if selected { Stroke::new(2.0, SELECTED_BORDER) } else { Stroke::new(1.0, NODE_BORDER) };
    painter.rect_stroke(rect, NODE_CORNER_RADIUS, border, StrokeKind::Inside);

    // Title bar
    let title_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), NODE_TITLE_HEIGHT));
    painter.rect_filled(
        title_rect,
        egui::CornerRadius { nw: NODE_CORNER_RADIUS as u8, ne: NODE_CORNER_RADIUS as u8, sw: 0, se: 0 },
        NODE_TITLE_BG,
    );
    painter.text(
        title_rect.center(),
        egui::Align2::CENTER_CENTER,
        node.title(),
        egui::FontId::proportional(13.0),
        Color32::WHITE,
    );

    // Input ports
    for (i, port_def) in node.inputs().iter().enumerate() {
        let pos = port_pos(rect.min, rect.width(), PortDir::Input, i);
        draw_port(painter, pos, port_def);
    }

    // Output ports
    for (i, port_def) in node.outputs().iter().enumerate() {
        let pos = port_pos(rect.min, rect.width(), PortDir::Output, i);
        draw_port(painter, pos, port_def);
    }
}

fn draw_port(painter: &Painter, pos: Pos2, def: &PortDef) {
    let color = def.port_type.color();
    painter.circle_filled(pos, PORT_RADIUS, color);
    painter.circle_stroke(pos, PORT_RADIUS, Stroke::new(1.0, color.linear_multiply(0.6)));
}

fn draw_grid(painter: &Painter, rect: Rect, pan: Vec2) {
    painter.rect_filled(rect, 0.0, Color32::from_rgb(22, 22, 26));

    let offset_x = pan.x.rem_euclid(GRID_SPACING);
    let offset_y = pan.y.rem_euclid(GRID_SPACING);

    let mut x = rect.min.x + offset_x;
    while x < rect.max.x {
        painter.line_segment(
            [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
            Stroke::new(1.0, GRID_COLOR),
        );
        x += GRID_SPACING;
    }
    let mut y = rect.min.y + offset_y;
    while y < rect.max.y {
        painter.line_segment(
            [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
            Stroke::new(1.0, GRID_COLOR),
        );
        y += GRID_SPACING;
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
) -> Option<Pos2> {
    let idx = states.iter().position(|s| s.id == port_id.node)?;
    let pos = states[idx].pos + origin;
    let width = nodes[idx].min_width();
    Some(port_pos(pos, width, port_id.dir, port_id.index))
}

fn port_type_for(
    nodes: &[Box<dyn NodeWidget>],
    states: &[NodeState],
    port_id: PortId,
) -> Option<PortType> {
    let idx = states.iter().position(|s| s.id == port_id.node)?;
    let ports = match port_id.dir {
        PortDir::Input => nodes[idx].inputs(),
        PortDir::Output => nodes[idx].outputs(),
    };
    ports.get(port_id.index).map(|p| p.port_type)
}
