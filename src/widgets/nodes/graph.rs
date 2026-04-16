use egui::{self, Color32, CursorIcon, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::node::*;
use super::types::*;
use crate::engine::nodes::meta::subgraph::{SubgraphPortDef, port_type_to_idx, BRIDGE_IN_NODE_ID, BRIDGE_OUT_NODE_ID};
use crate::engine::types::{Connection, NodeId, ParamDef, ParamValue, PortDir, PortId, PortType, SubgraphInnerCmd};
use crate::widgets::nodes::meta::subgraph::{SubgraphWidget, GraphInputWidget, GraphOutputWidget};

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
    /// Panning the canvas via left-click drag.
    panning: bool,
    /// Time and position of the last empty-canvas click (for manual double-click detection).
    last_canvas_click: Option<(std::time::Instant, Pos2)>,
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
    pub description: &'static str,
    pub factory: Box<dyn Fn(NodeId) -> Box<dyn NodeWidget>>,
}

/// Freshly spawned node, returned by `drain_new_nodes` so the app
/// can set up beat-clock subscriptions or other wiring.
pub struct NewNode {
    pub index: usize,
    /// Path of SubgraphProcessNode IDs from root to the level where this node lives.
    /// Empty means root level.
    pub subgraph_path: Vec<NodeId>,
}

// ---------------------------------------------------------------------------
// GraphLevel — one layer of the navigation stack
// ---------------------------------------------------------------------------

/// A single level in the graph navigation stack.
/// The root graph is level 0; navigating into a subgraph pushes a new level.
pub struct GraphLevel {
    pub nodes: Vec<Box<dyn NodeWidget>>,
    pub states: Vec<NodeState>,
    pub connections: Vec<Connection>,
    pub pan: Vec2,
    pub zoom: f32,
    /// The NodeId of the SubgraphWidget that owns this level (None for root).
    pub subgraph_id: Option<NodeId>,
    /// Label for breadcrumb display.
    pub label: String,
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
    levels: Vec<GraphLevel>,
    active_level: usize,
    drag: DragState,
    next_id: u64,
    registry: Vec<NodeEntry>,
    new_nodes: Vec<NewNode>,
    pending_engine_cmds: Vec<EngineCommand>,
    context_menu_pos: Option<Pos2>,
    context_menu_search: String,
    selected_nodes: Vec<usize>,
    canvas_rect: Rect,
    clipboard: Vec<ClipboardNode>,
    /// Set to true to fit the view to content on the next frame.
    fit_pending: bool,
}

impl NodeGraph {
    pub fn new() -> Self {
        Self {
            levels: vec![GraphLevel {
                nodes: Vec::new(),
                states: Vec::new(),
                connections: Vec::new(),
                pan: Vec2::ZERO,
                zoom: 1.0,
                subgraph_id: None,
                label: "Root".to_string(),
            }],
            active_level: 0,
            drag: DragState::default(),
            next_id: 1,
            registry: Vec::new(),
            new_nodes: Vec::new(),
            pending_engine_cmds: Vec::new(),
            context_menu_pos: None,
            context_menu_search: String::new(),
            selected_nodes: Vec::new(),
            canvas_rect: Rect::NOTHING,
            clipboard: Vec::new(),
            fit_pending: false,
        }
    }

    /// Request that the view fits all content on the next frame.
    pub fn fit_to_content(&mut self) {
        self.fit_pending = true;
    }

    /// Compute and apply pan/zoom so all nodes fit within canvas_rect with some padding.
    fn apply_fit_to_content(&mut self, canvas_rect: Rect) {
        let level = self.active();
        if level.nodes.is_empty() {
            return;
        }

        // Compute bounding box of all nodes in canvas-space (unzoomed).
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for (i, node) in level.nodes.iter().enumerate() {
            let pos = level.states[i].pos;
            let min_w = node.min_width();
            let inputs = node.ui_inputs();
            let outputs = node.ui_outputs();
            let port_h = ports_height(inputs.len(), outputs.len());
            let content_h = PORT_START_Y + node.min_content_height() + NODE_PADDING;
            let min_h = port_h.max(content_h);
            let size = level.states[i]
                .size_override
                .map(|s| Vec2::new(s.x.max(min_w), s.y.max(min_h)))
                .unwrap_or(Vec2::new(min_w, min_h));

            min_x = min_x.min(pos.x);
            min_y = min_y.min(pos.y);
            max_x = max_x.max(pos.x + size.x);
            max_y = max_y.max(pos.y + size.y);
        }

        let content_w = max_x - min_x;
        let content_h = max_y - min_y;

        if content_w <= 0.0 || content_h <= 0.0 {
            return;
        }

        let padding = 40.0;
        let available_w = (canvas_rect.width() - padding * 2.0).max(1.0);
        let available_h = (canvas_rect.height() - padding * 2.0).max(1.0);

        // Compute zoom to fit, clamped to reasonable range.
        let zoom = (available_w / content_w)
            .min(available_h / content_h)
            .clamp(0.2, 1.5);

        // Compute pan to center the content.
        let content_center_x = (min_x + max_x) / 2.0;
        let content_center_y = (min_y + max_y) / 2.0;
        let canvas_center_x = canvas_rect.width() / 2.0;
        let canvas_center_y = canvas_rect.height() / 2.0;

        let pan = Vec2::new(
            canvas_center_x - content_center_x * zoom,
            canvas_center_y - content_center_y * zoom,
        );

        let level = self.active_mut();
        level.zoom = zoom;
        level.pan = pan;
    }

