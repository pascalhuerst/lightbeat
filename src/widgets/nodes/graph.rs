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

pub struct NodeGraph {
    nodes: Vec<Box<dyn NodeWidget>>,
    states: Vec<NodeState>,
    connections: Vec<Connection>,
    drag: DragState,
    pan: Vec2,
}

impl NodeGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            states: Vec::new(),
            connections: Vec::new(),
            drag: DragState::default(),
            pan: Vec2::ZERO,
        }
    }

    pub fn add_node(&mut self, node: Box<dyn NodeWidget>, pos: Pos2) -> usize {
        let id = node.node_id();
        self.states.push(NodeState::new(id, pos));
        self.nodes.push(node);
        self.nodes.len() - 1
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
        let (response, painter) =
            ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let canvas_rect = response.rect;

        if response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
        {
            self.pan += response.drag_delta();
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
            draw_node_chrome(&painter, self.nodes[i].as_ref(), node_rects[i]);
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
                        self.drag.drawing_conn = Some(DrawingConnection {
                            from: make_port_id(node_id, PortDir::Input, pi),
                            from_pos: pos,
                            from_type: port_def.port_type,
                            to_pos: pointer_pos,
                            snap_target: None,
                        });
                        return;
                    }
                }
            }
        }

        // --- Node dragging (by title bar) ---
        if primary_pressed && self.drag.dragging_node.is_none() {
            for i in (0..self.nodes.len()).rev() {
                let title_rect = Rect::from_min_size(
                    node_rects[i].min,
                    Vec2::new(node_rects[i].width(), NODE_TITLE_HEIGHT),
                );
                if title_rect.contains(pointer_pos) {
                    self.drag.dragging_node = Some(i);
                    break;
                }
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

        // Hover cursor over ports.
        for i in 0..self.nodes.len() {
            let rect = node_rects[i];
            for (pi, _) in self.nodes[i].outputs().iter().enumerate() {
                let pos = port_pos(rect.min, rect.width(), PortDir::Output, pi);
                if pos.distance(pointer_pos) < PORT_RADIUS + 4.0 {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
            }
            for (pi, _) in self.nodes[i].inputs().iter().enumerate() {
                let pos = port_pos(rect.min, rect.width(), PortDir::Input, pi);
                if pos.distance(pointer_pos) < PORT_RADIUS + 4.0 {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Free functions (no &self needed)
// ---------------------------------------------------------------------------

fn node_content_rect(rect: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(rect.min.x + PORT_RADIUS + 8.0, rect.min.y + PORT_START_Y),
        Pos2::new(rect.max.x - PORT_RADIUS - 8.0, rect.max.y - NODE_PADDING),
    )
}

fn draw_node_chrome(painter: &Painter, node: &dyn NodeWidget, rect: Rect) {
    // Shadow
    let shadow_rect = rect.translate(Vec2::new(3.0, 3.0));
    painter.rect_filled(shadow_rect, NODE_CORNER_RADIUS, Color32::from_black_alpha(60));

    // Body
    painter.rect_filled(rect, NODE_CORNER_RADIUS, NODE_BG);
    painter.rect_stroke(rect, NODE_CORNER_RADIUS, Stroke::new(1.0, NODE_BORDER), StrokeKind::Inside);

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
        draw_port(painter, pos, port_def, PortDir::Input);
    }

    // Output ports
    for (i, port_def) in node.outputs().iter().enumerate() {
        let pos = port_pos(rect.min, rect.width(), PortDir::Output, i);
        draw_port(painter, pos, port_def, PortDir::Output);
    }
}

fn draw_port(painter: &Painter, pos: Pos2, def: &PortDef, dir: PortDir) {
    let color = def.port_type.color();
    painter.circle_filled(pos, PORT_RADIUS, color);
    painter.circle_stroke(pos, PORT_RADIUS, Stroke::new(1.0, color.linear_multiply(0.6)));

    let (anchor, label_x) = match dir {
        PortDir::Input => (egui::Align2::LEFT_CENTER, pos.x + PORT_RADIUS + 4.0),
        PortDir::Output => (egui::Align2::RIGHT_CENTER, pos.x - PORT_RADIUS - 4.0),
    };
    painter.text(
        Pos2::new(label_x, pos.y),
        anchor,
        &def.name,
        egui::FontId::proportional(10.0),
        Color32::from_gray(180),
    );
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