    fn active(&self) -> &GraphLevel { &self.levels[self.active_level] }
    fn active_mut(&mut self) -> &mut GraphLevel { &mut self.levels[self.active_level] }

    /// Returns the path of subgraph NodeIds from root to current level.
    pub fn current_subgraph_path(&self) -> Vec<NodeId> {
        self.levels[1..=self.active_level]
            .iter()
            .filter_map(|l| l.subgraph_id)
            .collect()
    }

    /// Push an engine command, wrapping in SubgraphInnerCommand if we're inside a subgraph.
    fn push_engine_cmd(&mut self, cmd: EngineCommand) {
        let path = self.current_subgraph_path();
        if path.is_empty() {
            self.pending_engine_cmds.push(cmd);
        } else {
            let inner_cmd = match cmd {
                EngineCommand::AddConnection(c) => SubgraphInnerCmd::AddConnection(c),
                EngineCommand::RemoveConnectionTo(p) => SubgraphInnerCmd::RemoveConnectionTo(p),
                EngineCommand::RemoveNode(id) => SubgraphInnerCmd::RemoveNode(id),
                EngineCommand::LoadData { node_id, data } => SubgraphInnerCmd::LoadData { node_id, data },
                EngineCommand::NotifyConnect { node_id, input_port, source_type } => {
                    SubgraphInnerCmd::NotifyConnect { node_id, input_port, source_type }
                }
                EngineCommand::NotifyDisconnect { node_id, input_port } => {
                    SubgraphInnerCmd::NotifyDisconnect { node_id, input_port }
                }
                other => {
                    // Commands that don't have inner equivalents go directly.
                    self.pending_engine_cmds.push(other);
                    return;
                }
            };
            self.pending_engine_cmds.push(EngineCommand::SubgraphInnerCommand {
                subgraph_path: path,
                command: Box::new(inner_cmd),
            });
        }
    }

    /// Drain pending engine commands (called by main.rs each frame).
    pub fn drain_engine_commands(&mut self) -> Vec<EngineCommand> {
        std::mem::take(&mut self.pending_engine_cmds)
    }

    /// Register a node type that can be spawned from the context menu.
    pub fn register_node(&mut self, category: impl Into<String>, label: impl Into<String>, factory: impl Fn(NodeId) -> Box<dyn NodeWidget> + 'static) {
        // Probe the widget's description by creating a sample instance.
        let sample = factory(NodeId(0));
        let description = sample.description();
        drop(sample);
        self.registry.push(NodeEntry {
            label: label.into(),
            category: category.into(),
            description,
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
        let level = self.active_mut();
        level.states.push(NodeState::new(id, pos));
        level.nodes.push(node);
        let idx = level.nodes.len() - 1;
        let path = self.current_subgraph_path();
        self.new_nodes.push(NewNode { index: idx, subgraph_path: path });
        idx
    }

    /// Drain the list of newly-added nodes (from the context menu).
    /// Call this each frame to wire up subscriptions.
    pub fn drain_new_nodes(&mut self) -> Vec<NewNode> {
        std::mem::take(&mut self.new_nodes)
    }

    /// Get a mutable reference to a node by index in the active level.
    pub fn node_mut(&mut self, index: usize) -> &mut dyn NodeWidget {
        self.active_mut().nodes[index].as_mut()
    }

    /// Get a mutable reference to a node by index in a specific level identified by subgraph path.
    pub fn node_mut_at_path(&mut self, index: usize, subgraph_path: &[NodeId]) -> &mut dyn NodeWidget {
        let level_idx = if subgraph_path.is_empty() {
            0
        } else {
            let last_sg = subgraph_path.last().unwrap();
            self.levels.iter()
                .position(|l| l.subgraph_id.as_ref() == Some(last_sg))
                .unwrap_or(self.active_level)
        };
        self.levels[level_idx].nodes[index].as_mut()
    }

    /// Get mutable references to selected nodes (for inspector).
    /// Returns an iterator of &mut Box<dyn NodeWidget>.
    pub fn selected_nodes_mut(&mut self) -> Vec<&mut Box<dyn NodeWidget>> {
        let indices: Vec<usize> = self.selected_nodes.clone();
        let mut result = Vec::new();
        for (i, node) in self.active_mut().nodes.iter_mut().enumerate() {
            if indices.contains(&i) {
                result.push(node);
            }
        }
        result
    }

    /// Get all nodes for iteration (e.g. to call show_editor on each).
    pub fn nodes_mut(&mut self) -> &mut [Box<dyn NodeWidget>] {
        &mut self.active_mut().nodes
    }

    /// Get all nodes as shared references.
    pub fn all_nodes(&self) -> &[Box<dyn NodeWidget>] {
        &self.active().nodes
    }

    pub fn node_count(&self) -> usize {
        self.active().nodes.len()
    }

    pub fn node_and_state(&self, index: usize) -> (&dyn NodeWidget, &NodeState) {
        let level = self.active();
        (level.nodes[index].as_ref(), &level.states[index])
    }

    pub fn connections(&self) -> &[Connection] {
        &self.active().connections
    }

    #[allow(dead_code)]
    pub fn set_node_size(&mut self, index: usize, size: egui::Vec2) {
        self.active_mut().states[index].size_override = Some(size);
    }

    /// Access root level directly (for save/load which always operates on root).
    pub fn root_level(&self) -> &GraphLevel { &self.levels[0] }

    /// Whether we're currently inside a subgraph.
    pub fn is_in_subgraph(&self) -> bool { self.active_level > 0 }

    /// Get the active level index.
    pub fn active_level_index(&self) -> usize { self.active_level }

    /// Get root level mutably (for load_graph which needs to operate on root).
    pub fn root_level_mut(&mut self) -> &mut GraphLevel { &mut self.levels[0] }

    /// Current zoom level of the active graph.
    pub fn zoom(&self) -> f32 { self.active().zoom }

    /// Find the inner level for a given subgraph node ID, if it exists.
    pub fn find_level_for_subgraph(&self, subgraph_id: NodeId) -> Option<&GraphLevel> {
        self.levels.iter().find(|l| l.subgraph_id == Some(subgraph_id))
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
        let level = self.active_mut();
        if !level.connections.contains(&conn) {
            level.connections.push(conn.clone());

            // Notify target widget (UI side, e.g. scope port colors).
            if let (Some(src_idx), Some(dst_idx)) = (
                level.states.iter().position(|s| s.id == from.node),
                level.states.iter().position(|s| s.id == to.node),
            ) {
                let src_type = level.nodes[src_idx]
                    .ui_outputs()
                    .get(from.index)
                    .map(|p| p.def.port_type)
                    .unwrap_or(PortType::Untyped);
                level.nodes[dst_idx].on_ui_connect(to.index, src_type);

                // Notify engine for on_connect callback.
                self.push_engine_cmd(EngineCommand::NotifyConnect {
                    node_id: to.node,
                    input_port: to.index,
                    source_type: src_type,
                });
            }

            // Notify engine.
            self.push_engine_cmd(EngineCommand::AddConnection(conn));
        }
    }

    fn remove_connection_to(&mut self, to: PortId) {
        let level = self.active_mut();
        let had = level.connections.iter().any(|c| c.to == to);
        level.connections.retain(|c| c.to != to);
        if had {
            if let Some(dst_idx) = level.states.iter().position(|s| s.id == to.node) {
                level.nodes[dst_idx].on_ui_disconnect(to.index);
            }

            // Notify engine.
            self.push_engine_cmd(EngineCommand::RemoveConnectionTo(to));
            self.push_engine_cmd(EngineCommand::NotifyDisconnect {
                node_id: to.node,
                input_port: to.index,
            });
        }
    }

    // -----------------------------------------------------------------------
    // Main draw
    // -----------------------------------------------------------------------

    /// Remove connections that reference ports beyond a node's current port count.
    fn cleanup_stale_connections(&mut self) {
        let level = self.active();
        let stale: Vec<PortId> = level.connections.iter().filter(|conn| {
            let from_ok = level.nodes.iter()
                .zip(level.states.iter())
                .find(|(_, s)| s.id == conn.from.node)
                .map(|(n, _)| conn.from.index < n.ui_outputs().len())
                .unwrap_or(false);
            let to_ok = level.nodes.iter()
                .zip(level.states.iter())
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
        // -- Breadcrumb bar when inside a subgraph --
        if self.active_level > 0 {
            ui.horizontal(|ui| {
                if ui.small_button("Root").clicked() {
                    self.navigate_to_level(0);
                }
                for i in 1..=self.active_level {
                    ui.label("\u{25B6}");
                    let label = self.levels[i].label.clone();
                    if i < self.active_level {
                        if ui.small_button(&label).clicked() {
                            self.navigate_to_level(i);
                        }
                    } else {
                        ui.strong(&label);
                    }
                }
            });
            ui.separator();
        }

        // Clean up stale connections (ports removed by dynamic nodes like Group Output).
        self.cleanup_stale_connections();

        let (response, painter) =
            ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let canvas_rect = response.rect;
        self.canvas_rect = canvas_rect;

        // Fit view to content if requested.
        if self.fit_pending {
            self.fit_pending = false;
            self.apply_fit_to_content(canvas_rect);
        }

        // Zoom with scroll wheel, centered on mouse position.
        if response.contains_pointer() {
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_delta != 0.0 {
                let zoom_factor = 1.0 + scroll_delta * 0.002;
                let level = self.active_mut();
                let new_zoom = (level.zoom * zoom_factor).clamp(0.2, 3.0);

                // Zoom around mouse position.
                if let Some(mouse) = ui.input(|i| i.pointer.hover_pos()) {
                    let mc = mouse.to_vec2() - canvas_rect.min.to_vec2();
                    level.pan = (level.pan - mc) * (new_zoom / level.zoom) + mc;
                }
                level.zoom = new_zoom;
            }
        }

        // Pan with middle mouse.
        if response.dragged_by(egui::PointerButton::Middle) {
            self.active_mut().pan += response.drag_delta();
        }

        // Right-click opens context menu (only on empty canvas, not on nodes).
        if response.secondary_clicked() {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                self.context_menu_pos = Some(pos);
                self.context_menu_search.clear();
            }
        }

        let level = self.active();
        let z = level.zoom;
        let pan = level.pan;
        draw_grid(&painter, canvas_rect, pan, z);

        let origin = canvas_rect.min.to_vec2() + pan;

        // -- Draw connections --
        for conn in &level.connections {
            if let (Some(from_pos), Some(from_type), Some(to_pos)) = (
                port_screen_pos(&level.nodes, &level.states, conn.from, origin, z),
                port_type_for(&level.nodes, &level.states, conn.from),
                port_screen_pos(&level.nodes, &level.states, conn.to, origin, z),
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
        let level = self.active();
        let node_rects: Vec<Rect> = (0..level.nodes.len())
            .map(|i| {
                let n = &level.nodes[i];
                let min_w = n.min_width();
                let inputs = n.ui_inputs();
                let outputs = n.ui_outputs();
                let port_h = ports_height(inputs.len(), outputs.len());
                let content_h = PORT_START_Y + n.min_content_height() + NODE_PADDING;
                let min_h = port_h.max(content_h);
                let size = level.states[i]
                    .size_override
                    .map(|s| Vec2::new(s.x.max(min_w), s.y.max(min_h)))
                    .unwrap_or(Vec2::new(min_w, min_h));
                let pos = Pos2::new(
                    level.states[i].pos.x * z,
                    level.states[i].pos.y * z,
                ) + origin;
                Rect::from_min_size(pos, size * z)
            })
            .collect();

        // -- Draw node chrome (painter-based, immutable) --
        for i in 0..level.nodes.len() {
            let selected = self.selected_nodes.contains(&i);
            let inputs = level.nodes[i].ui_inputs();
            let outputs = level.nodes[i].ui_outputs();
            draw_node_chrome(
                &painter,
                level.nodes[i].title(),
                level.nodes[i].resizable(),
                level.nodes[i].title_color(),
                &inputs,
                &outputs,
                node_rects[i],
                selected,
                z,
            );
        }

        // -- Draw node content (needs &mut node) --
        let level = self.active_mut();
        for i in 0..level.nodes.len() {
            let rect = node_rects[i];
            let content_rect = node_content_rect(rect, z);
            if content_rect.width() > 0.0 && content_rect.height() > 0.0 {
                let mut content_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(content_rect)
                        .layout(egui::Layout::top_down(egui::Align::LEFT)),
                );
                // Clip the content area so widgets can't paint outside the node.
                content_ui.set_clip_rect(content_rect);
                // Scale text and spacing for zoom.
                if (z - 1.0).abs() > 0.01 {
                    let mut style = (**content_ui.style()).clone();
                    for (_, font_id) in style.text_styles.iter_mut() {
                        font_id.size *= z;
                    }
                    style.spacing.item_spacing *= z;
                    style.spacing.button_padding *= z;
                    style.spacing.interact_size *= z;
                    style.spacing.icon_width *= z;
                    style.spacing.icon_width_inner *= z;
                    style.spacing.icon_spacing *= z;
                    style.spacing.slider_width *= z;
                    style.spacing.combo_width *= z;
                    style.spacing.text_edit_width *= z;
                    content_ui.set_style(style);
                }

                level.nodes[i].show_content(&mut content_ui, z);
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
            let pan = self.active().pan;
            let pos = ui.input(|i| i.pointer.hover_pos())
                .map(|p| p - canvas_rect.min.to_vec2() - pan)
                .unwrap_or(Pos2::new(100.0, 100.0));
            self.paste(pos);
        }

        // -- Check for subgraph open requests --
        let mut open_subgraph_idx = None;
        for i in 0..self.active().nodes.len() {
            if let Some(sub) = self.active_mut().nodes[i].as_any_mut().downcast_mut::<SubgraphWidget>() {
                if sub.wants_open {
                    sub.wants_open = false;
                    open_subgraph_idx = Some(i);
                    break;
                }
            }
        }
        if let Some(idx) = open_subgraph_idx {
            self.navigate_into(idx);
        }

        // -- Context menu --
        self.show_context_menu(ui, canvas_rect);
    }

    fn delete_selected(&mut self) {
        let mut to_remove = self.selected_nodes.clone();
        to_remove.sort_unstable();
        to_remove.dedup();

        // Don't delete bridge pseudo-nodes.
        let level = self.active();
        to_remove.retain(|&i| {
            let id = level.states[i].id;
            id != BRIDGE_IN_NODE_ID && id != BRIDGE_OUT_NODE_ID
        });
        if to_remove.is_empty() {
            return;
        }

        let level = self.active_mut();
        let removed_ids: Vec<NodeId> = to_remove.iter().map(|&i| level.states[i].id).collect();

        level.connections
            .retain(|c| !removed_ids.contains(&c.from.node) && !removed_ids.contains(&c.to.node));

        // Remove nodes in reverse order.
        for &i in to_remove.iter().rev() {
            level.nodes.remove(i);
            level.states.remove(i);
        }

        // Notify engine to remove these nodes.
        for &id in &removed_ids {
            self.push_engine_cmd(EngineCommand::RemoveNode(id));
        }

        self.selected_nodes.clear();
    }

    fn copy_selected(&mut self) {
        self.clipboard.clear();
        if self.selected_nodes.is_empty() {
            return;
        }

        let origin = self.active().states[self.selected_nodes[0]].pos;
        let selected = self.selected_nodes.clone();

        let mut new_clip = Vec::new();
        for &i in &selected {
            let level = self.active();
            let node = &level.nodes[i];
            let state = &level.states[i];

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

            let data = shared
                .display
                .as_ref()
                .and_then(|d| d.downcast_ref::<serde_json::Value>().cloned());
            drop(shared);

            new_clip.push(ClipboardNode {
                type_name: node.type_name().to_string(),
                size: state.size_override,
                params,
                data,
                offset: state.pos - origin,
            });
        }
        self.clipboard = new_clip;
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
                    let shared = self.active().nodes[idx].shared_state();
                    let mut state = shared.lock().unwrap();
                    for (pi, val) in &cn.params {
                        state.pending_params.push((*pi, val.clone()));
                    }
                }

                if let Some(size) = cn.size {
                    self.active_mut().states[idx].size_override = Some(size);
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
            self.active().states[i].pos + Vec2::new(GRID_SPACING * 2.0, GRID_SPACING * 2.0)
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

        // Filter entries by search (exclude hidden categories).
        let filtered: Vec<(usize, &str, &str, &str)> = self.registry.iter().enumerate()
            .filter(|(_, e)| !e.category.starts_with('_'))
            .filter(|(_, e)| search.is_empty() || e.label.to_lowercase().contains(&search) || e.category.to_lowercase().contains(&search))
            .map(|(i, e)| (i, e.category.as_str(), e.label.as_str(), e.description))
            .collect();

        // Description shown for the currently hovered entry (defaults to first match).
        let mut hovered_desc: Option<&'static str> =
            filtered.first().map(|&(_, _, _, d)| d);

        let mut open = true;
        let _win_resp = egui::Window::new("Add node")
            .id(egui::Id::new("node_ctx_menu"))
            .default_pos(menu_pos)
            .default_size([260.0, 420.0])
            .resizable(true)
            .collapsible(false)
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                // Search field — auto-focused.
                let search_resp = ui.add(
                    egui::TextEdit::singleline(&mut self.context_menu_search)
                        .hint_text("Search nodes...")
                        .desired_width(ui.available_width()),
                );
                if search_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.context_menu_pos = None;
                    self.context_menu_search.clear();
                    return;
                }
                if search_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Some(&(idx, _, _, _)) = filtered.first() {
                        spawn = Some(idx);
                    }
                }
                search_resp.request_focus();

                ui.separator();

                // Reserve space at the bottom for the description preview.
                let desc_h = 64.0;
                let list_h = (ui.available_height() - desc_h).max(80.0);

                egui::ScrollArea::vertical()
                    .max_height(list_h)
                    .show(ui, |ui| {
                        if filtered.is_empty() {
                            ui.colored_label(egui::Color32::from_gray(100), "No matches");
                        } else {
                            let mut last_cat = "";
                            for &(idx, cat, label, desc) in &filtered {
                                if cat != last_cat {
                                    if !last_cat.is_empty() { ui.add_space(2.0); }
                                    ui.colored_label(
                                        egui::Color32::from_gray(120),
                                        egui::RichText::new(cat).size(10.0),
                                    );
                                    last_cat = cat;
                                }
                                let resp = ui.button(label);
                                if resp.hovered() {
                                    hovered_desc = Some(desc);
                                }
                                if resp.clicked() {
                                    spawn = Some(idx);
                                }
                            }
                        }
                    });

                ui.separator();
                // Description preview area.
                ui.allocate_ui(egui::Vec2::new(ui.available_width(), desc_h - 8.0), |ui| {
                    let desc = hovered_desc.unwrap_or("");
                    if desc.is_empty() {
                        ui.colored_label(egui::Color32::from_gray(80), "Hover an entry for details.");
                    } else {
                        ui.colored_label(egui::Color32::from_gray(180), desc);
                    }
                });
            });

        if !open {
            self.context_menu_pos = None;
            self.context_menu_search.clear();
        }

        // "Move to Subgraph" option when nodes are selected.
        if !self.selected_nodes.is_empty() && self.context_menu_pos.is_some() {
            let mut move_to_sub = false;
            egui::Window::new("Selection")
                .id(egui::Id::new("selection_ctx_menu"))
                .default_pos(menu_pos + Vec2::new(210.0, 0.0))
                .auto_sized()
                .collapsible(false)
                .show(ui.ctx(), |ui| {
                    if ui.button("Move to Subgraph").clicked() {
                        move_to_sub = true;
                    }
                });
            if move_to_sub {
                self.move_selection_to_subgraph();
                self.context_menu_pos = None;
                self.context_menu_search.clear();
                return;
            }
        }

        if let Some(reg_idx) = spawn {
            let id = self.alloc_id();
            let level = self.active();
            let canvas_pos = (menu_pos - canvas_rect.min.to_vec2() - level.pan) / level.zoom;
            let node = (self.registry[reg_idx].factory)(id);
            self.add_node(node, canvas_pos);
            self.context_menu_search.clear();
            self.context_menu_pos = None;
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
        let canvas_has_pointer = response.contains_pointer();
        let primary_pressed = canvas_has_pointer
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
        let double_clicked = canvas_has_pointer
            && ui.input(|i| i.pointer.button_double_clicked(egui::PointerButton::Primary));
        let primary_down = ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary));
        let primary_released =
            ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
        let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
        let drag_delta = response.drag_delta();
        let on_canvas = canvas_has_pointer && self.canvas_rect.contains(pointer_pos);
        let z = self.active().zoom;

        // --- Connection drawing ---
        if self.drag.drawing_conn.is_some() {
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
                    (level.nodes[i].ui_inputs(), PortDir::Input)
                } else {
                    (level.nodes[i].ui_outputs(), PortDir::Output)
                };
                for (pi, ui_port) in ports.iter().enumerate() {
                    if !dc_from_type.compatible_with(&ui_port.def.port_type) {
                        continue;
                    }
                    if level.states[i].id == dc_from_node {
                        continue;
                    }
                    let pos =
                        port_pos_z(node_rects[i].min, node_rects[i].width(), target_dir, pi, z);
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
                self.drag.drawing_conn = None;
                if let Some(target) = snap {
                    let (from, to) = if dc_from_dir == PortDir::Output {
                        (dc_from, target)
                    } else {
                        (target, dc_from)
                    };
                    self.remove_connection_to(to);
                    self.add_connection(from, to);
                }
            }
            return;
        }

        // --- Selection rectangle ---
        if let Some(start) = self.drag.selection_rect_start {
            if primary_down {
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
            let level = self.active();
            for i in 0..level.nodes.len() {
                let rect = node_rects[i];
                let node_id = level.states[i].id;

                let outputs = level.nodes[i].ui_outputs();
                for (pi, ui_port) in outputs.iter().enumerate() {
                    let pos = port_pos_z(rect.min, rect.width(), PortDir::Output, pi, z);
                    if pos.distance(pointer_pos) < (PORT_RADIUS + 4.0) * z {
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
                let input_ports: Vec<(usize, Pos2, PortType)> = level.nodes[i]
                    .ui_inputs()
                    .iter()
                    .enumerate()
                    .map(|(pi, up)| {
                        let pos = port_pos_z(rect.min, rect.width(), PortDir::Input, pi, z);
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
                                &level.states, node_rects, old_from, z,
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
                let level = self.active();
                let n = &level.nodes[idx];
                let min_w = n.min_width();
                let inputs = n.ui_inputs();
                let outputs = n.ui_outputs();
                let port_h = ports_height(inputs.len(), outputs.len());
                let content_h = PORT_START_Y + n.min_content_height() + NODE_PADDING;
                let min_h = port_h.max(content_h);
                let current = level.states[idx].size_override.unwrap_or(Vec2::new(min_w, min_h));
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
                let mut clicked_node = None;
                let level = self.active();
                for i in (0..level.nodes.len()).rev() {
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
                    // Double-click on a subgraph node to open it.
                    if double_clicked {
                        if self.active().nodes[i].type_name() == "Subgraph" {
                            if let Some(sub) = self.active_mut().nodes[i].as_any_mut().downcast_mut::<SubgraphWidget>() {
                                sub.wants_open = true;
                            }
                        }
                    }

                    let title_rect = Rect::from_min_size(
                        node_rects[i].min,
                        Vec2::new(node_rects[i].width(), NODE_TITLE_HEIGHT * z),
                    );
                    if title_rect.contains(pointer_pos) && !double_clicked {
                        self.drag.dragging_nodes = true;
                    }
                } else {
                    // Manual double-click detection on empty canvas:
                    // egui's double_clicked may not fire reliably when the first click
                    // started a drag (e.g., panning). We track our own timer.
                    let now = std::time::Instant::now();
                    let is_double = self.drag.last_canvas_click.map_or(false, |(t, p)| {
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

                    // Empty canvas: shift+drag = pan, plain drag = selection rect.
                    let shift = ui.input(|i| i.modifiers.shift);
                    if shift {
                        self.drag.panning = true;
                    } else {
                        if !ctrl { self.selected_nodes.clear(); }
                        self.drag.selection_rect_start = Some(pointer_pos);
                    }
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
            for (pi, ui_port) in level.nodes[i].ui_outputs().iter().enumerate() {
                let pos = port_pos_z(rect.min, rect.width(), PortDir::Output, pi, z);
                if pos.distance(pointer_pos) < (PORT_RADIUS + 4.0) * z {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    egui::show_tooltip_at(ui.ctx(), ui.layer_id(), egui::Id::new(("port_tip", i, pi, 1)), pos + egui::vec2(10.0, -10.0), |ui| {
                        ui.label(&ui_port.def.name);
                    });
                }
            }
            for (pi, ui_port) in level.nodes[i].ui_inputs().iter().enumerate() {
                let pos = port_pos_z(rect.min, rect.width(), PortDir::Input, pi, z);
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

    /// Public entry point for navigating into a subgraph (used by load_graph).
    pub fn navigate_into_by_index(&mut self, subgraph_node_index: usize) {
        self.navigate_into(subgraph_node_index);
    }

    /// Navigate back to the parent level.
    pub fn navigate_up(&mut self) {
        if self.active_level > 0 {
            self.active_level -= 1;
            self.selected_nodes.clear();
            self.drag = DragState::default();
        }
    }

    fn navigate_into(&mut self, subgraph_node_index: usize) {
        let level = self.active();
        let subgraph_id = level.states[subgraph_node_index].id;
        let label = level.nodes[subgraph_node_index].title().to_string();

        // Get port defs by reading the SubgraphWidget's display from shared state.
        // The widget's ports are also reflected in its ui_inputs/ui_outputs.
        let (input_defs, output_defs) = {
            let inputs = level.nodes[subgraph_node_index].ui_inputs();
            let outputs = level.nodes[subgraph_node_index].ui_outputs();
            let in_defs: Vec<SubgraphPortDef> = inputs.iter()
                .map(|p| SubgraphPortDef {
                    name: p.def.name.clone(),
                    port_type_idx: port_type_to_idx(p.def.port_type),
                })
                .collect();
            let out_defs: Vec<SubgraphPortDef> = outputs.iter()
                .map(|p| SubgraphPortDef {
                    name: p.def.name.clone(),
                    port_type_idx: port_type_to_idx(p.def.port_type),
                })
                .collect();
            (in_defs, out_defs)
        };

        // Check if we already have this level on the stack (re-entering after navigating up).
        let existing = self.levels.iter().position(|l| l.subgraph_id == Some(subgraph_id));
        if let Some(idx) = existing {
            self.active_level = idx;
            // Update bridge node ports in case they changed.
            let level = &mut self.levels[idx];
            for node in &mut level.nodes {
                if node.node_id() == BRIDGE_IN_NODE_ID {
                    if let Some(gi) = node.as_any_mut().downcast_mut::<GraphInputWidget>() {
                        gi.update_ports(input_defs.clone());
                    }
                } else if node.node_id() == BRIDGE_OUT_NODE_ID {
                    if let Some(go) = node.as_any_mut().downcast_mut::<GraphOutputWidget>() {
                        go.update_ports(output_defs.clone());
                    }
                }
            }
        } else {
            // Create new level with bridge nodes.
            let mut nodes: Vec<Box<dyn NodeWidget>> = Vec::new();
            let mut states: Vec<NodeState> = Vec::new();

            nodes.push(Box::new(GraphInputWidget::new(input_defs)));
            states.push(NodeState::new(BRIDGE_IN_NODE_ID, Pos2::new(20.0, 100.0)));
            nodes.push(Box::new(GraphOutputWidget::new(output_defs)));
            states.push(NodeState::new(BRIDGE_OUT_NODE_ID, Pos2::new(500.0, 100.0)));

            self.levels.push(GraphLevel {
                nodes,
                states,
                connections: Vec::new(),
                pan: Vec2::ZERO,
                zoom: 1.0,
                subgraph_id: Some(subgraph_id),
                label,
            });
            self.active_level = self.levels.len() - 1;
        }
        self.selected_nodes.clear();
        self.drag = DragState::default();
    }

    fn navigate_to_level(&mut self, level: usize) {
        if level < self.levels.len() {
            self.active_level = level;
            self.selected_nodes.clear();
            self.drag = DragState::default();
        }
    }

    // -----------------------------------------------------------------------
    // Move selection into subgraph
    // -----------------------------------------------------------------------

    fn move_selection_to_subgraph(&mut self) {
        let selected = self.selected_nodes.clone();
        if selected.is_empty() { return; }

        let level = self.active();
        let selected_ids: Vec<NodeId> = selected.iter()
            .map(|&i| level.states[i].id)
            .collect();

        // 1. Classify connections.
        let mut internal_conns = Vec::new();
        let mut incoming_conns = Vec::new();
        let mut outgoing_conns = Vec::new();

        for conn in &level.connections {
            let from_inside = selected_ids.contains(&conn.from.node);
            let to_inside = selected_ids.contains(&conn.to.node);
            match (from_inside, to_inside) {
                (true, true) => internal_conns.push(conn.clone()),
                (false, true) => incoming_conns.push(conn.clone()),
                (true, false) => outgoing_conns.push(conn.clone()),
                (false, false) => {}
            }
        }

        // 2. Deduplicate inputs: group incoming connections by source port.
        //    Multiple wires from the same output become one subgraph input.
        let mut unique_input_sources: Vec<PortId> = Vec::new();
        let mut incoming_port_map: Vec<usize> = Vec::new(); // incoming_conn index -> subgraph input port index
        for conn in &incoming_conns {
            let idx = unique_input_sources.iter().position(|s| *s == conn.from);
            if let Some(idx) = idx {
                incoming_port_map.push(idx);
            } else {
                incoming_port_map.push(unique_input_sources.len());
                unique_input_sources.push(conn.from);
            }
        }

        let input_defs: Vec<SubgraphPortDef> = unique_input_sources.iter().enumerate()
            .map(|(i, src_port)| {
                let pt = port_type_for(&level.nodes, &level.states, *src_port)
                    .unwrap_or(PortType::Untyped);
                SubgraphPortDef {
                    name: format!("in {}", i),
                    port_type_idx: port_type_to_idx(pt),
                }
            }).collect();

        // 3. Deduplicate outputs: group outgoing connections by source port.
        //    Multiple wires from the same inner output become one subgraph output.
        let mut unique_output_sources: Vec<PortId> = Vec::new();
        let mut outgoing_port_map: Vec<usize> = Vec::new(); // outgoing_conn index -> subgraph output port index
        for conn in &outgoing_conns {
            let idx = unique_output_sources.iter().position(|s| *s == conn.from);
            if let Some(idx) = idx {
                outgoing_port_map.push(idx);
            } else {
                outgoing_port_map.push(unique_output_sources.len());
                unique_output_sources.push(conn.from);
            }
        }

        let output_defs: Vec<SubgraphPortDef> = unique_output_sources.iter().enumerate()
            .map(|(i, src_port)| {
                let pt = port_type_for(&level.nodes, &level.states, *src_port)
                    .unwrap_or(PortType::Untyped);
                SubgraphPortDef {
                    name: format!("out {}", i),
                    port_type_idx: port_type_to_idx(pt),
                }
            }).collect();

        // 4. Compute centroid of selected nodes.
        let centroid = {
            let sum: Vec2 = selected.iter()
                .map(|&i| level.states[i].pos.to_vec2())
                .fold(Vec2::ZERO, |a, b| a + b);
            Pos2::new(0.0, 0.0) + sum / selected.len() as f32
        };

        // 5. Create SubgraphWidget.
        let sub_id = self.alloc_id();
        let shared = crate::engine::types::new_shared_state(12, 12);
        let mut sub_widget = SubgraphWidget::new(sub_id, shared);
        sub_widget.input_defs = input_defs.clone();
        sub_widget.output_defs = output_defs.clone();
        sub_widget.push_config();

        // 6. Extract selected nodes from the active level.
        //    Capture current params and save_data so we can restore them on the new engine nodes.
        let mut moved_nodes: Vec<(Box<dyn NodeWidget>, NodeState, Vec<(usize, ParamValue)>, Option<serde_json::Value>)> = Vec::new();
        let mut sorted_selected = selected.clone();
        sorted_selected.sort_unstable();
        sorted_selected.dedup();

        let level = self.active_mut();
        for &i in sorted_selected.iter().rev() {
            let node = level.nodes.remove(i);
            let state = level.states.remove(i);
            // Snapshot params and save_data from shared state.
            let (params, save_data) = {
                let shared = node.shared_state().lock().unwrap();
                let params: Vec<(usize, ParamValue)> = shared.current_params.iter().enumerate()
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
                (params, shared.save_data.clone())
            };
            moved_nodes.push((node, state, params, save_data));
        }
        moved_nodes.reverse();

        // Remove all connections involving selected nodes from outer graph.
        level.connections.retain(|c| {
            !selected_ids.contains(&c.from.node) && !selected_ids.contains(&c.to.node)
        });

        // 7. Notify engine to remove moved nodes from outer graph.
        for &id in &selected_ids {
            self.push_engine_cmd(EngineCommand::RemoveNode(id));
        }

        // 8. Add subgraph node to outer graph.
        let sub_idx = self.add_node(Box::new(sub_widget), centroid);
        let sub_node_id = self.active().states[sub_idx].id;

        // 9. Wire outer connections to the subgraph's ports (deduplicated).
        // Incoming: each unique source -> subgraph input port
        for (i, src_port) in unique_input_sources.iter().enumerate() {
            let to = PortId { node: sub_node_id, index: i, dir: PortDir::Input };
            self.add_connection(*src_port, to);
        }
        // Outgoing: subgraph output port -> each external destination
        for (conn_idx, conn) in outgoing_conns.iter().enumerate() {
            let port_idx = outgoing_port_map[conn_idx];
            let from = PortId { node: sub_node_id, index: port_idx, dir: PortDir::Output };
            self.add_connection(from, conn.to);
        }

        // 10. Navigate into the subgraph and populate it.
        // navigate_into already creates bridge nodes (GraphInput/GraphOutput).
        self.navigate_into(sub_idx);

        // Add the moved nodes to the inner level (after bridge nodes added by navigate_into).
        // Collect node IDs, params, and save_data for restoration.
        let mut restore_info: Vec<(NodeId, Vec<(usize, ParamValue)>, Option<serde_json::Value>)> = Vec::new();
        let level = self.active_mut();
        for (node, state, params, save_data) in moved_nodes {
            let node_id = node.node_id();
            let new_pos = Pos2::new(
                state.pos.x - centroid.x + 250.0,
                state.pos.y - centroid.y + 150.0,
            );
            level.states.push(NodeState::new(node_id, new_pos));
            if let Some(size) = state.size_override {
                level.states.last_mut().unwrap().size_override = Some(size);
            }
            level.nodes.push(node);
            restore_info.push((node_id, params, save_data));
        }

        // Register moved nodes with engine (bridge nodes at indices 0,1 don't need it).
        let path = self.current_subgraph_path();
        let moved_count = self.active().nodes.len();
        for i in 2..moved_count {
            self.new_nodes.push(NewNode {
                index: i,
                subgraph_path: path.clone(),
            });
        }

        // Restore params and save_data on moved nodes via shared state.
        // The new engine nodes will pick these up on the next tick.
        for (node_id, params, save_data) in restore_info {
            // Find the widget by node_id in the inner level.
            let level = self.active();
            if let Some(idx) = level.states.iter().position(|s| s.id == node_id) {
                let shared = level.nodes[idx].shared_state();
                let mut state = shared.lock().unwrap();
                for (pi, val) in params {
                    state.pending_params.push((pi, val));
                }
                if let Some(data) = save_data {
                    state.pending_config = Some(data);
                }
            }
        }

        // Add internal connections inside the subgraph.
        let level = self.active_mut();
        for conn in &internal_conns {
            level.connections.push(conn.clone());
        }
        for conn in &internal_conns {
            self.push_engine_cmd(EngineCommand::AddConnection(conn.clone()));
        }

        // Add bridge connections: BRIDGE_IN -> inner node inputs (using deduplicated port indices).
        for (conn_idx, conn) in incoming_conns.iter().enumerate() {
            let port_idx = incoming_port_map[conn_idx];
            let bridge_conn = Connection {
                from: PortId { node: BRIDGE_IN_NODE_ID, index: port_idx, dir: PortDir::Output },
                to: conn.to,
            };
            self.active_mut().connections.push(bridge_conn.clone());
            self.push_engine_cmd(EngineCommand::AddConnection(bridge_conn));
        }
        // Inner node outputs -> BRIDGE_OUT (using deduplicated port indices).
        for (conn_idx, conn) in outgoing_conns.iter().enumerate() {
            let port_idx = outgoing_port_map[conn_idx];
            let bridge_conn = Connection {
                from: conn.from,
                to: PortId { node: BRIDGE_OUT_NODE_ID, index: port_idx, dir: PortDir::Input },
            };
            // Only add if not already added (dedup: multiple outgoing from same source).
            let level = self.active_mut();
            if !level.connections.contains(&bridge_conn) {
                level.connections.push(bridge_conn.clone());
            }
            self.push_engine_cmd(EngineCommand::AddConnection(bridge_conn));
        }

        self.selected_nodes.clear();

        // Navigate back to the parent level.
        self.navigate_to_level(self.active_level - 1);
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
    title_color: Option<Color32>,
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

    // Title bar
    let title_bg = title_color.unwrap_or(NODE_TITLE_BG);
    let title_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), NODE_TITLE_HEIGHT * zoom));
    painter.rect_filled(
        title_rect,
        egui::CornerRadius { nw: NODE_CORNER_RADIUS as u8, ne: NODE_CORNER_RADIUS as u8, sw: 0, se: 0 },
        title_bg,
    );
    painter.text(
        title_rect.center(),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(13.0 * zoom),
        Color32::WHITE,
    );

    // Border (drawn after title bar so it isn't covered).
    let border = if selected { Stroke::new(2.0, SELECTED_BORDER) } else { Stroke::new(1.0, NODE_BORDER) };
    painter.rect_stroke(rect, NODE_CORNER_RADIUS, border, StrokeKind::Inside);

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
