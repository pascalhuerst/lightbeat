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

use crate::theme;

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
    /// Bundle size — number of consecutive ports to connect on drop.
    /// Set by pressing a digit key (2..=9, 0 = 10) while a wire is being
    /// drawn. Defaults to 1 (single wire). On drop, indices
    /// `from..from + bundle_size` are paired with `to..to + bundle_size`,
    /// skipping any pair that's type-incompatible or out of range.
    bundle_size: usize,
    /// When the drag started by grabbing a connected input port (effectively
    /// "unwiring" it), this records that port id. Used for bundle-remove:
    /// dropping on empty canvas with a bundle armed deletes the additional
    /// N-1 wires at consecutive input indices on the same node.
    unwired_to: Option<PortId>,
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
    /// Index of the level this subgraph lives inside. `None` for root. Tracked
    /// explicitly because `self.levels` is a visit-order list (not a simple
    /// stack) — when the user navigates between sibling subgraphs, multiple
    /// levels can end up above the parent, and we need the true parent chain
    /// to build correct subgraph paths for engine commands.
    pub parent_level_idx: Option<usize>,
    /// Label for breadcrumb display.
    pub label: String,
    /// Decorative frames — purely visual containers stored per-level.
    /// Rendered behind nodes, draggable/resizable, with title and notes.
    /// No engine semantics; nodes "in" a frame are just nodes whose rect
    /// overlaps the frame's rect.
    #[allow(dead_code)]
    pub frames: Vec<GraphFrame>,
}

/// A purely-visual rectangle drawn behind nodes for grouping purposes.
/// Position and size are in world coordinates (same space as `NodeState.pos`).
#[derive(Debug, Clone)]
pub struct GraphFrame {
    pub id: u64,
    pub title: String,
    /// Stored as RGBA (multiplied alpha is applied at draw time).
    pub color: egui::Color32,
    pub notes: String,
    pub pos: egui::Pos2,
    pub size: Vec2,
}

impl GraphFrame {
    pub fn rect(&self) -> egui::Rect {
        egui::Rect::from_min_size(self.pos, self.size)
    }
}

/// View state snapshot — pan/zoom per level + active navigation path.
/// Used to preserve view across full graph rebuilds (undo/redo).
pub struct ViewState {
    per_level: Vec<(Option<NodeId>, Vec2, f32)>,
    active_path: Vec<NodeId>,
}

/// Which context menu the right-click is currently showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextMenuMode {
    AddNode,
    Selection,
}

/// Right-click action on a Subgraph node that the app needs to handle
/// (because it requires interaction outside the graph itself — opening a
/// dialog, walking the graph for serialization, etc.).
#[derive(Debug, Clone)]
pub enum MacroRequest {
    /// Open "Save as macro" dialog for this Subgraph.
    SaveAs { node_id: NodeId, subgraph_path: Vec<NodeId> },
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
    context_menu_mode: ContextMenuMode,
    /// Pending macro action emitted by the right-click menu, consumed by the
    /// app each frame.
    pending_macro_request: Option<MacroRequest>,
    selected_nodes: Vec<usize>,
    /// Selected decorative-frame ids (parallel to `selected_nodes` but for
    /// frames; frames don't share index-space with nodes).
    selected_frames: std::collections::HashSet<u64>,
    /// Selected wires, keyed by their full `Connection` identity so they
    /// survive connection-vec re-ordering / removals without stale indices.
    selected_connections: std::collections::HashSet<Connection>,
    /// Port currently under the cursor, and a rolling sample buffer of its
    /// scalar value. Cleared when hover moves to a different port or off
    /// any port. Populated every frame so the tooltip scope has history.
    hover_port: Option<PortId>,
    hover_samples: std::collections::VecDeque<f32>,
    /// Per-active-level drag state for frames. None means "no frame drag in
    /// progress this frame". `frame_id` identifies the dragged frame; `mode`
    /// distinguishes title-bar drag (move) vs corner drag (resize).
    frame_drag: Option<FrameDrag>,
    canvas_rect: Rect,
    clipboard: Vec<ClipboardNode>,
    /// Set to true to fit the view to content on the next frame.
    fit_pending: bool,
}

#[derive(Clone)]
struct FrameDrag {
    frame_id: u64,
    mode: FrameDragMode,
    /// Node state-indices captured at drag start (Move mode only). These
    /// are the nodes "sitting on top of" the frame; they're dragged along
    /// with it so the frame feels like a container. Snapshotted once so
    /// nodes don't spontaneously attach/detach when the frame sweeps over
    /// them mid-drag.
    hitched_nodes: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq)]
enum FrameDragMode {
    Move,
    Resize,
}

#[derive(Clone, Copy)]
enum FrameHit {
    TitleBar(u64),
    Corner(u64),
    Body(u64),
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
                parent_level_idx: None,
                label: "Root".to_string(),
                frames: Vec::new(),
            }],
            active_level: 0,
            drag: DragState::default(),
            next_id: 1,
            registry: Vec::new(),
            new_nodes: Vec::new(),
            pending_engine_cmds: Vec::new(),
            context_menu_pos: None,
            context_menu_search: String::new(),
            context_menu_mode: ContextMenuMode::AddNode,
            pending_macro_request: None,
            selected_nodes: Vec::new(),
            selected_frames: std::collections::HashSet::new(),
            selected_connections: std::collections::HashSet::new(),
            hover_port: None,
            hover_samples: std::collections::VecDeque::with_capacity(128),
            frame_drag: None,
            canvas_rect: Rect::NOTHING,
            clipboard: Vec::new(),
            fit_pending: false,
        }
    }

    /// Add a new decorative frame at the given world position with default
    /// size and color. Returns the new frame's id.
    pub fn add_frame_at(&mut self, world_pos: Pos2) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let frame = GraphFrame {
            id,
            title: "Frame".to_string(),
            color: Color32::from_rgb(200, 160, 80),
            notes: String::new(),
            pos: world_pos,
            size: Vec2::new(280.0, 180.0),
        };
        self.active_mut().frames.push(frame);
        id
    }

    /// Read-only access to current level frames (for the inspector).
    pub fn frames(&self) -> &[GraphFrame] { &self.active().frames }
    /// Mutable access for the inspector to edit titles/colors/notes.
    pub fn frames_mut(&mut self) -> &mut Vec<GraphFrame> { &mut self.active_mut().frames }
    pub fn selected_frame_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.selected_frames.iter().copied()
    }
    pub fn deselect_all_frames(&mut self) { self.selected_frames.clear(); }

    /// Indices of nodes whose screen-rect centre currently falls inside the
    /// frame with id `frame_id`. Captured once at frame-drag start so those
    /// nodes ride along — centre-based so a node that's only partially over
    /// a frame only gets hitched when most of it is inside.
    fn nodes_on_frame(&self, frame_id: u64, node_rects: &[Rect]) -> Vec<usize> {
        let level = self.active();
        let z = level.zoom;
        let origin = self.canvas_rect.min.to_vec2() + level.pan;
        let Some(frame) = level.frames.iter().find(|f| f.id == frame_id) else {
            return Vec::new();
        };
        let world_rect = frame.rect();
        let screen_min = Pos2::new(world_rect.min.x * z, world_rect.min.y * z) + origin;
        let screen_max = Pos2::new(world_rect.max.x * z, world_rect.max.y * z) + origin;
        let screen_rect = Rect::from_min_max(screen_min, screen_max);
        node_rects.iter().enumerate()
            .filter(|(_, r)| screen_rect.contains(r.center()))
            .map(|(i, _)| i)
            .collect()
    }

    fn frame_hit_at(&self, pointer: Pos2, canvas_rect: Rect) -> Option<FrameHit> {
        let level = self.active();
        let z = level.zoom;
        let origin = canvas_rect.min.to_vec2() + level.pan;
        // Topmost frame wins; iterate in reverse.
        for f in level.frames.iter().rev() {
            let world_rect = f.rect();
            let screen_min = Pos2::new(world_rect.min.x * z, world_rect.min.y * z) + origin;
            let screen_max = Pos2::new(world_rect.max.x * z, world_rect.max.y * z) + origin;
            let screen_rect = Rect::from_min_max(screen_min, screen_max);
            if !screen_rect.contains(pointer) { continue; }
            // Hit-test sizes must match render sizes (scale with canvas zoom).
            let handle_size = 10.0 * z;
            let corner = Rect::from_min_size(
                Pos2::new(screen_rect.max.x - handle_size, screen_rect.max.y - handle_size),
                Vec2::splat(handle_size),
            );
            if corner.contains(pointer) { return Some(FrameHit::Corner(f.id)); }
            let title_h = 24.0 * z;
            let title_rect = Rect::from_min_size(
                screen_rect.min,
                Vec2::new(screen_rect.width(), title_h.min(screen_rect.height())),
            );
            if title_rect.contains(pointer) { return Some(FrameHit::TitleBar(f.id)); }
            return Some(FrameHit::Body(f.id));
        }
        None
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

    /// Returns the path of subgraph NodeIds from root to current level. We
    /// walk `parent_level_idx` explicitly — the levels Vec is a visit-order
    /// list, not a stack, so simple index slicing would include unrelated
    /// sibling levels pushed during earlier navigation.
    pub fn current_subgraph_path(&self) -> Vec<NodeId> {
        let mut chain: Vec<NodeId> = Vec::new();
        let mut idx = self.active_level;
        while idx != 0 {
            if let Some(sid) = self.levels[idx].subgraph_id {
                chain.push(sid);
            }
            match self.levels[idx].parent_level_idx {
                Some(p) => idx = p,
                None => break,
            }
        }
        chain.reverse();
        chain
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

    pub fn alloc_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Ensure the next id allocated is strictly greater than `id`.
    /// Used after loading state that may have reserved ids (frames, etc.).
    pub fn bump_next_id_above(&mut self, id: u64) {
        if id >= self.next_id {
            self.next_id = id + 1;
        }
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

    /// Screen rect currently occupied by the graph canvas.
    pub fn canvas_rect(&self) -> Rect { self.canvas_rect }

    /// Convert a screen-space point to world coords on the active level,
    /// matching the placement logic used by the add-node context menu.
    pub fn screen_to_world(&self, screen: Pos2) -> Pos2 {
        let level = self.active();
        (screen - self.canvas_rect.min.to_vec2() - level.pan) / level.zoom
    }

    /// Take any pending macro action emitted by the right-click menu.
    pub fn take_macro_request(&mut self) -> Option<MacroRequest> {
        self.pending_macro_request.take()
    }

    /// Iterate over all graph levels (root + open subgraph inner levels).
    pub fn all_levels(&self) -> impl Iterator<Item = &GraphLevel> {
        self.levels.iter()
    }

    /// Find the inner level for a given subgraph node ID, if it exists.
    pub fn find_level_for_subgraph(&self, subgraph_id: NodeId) -> Option<&GraphLevel> {
        self.levels.iter().find(|l| l.subgraph_id == Some(subgraph_id))
    }

    /// Snapshot per-level pan/zoom keyed by subgraph_id (None = root) plus
    /// the active subgraph navigation path. Used to preserve view state
    /// across apply_project (undo/redo).
    pub fn capture_view_state(&self) -> ViewState {
        ViewState {
            per_level: self.levels.iter()
                .map(|l| (l.subgraph_id, l.pan, l.zoom))
                .collect(),
            active_path: self.current_subgraph_path(),
        }
    }

    /// Restore pan/zoom on each existing level by matching subgraph_id,
    /// then navigate to the saved active subgraph path (best effort).
    pub fn restore_view_state(&mut self, state: &ViewState) {
        for (sid, pan, zoom) in &state.per_level {
            if let Some(level) = self.levels.iter_mut().find(|l| l.subgraph_id == *sid) {
                level.pan = *pan;
                level.zoom = *zoom;
            }
        }
        // Walk the active path one subgraph at a time, navigating into each.
        self.active_level = 0;
        for sid in &state.active_path {
            let idx = self.levels[self.active_level].nodes.iter()
                .position(|n| n.node_id() == *sid);
            if let Some(idx) = idx {
                self.navigate_into(idx);
            } else {
                break;
            }
        }
    }

    /// Create a node from the registry by type name.
    pub fn create_from_registry(&self, type_name: &str, id: NodeId) -> Option<Box<dyn NodeWidget>> {
        self.registry
            .iter()
            .find(|e| e.label == type_name)
            .map(|e| (e.factory)(id))
    }

    /// Find the port under the pointer (if any), push a fresh sample to
    /// the hover ring buffer, and render a tooltip next to the cursor
    /// showing the port's name + current value + a small scope.
    fn update_and_show_hover_tooltip(
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

                let inputs = level.nodes[i].ui_inputs();
                let mut base = 0usize;
                for (pi, up) in inputs.iter().enumerate() {
                    let pos = port_pos_z(rect.min, rect.width(), PortDir::Input, pi, z);
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
                    let pos = port_pos_z(rect.min, rect.width(), PortDir::Output, pi, z);
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
            let first = values.first().copied().unwrap_or(0.0);
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
    fn update_cursor_icon(
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
                let pos = port_pos_z(rect.min, rect.width(), PortDir::Output, pi, z);
                if pos.distance(pointer) < radius {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                    return;
                }
            }
            let ins = level.nodes[i].ui_inputs();
            for (pi, _) in ins.iter().enumerate() {
                let pos = port_pos_z(rect.min, rect.width(), PortDir::Input, pi, z);
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
            let title_rect = Rect::from_min_size(
                node_rects[i].min,
                Vec2::new(node_rects[i].width(), NODE_TITLE_HEIGHT * z),
            );
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
            if d <= tolerance {
                if best.as_ref().map(|(db, _)| d < *db).unwrap_or(true) {
                    best = Some((d, conn.clone()));
                }
            }
        }
        best.map(|(_, c)| c)
    }

    /// Bundle-connect helper: pair `bundle_size` consecutive ports starting
    /// at the source and target indices, skipping pairs where ports are
    /// missing or types don't match. `dc_from` is the port the user dragged
    /// from (could be input or output); `target` is the snapped destination.
    fn connect_bundle(&mut self, dc_from: PortId, target: PortId, bundle_size: usize) {
        let dc_from_dir = dc_from.dir;
        let level = self.active();
        let src_node_id = if dc_from_dir == PortDir::Output { dc_from.node } else { target.node };
        let dst_node_id = if dc_from_dir == PortDir::Output { target.node } else { dc_from.node };
        let src_start = if dc_from_dir == PortDir::Output { dc_from.index } else { target.index };
        let dst_start = if dc_from_dir == PortDir::Output { target.index } else { dc_from.index };

        let src_node_idx = match level.states.iter().position(|s| s.id == src_node_id) {
            Some(i) => i, None => return,
        };
        let dst_node_idx = match level.states.iter().position(|s| s.id == dst_node_id) {
            Some(i) => i, None => return,
        };
        let src_outs = level.nodes[src_node_idx].ui_outputs();
        let dst_ins = level.nodes[dst_node_idx].ui_inputs();

        let mut pairs: Vec<(usize, usize)> = Vec::with_capacity(bundle_size);
        for k in 0..bundle_size {
            let si = src_start + k;
            let di = dst_start + k;
            let (Some(src_port), Some(dst_port)) = (src_outs.get(si), dst_ins.get(di)) else {
                continue;
            };
            if src_port.disabled || dst_port.disabled { continue; }
            if !src_port.def.port_type.compatible_with(&dst_port.def.port_type) { continue; }
            pairs.push((si, di));
        }

        for (si, di) in pairs {
            let from = make_port_id(src_node_id, PortDir::Output, si);
            let to = make_port_id(dst_node_id, PortDir::Input, di);
            self.remove_connection_to(to);
            self.add_connection(from, to);
        }
    }

    pub fn add_connection(&mut self, from: PortId, to: PortId) {
        let conn = Connection { from, to };
        let level = self.active_mut();
        if !level.connections.contains(&conn) {
            level.connections.push(conn.clone());

            if let (Some(src_idx), Some(dst_idx)) = (
                level.states.iter().position(|s| s.id == from.node),
                level.states.iter().position(|s| s.id == to.node),
            ) {
                let src_type = level.nodes[src_idx]
                    .ui_outputs()
                    .get(from.index)
                    .map(|p| p.def.port_type)
                    .unwrap_or(PortType::Untyped);
                let dst_type = level.nodes[dst_idx]
                    .ui_inputs()
                    .get(to.index)
                    .map(|p| p.def.port_type)
                    .unwrap_or(PortType::Untyped);

                // Notify destination widget (input got connected).
                level.nodes[dst_idx].on_ui_connect(to.index, src_type);
                // Notify source widget (output got connected).
                level.nodes[src_idx].on_ui_output_connect(from.index, dst_type);

                // Notify engine for on_connect callback.
                self.push_engine_cmd(EngineCommand::NotifyConnect {
                    node_id: to.node,
                    input_port: to.index,
                    source_type: src_type,
                });
            }

            self.push_engine_cmd(EngineCommand::AddConnection(conn));
        }
    }

    fn remove_connection_to(&mut self, to: PortId) {
        let level = self.active_mut();
        let had = level.connections.iter().any(|c| c.to == to);
        // Capture the source port before the connection is removed so we can
        // notify the source widget that one of its outputs lost a connection.
        let removed_from = level.connections.iter().find(|c| c.to == to).map(|c| c.from);
        level.connections.retain(|c| c.to != to);
        if had {
            if let Some(dst_idx) = level.states.iter().position(|s| s.id == to.node) {
                level.nodes[dst_idx].on_ui_disconnect(to.index);
            }
            // Notify the source widget if its output is now unused (no other
            // connections from the same output port).
            if let Some(from) = removed_from {
                let still_connected = level.connections.iter().any(|c| c.from == from);
                if !still_connected
                    && let Some(src_idx) = level.states.iter().position(|s| s.id == from.node) {
                        level.nodes[src_idx].on_ui_output_disconnect(from.index);
                    }
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

    /// Remove connections whose endpoints no longer exist (index out of range)
    /// or whose port types have become incompatible (e.g. a node's mode changed
    /// and a previously-matching wire is now of the wrong type).
    fn cleanup_stale_connections(&mut self) {
        let level = self.active();
        let stale: Vec<PortId> = level.connections.iter().filter(|conn| {
            let src = level.nodes.iter()
                .zip(level.states.iter())
                .find(|(_, s)| s.id == conn.from.node)
                .and_then(|(n, _)| n.ui_outputs().get(conn.from.index).map(|p| p.def.port_type));
            let dst = level.nodes.iter()
                .zip(level.states.iter())
                .find(|(_, s)| s.id == conn.to.node)
                .and_then(|(n, _)| n.ui_inputs().get(conn.to.index).map(|p| p.def.port_type));
            match (src, dst) {
                (Some(s), Some(d)) => !s.compatible_with(&d),
                _ => true, // missing endpoint = stale
            }
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
                    ui.label(egui_phosphor::regular::CARET_RIGHT);
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

        // Right-click opens a context menu. The menu shown depends on whether
        // the click landed on a selected node:
        //   - selected node → selection actions ("Move to Subgraph", ...)
        //   - empty canvas / unselected node → add-node selector
        if response.secondary_clicked()
            && let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                self.context_menu_mode = if self.is_pos_on_selected_node(pos, canvas_rect) {
                    ContextMenuMode::Selection
                } else {
                    ContextMenuMode::AddNode
                };
                self.context_menu_pos = Some(pos);
                self.context_menu_search.clear();
            }

        let level = self.active();
        let z = level.zoom;
        let pan = level.pan;
        draw_grid(&painter, canvas_rect, pan, z);

        let origin = canvas_rect.min.to_vec2() + pan;

        // -- Draw decorative frames behind everything --
        for frame in &level.frames {
            let world_rect = frame.rect();
            let screen_min = Pos2::new(world_rect.min.x * z, world_rect.min.y * z) + origin;
            let screen_max = Pos2::new(world_rect.max.x * z, world_rect.max.y * z) + origin;
            let screen_rect = Rect::from_min_max(screen_min, screen_max);
            let selected = self.selected_frames.contains(&frame.id);

            // Body fill — frame's color at low alpha so nodes underneath
            // remain readable.
            let fill = Color32::from_rgba_unmultiplied(
                frame.color.r(), frame.color.g(), frame.color.b(), 40,
            );
            painter.rect_filled(screen_rect, 6.0, fill);

            // Title bar across the top with a slightly stronger fill.
            // All sizes scale linearly with `z` — the frame is a world-space
            // object and everything drawn on it should follow the canvas zoom
            // without floors.
            let title_h = 24.0 * z;
            let title_rect = Rect::from_min_size(
                screen_rect.min,
                Vec2::new(screen_rect.width(), title_h.min(screen_rect.height())),
            );
            let title_fill = Color32::from_rgba_unmultiplied(
                frame.color.r(), frame.color.g(), frame.color.b(), 110,
            );
            painter.rect_filled(title_rect, 6.0, title_fill);

            // Border, thicker when selected.
            let border = if selected {
                Stroke::new(2.5 * z, frame.color)
            } else {
                Stroke::new(1.5 * z, Color32::from_rgba_unmultiplied(
                    frame.color.r(), frame.color.g(), frame.color.b(), 180,
                ))
            };
            painter.rect_stroke(screen_rect, 6.0, border, egui::StrokeKind::Inside);

            if !frame.title.is_empty() {
                painter.text(
                    title_rect.left_center() + Vec2::new(8.0 * z, 0.0),
                    egui::Align2::LEFT_CENTER,
                    &frame.title,
                    egui::FontId::proportional(13.0 * z),
                    Color32::from_gray(230),
                );
            }

            // Notes shown under the title bar when there's room and the
            // frame is large enough to carry text.
            if !frame.notes.is_empty() && screen_rect.height() > title_h + 16.0 * z {
                let notes_pos = Pos2::new(
                    screen_rect.min.x + 8.0 * z,
                    title_rect.max.y + 4.0 * z,
                );
                painter.text(
                    notes_pos,
                    egui::Align2::LEFT_TOP,
                    &frame.notes,
                    egui::FontId::proportional(11.0 * z),
                    Color32::from_gray(180),
                );
            }

            // Resize handle in the bottom-right corner.
            let handle_size = 10.0 * z;
            let handle_rect = Rect::from_min_size(
                Pos2::new(screen_rect.max.x - handle_size, screen_rect.max.y - handle_size),
                Vec2::splat(handle_size),
            );
            painter.rect_filled(handle_rect, 1.0, Color32::from_rgba_unmultiplied(
                frame.color.r(), frame.color.g(), frame.color.b(), 200,
            ));
        }

        // -- Draw connections --
        for conn in &level.connections {
            if let (Some(from_pos), Some(from_type), Some(to_pos)) = (
                port_screen_pos(&level.nodes, &level.states, conn.from, origin, z),
                port_type_for(&level.nodes, &level.states, conn.from),
                port_screen_pos(&level.nodes, &level.states, conn.to, origin, z),
            ) {
                let selected = self.selected_connections.contains(conn);
                if selected {
                    // Soft white halo behind the wire, then redraw the wire
                    // on top so the type colour still reads.
                    draw_bezier(&painter, from_pos, to_pos,
                        theme::WIRE_HALO,
                        CONNECTION_THICKNESS + 4.0);
                }
                draw_bezier(&painter, from_pos, to_pos, from_type.color(),
                    CONNECTION_THICKNESS + if selected { 1.5 } else { 0.0 });
            }
        }

        // -- Draw in-progress connection --
        if let Some(dc) = &self.drag.drawing_conn {
            let color = dc.from_type.color().linear_multiply(0.7);
            draw_bezier(&painter, dc.from_pos, dc.to_pos, color, CONNECTION_THICKNESS);
            if dc.snap_target.is_some() {
                painter.circle_filled(dc.to_pos, PORT_RADIUS + 3.0, color.linear_multiply(0.3));
            }
            // Bundle badge near the cursor when armed for a multi-wire drop.
            if dc.bundle_size > 1 {
                let badge_pos = dc.to_pos + Vec2::new(14.0, -14.0);
                let label = format!("×{}", dc.bundle_size);
                let font = egui::FontId::proportional(13.0);
                let galley = painter.layout_no_wrap(label, font.clone(), Color32::WHITE);
                let pad = Vec2::new(6.0, 2.0);
                let bg_rect = egui::Rect::from_center_size(
                    badge_pos,
                    galley.size() + pad * 2.0,
                );
                painter.rect_filled(bg_rect, 4.0, Color32::from_rgba_unmultiplied(0, 0, 0, 200));
                painter.rect_stroke(
                    bg_rect, 4.0,
                    egui::Stroke::new(1.0, dc.from_type.color()),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    badge_pos,
                    egui::Align2::CENTER_CENTER,
                    format!("×{}", dc.bundle_size),
                    font,
                    Color32::WHITE,
                );
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

        // Figure out which portal keys are currently selected so we can
        // outline their peers — the "wireless cable" hint.
        let highlighted_portal_keys: std::collections::HashSet<String> = self
            .selected_nodes
            .iter()
            .filter_map(|&i| level.nodes.get(i).and_then(|n| n.portal_key()))
            .collect();

        // -- Draw node chrome (painter-based, immutable) --
        let now = ui.ctx().input(|i| i.time);
        for i in 0..level.nodes.len() {
            let selected = self.selected_nodes.contains(&i);
            let portal_peer = !selected
                && level.nodes[i].portal_key()
                    .map(|k| highlighted_portal_keys.contains(&k))
                    .unwrap_or(false);
            let inputs = level.nodes[i].ui_inputs();
            let outputs = level.nodes[i].ui_outputs();
            let out_highlights: Vec<f32> = (0..outputs.len())
                .map(|pi| level.nodes[i].output_highlight(pi, now))
                .collect();
            let in_highlights: Vec<f32> = (0..inputs.len())
                .map(|pi| level.nodes[i].input_highlight(pi, now))
                .collect();
            let disabled = level.nodes[i].shared_state().lock().unwrap().disabled;
            draw_node_chrome(
                &painter,
                level.nodes[i].title(),
                level.nodes[i].resizable(),
                level.nodes[i].accent_color(),
                &inputs,
                &outputs,
                &in_highlights,
                &out_highlights,
                disabled,
                node_rects[i],
                selected,
                portal_peer,
                z,
            );
        }

        // -- Draw node content (needs &mut node) --
        let canvas_clip = canvas_rect;
        let level = self.active_mut();
        for i in 0..level.nodes.len() {
            let rect = node_rects[i];
            let content_rect = node_content_rect(rect, z);
            // Intersect with the canvas so node UI doesn't paint over panels
            // (inspector, status bar) when a node is partially off-canvas.
            let clip_rect = content_rect.intersect(canvas_clip);
            if content_rect.width() > 0.0 && content_rect.height() > 0.0
                && clip_rect.width() > 0.0 && clip_rect.height() > 0.0 {
                let mut content_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(content_rect)
                        .layout(egui::Layout::top_down(egui::Align::LEFT)),
                );
                content_ui.set_clip_rect(clip_rect);
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

        // -- Disabled-node wash --
        // Drawn AFTER widget content so it actually covers things like the
        // fader's coloured fill or the XY pad's cyan knob. Near-opaque gray
        // so no colour from the widget's own painter leaks through.
        let level = self.active();
        for i in 0..level.nodes.len() {
            let disabled = level.nodes[i].shared_state().lock().unwrap().disabled;
            if !disabled { continue; }
            let rect = node_rects[i];
            let body_rect = Rect::from_min_max(
                Pos2::new(rect.min.x, rect.min.y + NODE_TITLE_HEIGHT * z),
                rect.max,
            );
            painter.rect_filled(
                body_rect, NODE_CORNER_RADIUS,
                theme::DISABLED_WASH,
            );
        }

        // -- Draw selection rectangle --
        // Solid outline = contain mode (drag top-left → bottom-right).
        // Dashed outline = crossing mode (drag bottom-right → top-left).
        if let Some(start) = self.drag.selection_rect_start
            && let Some(current) = ui.input(|i| i.pointer.hover_pos()) {
                let sel_rect = Rect::from_two_pos(start, current);
                let crossing = current.x < start.x;
                painter.rect_filled(sel_rect, 0.0, theme::SEM_PRIMARY_FILL);
                if crossing {
                    draw_dashed_rect(&painter, sel_rect, theme::SEM_PRIMARY);
                } else {
                    painter.rect_stroke(sel_rect, 0.0, Stroke::new(1.0, theme::SEM_PRIMARY), StrokeKind::Inside);
                }
            }

        // -- Handle interactions --
        self.handle_interactions(ui, &response, &node_rects, snap_to_grid);

        // -- Port-hover scope tooltip --
        // Skipped while a drag is in progress (wire draw / node move / frame)
        // since hovering a port then is almost always incidental.
        let drag_active = self.drag.drawing_conn.is_some()
            || self.drag.dragging_nodes
            || self.drag.resizing_node.is_some()
            || self.drag.panning
            || self.frame_drag.is_some()
            || self.drag.selection_rect_start.is_some();
        if !drag_active {
            self.update_and_show_hover_tooltip(ui, &response, &node_rects);
            self.update_cursor_icon(ui, &response, &node_rects);
        } else {
            self.hover_port = None;
            self.hover_samples.clear();
        }

        let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);

        // Only process keyboard shortcuts when no text field has focus.
        let text_has_focus = ui.ctx().memory(|m| m.focused().is_some());
        // For destructive shortcuts, additionally require the pointer to be
        // over the canvas — otherwise a stray Backspace from a side panel
        // could silently wipe selected nodes / frames.
        let pointer_over_canvas =
            !ui.ctx().is_pointer_over_area() && response.contains_pointer();

        // -- Delete selected nodes / frames / connections --
        if pointer_over_canvas
            && !text_has_focus
            && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
            && (!self.selected_nodes.is_empty()
                || !self.selected_frames.is_empty()
                || !self.selected_connections.is_empty())
        {
            self.delete_selected();
        }

        // -- Fit view to content (F or Home) --
        // Works even without pointer over canvas — handy when nodes end up
        // off-screen and you don't know where to pan to reach them.
        if !text_has_focus
            && ui.input(|i| i.key_pressed(egui::Key::F) || i.key_pressed(egui::Key::Home))
        {
            self.fit_to_content();
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
            if let Some(sub) = self.active_mut().nodes[i].as_any_mut().downcast_mut::<SubgraphWidget>()
                && sub.wants_open {
                    sub.wants_open = false;
                    open_subgraph_idx = Some(i);
                    break;
                }
        }
        if let Some(idx) = open_subgraph_idx {
            self.navigate_into(idx);
        }

        // -- Context menu --
        // `just_opened` suppresses the "click outside" dismiss check for the
        // very frame the menu opens (otherwise the right-click that triggered
        // it would also count as an outside click).
        let just_opened = response.secondary_clicked();
        self.show_context_menu(ui, canvas_rect, just_opened);
    }

    fn delete_selected(&mut self) {
        // Frames have no engine-side counterpart, so just drop them.
        if !self.selected_frames.is_empty() {
            let to_drop = self.selected_frames.clone();
            self.active_mut().frames.retain(|f| !to_drop.contains(&f.id));
            self.selected_frames.clear();
        }

        // Selected connections — route through `remove_connection_to` so
        // widgets get their disconnect callbacks (mode-autodetect nodes
        // reset to Neutral, etc.), same as any other wire removal.
        if !self.selected_connections.is_empty() {
            let to_drop: Vec<Connection> = self.selected_connections.drain().collect();
            for conn in to_drop {
                self.remove_connection_to(conn.to);
            }
        }

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

        let removed_ids: Vec<NodeId> = to_remove.iter()
            .map(|&i| self.active().states[i].id).collect();

        // Drop every connection touching a removed node *through* the regular
        // remove_connection_to path, so endpoint widgets get disconnect
        // callbacks (e.g. Scope resets its port type back to Any). This keeps
        // disconnect-on-delete behavior generic — widgets don't have to
        // implement anything extra.
        let affected_tos: Vec<PortId> = self.active().connections.iter()
            .filter(|c| removed_ids.contains(&c.from.node) || removed_ids.contains(&c.to.node))
            .map(|c| c.to)
            .collect();
        for to in affected_tos {
            self.remove_connection_to(to);
        }

        // Remove nodes in reverse order.
        let level = self.active_mut();
        for &i in to_remove.iter().rev() {
            level.nodes.remove(i);
            level.states.remove(i);
        }

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

    /// Close the open context menu when the user clicks outside its rect.
    /// `just_opened` skips the check on the same frame the menu opens, so the
    /// triggering right-click doesn't immediately count as an outside click.
    fn dismiss_on_outside_click(&mut self, ui: &Ui, area_rect: Rect, just_opened: bool) {
        if just_opened { return; }
        let any_press = ui.input(|i| i.pointer.any_pressed());
        if !any_press { return; }
        let pos = ui.input(|i| i.pointer.interact_pos());
        if let Some(p) = pos
            && !area_rect.contains(p) {
                self.context_menu_pos = None;
                self.context_menu_search.clear();
            }
    }

    /// For exactly two selected nodes, plan a one-shot pairwise connection
    /// from the leftmost node's outputs to the rightmost node's inputs.
    /// Skips disabled ports and pairs only type-compatible ones (in order).
    /// Returns `(source_node_id, destination_node_id, [(src_out, dst_in), ...])`.
    fn auto_connect_plan(&self) -> Option<(NodeId, NodeId, Vec<(usize, usize)>)> {
        if self.selected_nodes.len() != 2 { return None; }
        let level = self.active();
        let i_a = self.selected_nodes[0];
        let i_b = self.selected_nodes[1];
        if i_a >= level.nodes.len() || i_b >= level.nodes.len() { return None; }

        // Spatial direction: leftmost node = source.
        let (src_idx, dst_idx) = if level.states[i_a].pos.x <= level.states[i_b].pos.x {
            (i_a, i_b)
        } else {
            (i_b, i_a)
        };

        let src_id = level.states[src_idx].id;
        let dst_id = level.states[dst_idx].id;
        let outs = level.nodes[src_idx].ui_outputs();
        let ins = level.nodes[dst_idx].ui_inputs();

        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let mut o = 0usize;
        let mut i = 0usize;
        while o < outs.len() && i < ins.len() {
            if outs[o].disabled { o += 1; continue; }
            if ins[i].disabled { i += 1; continue; }
            if !outs[o].def.port_type.compatible_with(&ins[i].def.port_type) {
                // Advance source first; if next source matches, we'll pair
                // with the same input. Otherwise input also gets skipped.
                o += 1;
                continue;
            }
            pairs.push((o, i));
            o += 1;
            i += 1;
        }
        if pairs.is_empty() { return None; }
        Some((src_id, dst_id, pairs))
    }

    /// True if `screen_pos` lies inside any currently-selected node's screen rect.
    fn is_pos_on_selected_node(&self, screen_pos: Pos2, canvas_rect: Rect) -> bool {
        let level = self.active();
        let z = level.zoom;
        let origin = canvas_rect.min.to_vec2() + level.pan;
        for &i in &self.selected_nodes {
            if i >= level.nodes.len() { continue; }
            let n = &level.nodes[i];
            let min_w = n.min_width();
            let inputs = n.ui_inputs();
            let outputs = n.ui_outputs();
            let port_h = ports_height(inputs.len(), outputs.len());
            let content_h = PORT_START_Y + n.min_content_height() + NODE_PADDING;
            let min_h = port_h.max(content_h);
            let size = level.states[i].size_override
                .map(|s| Vec2::new(s.x.max(min_w), s.y.max(min_h)))
                .unwrap_or(Vec2::new(min_w, min_h));
            let pos = Pos2::new(level.states[i].pos.x * z, level.states[i].pos.y * z) + origin;
            let rect = Rect::from_min_size(pos, size * z);
            if rect.contains(screen_pos) {
                return true;
            }
        }
        false
    }

    fn show_context_menu(&mut self, ui: &mut Ui, canvas_rect: Rect, just_opened: bool) {
        if self.context_menu_pos.is_none() {
            return;
        }
        let menu_pos = self.context_menu_pos.unwrap();

        // Selection actions menu — shown only for right-clicks on a selected node.
        if self.context_menu_mode == ContextMenuMode::Selection {
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
                self.context_menu_search.clear();
                return;
            }
            if move_to_sub {
                self.move_selection_to_subgraph();
                self.context_menu_pos = None;
                self.context_menu_search.clear();
                return;
            }
            if let Some(id) = do_save_macro {
                let path = self.current_subgraph_path();
                self.pending_macro_request = Some(MacroRequest::SaveAs {
                    node_id: id,
                    subgraph_path: path,
                });
                self.context_menu_pos = None;
                self.context_menu_search.clear();
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
                self.context_menu_search.clear();
                return;
            }
            if let Some(sub_idx) = do_unpack {
                self.unpack_subgraph(sub_idx);
                self.context_menu_pos = None;
                self.context_menu_search.clear();
                return;
            }
            self.dismiss_on_outside_click(ui, area_resp.response.rect, just_opened);
            return;
        }

        // Add-node menu — default for right-clicks on empty canvas / unselected nodes.

        if self.registry.is_empty() {
            self.context_menu_pos = None;
            return;
        }

        let mut spawn: Option<usize> = None;
        let mut spawn_frame = false;
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

        let mut dismiss_via_esc = false;
        let area_resp = egui::Area::new(egui::Id::new("node_ctx_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu_pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_size(egui::Vec2::new(260.0, 420.0));
                    ui.set_max_width(260.0);

                    // Search field — auto-focused.
                    let search_resp = ui.add(
                        egui::TextEdit::singleline(&mut self.context_menu_search)
                            .hint_text("Search nodes...")
                            .desired_width(ui.available_width()),
                    );
                    if search_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        dismiss_via_esc = true;
                        return;
                    }
                    if search_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && let Some(&(idx, _, _, _)) = filtered.first() {
                            spawn = Some(idx);
                        }
                    search_resp.request_focus();

                    ui.separator();

                    if ui.button(egui::RichText::new("+ Add Frame").color(Color32::from_gray(200)))
                        .on_hover_text(
                            "Add a decorative frame for visually grouping nodes. \
                             Drag the title bar to move, drag the bottom-right corner to resize.",
                        )
                        .clicked()
                    {
                        spawn_frame = true;
                    }
                    ui.separator();

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
            });

        if dismiss_via_esc {
            self.context_menu_pos = None;
            self.context_menu_search.clear();
            return;
        }

        // Spawn check happens before dismiss so the click on a list item
        // doesn't get treated as "outside click then create node next frame".
        if spawn.is_none() && !spawn_frame {
            self.dismiss_on_outside_click(ui, area_resp.response.rect, just_opened);
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

        if spawn_frame {
            let level = self.active();
            let world = (menu_pos - canvas_rect.min.to_vec2() - level.pan) / level.zoom;
            let id = self.add_frame_at(world);
            // Select the new frame so the user can immediately edit it in
            // the inspector and see the selected-state border.
            self.selected_frames.clear();
            self.selected_frames.insert(id);
            self.selected_nodes.clear();
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
        let on_canvas = canvas_has_pointer && self.canvas_rect.contains(pointer_pos);
        let z = self.active().zoom;

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
                    (level.nodes[i].ui_inputs(), PortDir::Input)
                } else {
                    (level.nodes[i].ui_outputs(), PortDir::Output)
                };
                for (pi, ui_port) in ports.iter().enumerate() {
                    if ui_port.disabled {
                        continue;
                    }
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
                    let pos = port_pos_z(rect.min, rect.width(), PortDir::Output, pi, z);
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
                // --- Title-bar enable/disable toggle hit-test ---
                // Runs before the selection / drag logic so clicks on the
                // toggle never start a title drag.
                {
                    let mut toggle_hit: Option<usize> = None;
                    for i in (0..self.active().nodes.len()).rev() {
                        if i >= node_rects.len() { break; }
                        let t = title_toggle_rect(node_rects[i], z);
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

                    let title_rect = Rect::from_min_size(
                        node_rects[i].min,
                        Vec2::new(node_rects[i].width(), NODE_TITLE_HEIGHT * z),
                    );
                    // Don't start a drag when modifier-clicking — the user is
                    // just toggling selection membership.
                    if title_rect.contains(pointer_pos) && !double_clicked && !shift && !ctrl {
                        self.drag.dragging_nodes = true;
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

                    // Empty canvas: shift+drag = pan, plain drag = selection rect.
                    if shift {
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
        if let Some(parent) = self.levels[self.active_level].parent_level_idx {
            self.active_level = parent;
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
                } else if node.node_id() == BRIDGE_OUT_NODE_ID
                    && let Some(go) = node.as_any_mut().downcast_mut::<GraphOutputWidget>() {
                        go.update_ports(output_defs.clone());
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

            let parent = self.active_level;
            self.levels.push(GraphLevel {
                nodes,
                states,
                connections: Vec::new(),
                pan: Vec2::ZERO,
                zoom: 1.0,
                subgraph_id: Some(subgraph_id),
                parent_level_idx: Some(parent),
                label,
                frames: Vec::new(),
            });
            self.active_level = self.levels.len() - 1;
        }
        self.selected_nodes.clear();
        self.drag = DragState::default();
    }

    pub fn navigate_to_level(&mut self, level: usize) {
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

    /// Inverse of `move_selection_to_subgraph`: replace the Subgraph at
    /// `sub_idx` (in the active level) with its inner nodes and re-route its
    /// external wires through the bridges' inner connections.
    fn unpack_subgraph(&mut self, sub_idx: usize) {
        // Validate selection and capture the subgraph's id/pos up front.
        let (sub_id, sub_pos) = {
            let level = self.active();
            if sub_idx >= level.nodes.len() { return; }
            if level.nodes[sub_idx].type_name() != "Subgraph" { return; }
            (level.states[sub_idx].id, level.states[sub_idx].pos)
        };

        let inner_level_idx = match self.levels.iter().position(|l| l.subgraph_id == Some(sub_id)) {
            Some(i) => i,
            None => return,
        };

        // External connections touching the subgraph (in the parent level).
        let (external_incoming, external_outgoing): (Vec<(PortId, usize)>, Vec<(usize, PortId)>) = {
            let level = self.active();
            let mut inc = Vec::new();
            let mut out = Vec::new();
            for c in &level.connections {
                if c.to.node == sub_id { inc.push((c.from, c.to.index)); }
                if c.from.node == sub_id { out.push((c.from.index, c.to)); }
            }
            (inc, out)
        };

        // Partition inner connections into bridge-in / bridge-out / internal.
        let (bridge_in_conns, bridge_out_conns, internal_conns): (Vec<Connection>, Vec<Connection>, Vec<Connection>) = {
            let inner = &self.levels[inner_level_idx];
            let mut bi = Vec::new();
            let mut bo = Vec::new();
            let mut it = Vec::new();
            for c in &inner.connections {
                let from_br = c.from.node == BRIDGE_IN_NODE_ID;
                let to_br = c.to.node == BRIDGE_OUT_NODE_ID;
                if from_br && !to_br { bi.push(c.clone()); }
                else if !from_br && to_br { bo.push(c.clone()); }
                else if !from_br && !to_br { it.push(c.clone()); }
            }
            (bi, bo, it)
        };

        // Drain the inner widgets (skip bridges). Snapshot params + save_data
        // so the new engine nodes can be primed on their next tick.
        let mut taken: Vec<(Box<dyn NodeWidget>, NodeState, Vec<(usize, ParamValue)>, Option<serde_json::Value>)> = Vec::new();
        {
            let inner = &mut self.levels[inner_level_idx];
            let mut i = inner.nodes.len();
            while i > 0 {
                i -= 1;
                let id = inner.states[i].id;
                if id == BRIDGE_IN_NODE_ID || id == BRIDGE_OUT_NODE_ID { continue; }
                let node = inner.nodes.remove(i);
                let state = inner.states.remove(i);
                let (params, save_data) = {
                    let shared = node.shared_state().lock().unwrap();
                    let params: Vec<(usize, ParamValue)> = shared.current_params.iter().enumerate()
                        .map(|(pi, p)| {
                            let v = match p {
                                ParamDef::Float { value, .. } => ParamValue::Float(*value),
                                ParamDef::Int { value, .. } => ParamValue::Int(*value),
                                ParamDef::Bool { value, .. } => ParamValue::Bool(*value),
                                ParamDef::Choice { value, .. } => ParamValue::Choice(*value),
                            };
                            (pi, v)
                        })
                        .collect();
                    (params, shared.save_data.clone())
                };
                taken.push((node, state, params, save_data));
            }
            taken.reverse();
            // Clear the orphaned inner level's connections too; the inner
            // engine gets torn down as soon as we remove the subgraph.
            inner.connections.clear();
        }

        // Remove the subgraph node and its connections from the parent level.
        {
            let level = self.active_mut();
            level.connections.retain(|c| c.from.node != sub_id && c.to.node != sub_id);
            level.states.remove(sub_idx);
            level.nodes.remove(sub_idx);
        }
        self.push_engine_cmd(EngineCommand::RemoveNode(sub_id));

        // Spread the taken nodes around sub_pos, preserving their relative layout.
        let centroid: Vec2 = if taken.is_empty() {
            Vec2::ZERO
        } else {
            let sum: Vec2 = taken.iter()
                .map(|(_, s, _, _)| s.pos.to_vec2())
                .fold(Vec2::ZERO, |a, b| a + b);
            sum / (taken.len() as f32)
        };

        let parent_path = self.current_subgraph_path();
        let mut restore_info: Vec<(NodeId, Vec<(usize, ParamValue)>, Option<serde_json::Value>)> = Vec::new();
        let start_idx = self.active().nodes.len();
        {
            let level = self.active_mut();
            for (node, state, params, save_data) in taken {
                let node_id = node.node_id();
                let new_pos = Pos2::new(
                    state.pos.x - centroid.x + sub_pos.x,
                    state.pos.y - centroid.y + sub_pos.y,
                );
                level.states.push(NodeState::new(node_id, new_pos));
                if let Some(sz) = state.size_override {
                    level.states.last_mut().unwrap().size_override = Some(sz);
                }
                level.nodes.push(node);
                restore_info.push((node_id, params, save_data));
            }
        }
        let end_idx = self.active().nodes.len();
        for i in start_idx..end_idx {
            self.new_nodes.push(NewNode { index: i, subgraph_path: parent_path.clone() });
        }

        // Restore params/save_data on the relocated widgets.
        for (node_id, params, save_data) in restore_info {
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

        // Internal inner connections carry over as-is.
        for conn in &internal_conns {
            self.active_mut().connections.push(conn.clone());
            self.push_engine_cmd(EngineCommand::AddConnection(conn.clone()));
        }

        // External src -> subgraph.input[k], combined with BRIDGE_IN[k] -> inner_to,
        // becomes src -> inner_to.
        for bin in &bridge_in_conns {
            let k = bin.from.index;
            let inner_to = bin.to;
            for (src, ext_k) in &external_incoming {
                if *ext_k == k {
                    let c = Connection { from: *src, to: inner_to };
                    self.active_mut().connections.push(c.clone());
                    self.push_engine_cmd(EngineCommand::AddConnection(c));
                }
            }
        }
        // inner_from -> BRIDGE_OUT[k], combined with subgraph.output[k] -> dst,
        // becomes inner_from -> dst.
        for bout in &bridge_out_conns {
            let k = bout.to.index;
            let inner_from = bout.from;
            for (ext_k, dst) in &external_outgoing {
                if *ext_k == k {
                    let c = Connection { from: inner_from, to: *dst };
                    self.active_mut().connections.push(c.clone());
                    self.push_engine_cmd(EngineCommand::AddConnection(c));
                }
            }
        }

        self.selected_nodes.clear();
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


/// Rect of the title-bar enable/disable toggle (a small button at the
/// right edge of the title bar). Shared between the renderer and the
/// interaction hit-test so the click region and the visible glyph are
/// always aligned.
fn title_toggle_rect(node_rect: Rect, zoom: f32) -> Rect {
    let title_h = NODE_TITLE_HEIGHT * zoom;
    let btn_size = 14.0 * zoom;
    let inset = 5.0 * zoom;
    let cx = node_rect.max.x - inset - btn_size * 0.5;
    let cy = node_rect.min.y + title_h * 0.5;
    Rect::from_center_size(Pos2::new(cx, cy), Vec2::splat(btn_size))
}

fn draw_node_chrome(
    painter: &Painter,
    title: &str,
    resizable: bool,
    accent: Option<Color32>,
    inputs: &[UiPortDef],
    outputs: &[UiPortDef],
    in_highlights: &[f32],
    out_highlights: &[f32],
    disabled: bool,
    rect: Rect,
    selected: bool,
    portal_peer: bool,
    zoom: f32,
) {
    // Shadow
    let shadow_rect = rect.translate(Vec2::new(3.0, 3.0));
    painter.rect_filled(shadow_rect, NODE_CORNER_RADIUS, Color32::from_black_alpha(60));

    // Accent glow: a soft halo behind the node in the accent colour, so
    // "important" node kinds (subgraphs, portals…) stand out even when not
    // selected. Portal-peer glow takes precedence so linked-portal pairs
    // keep their amber indicator regardless of accent. All accents are
    // suppressed when disabled — a disabled node reads as pure grayscale
    // chrome so the "off" state is obvious at a glance.
    if !disabled {
        if portal_peer {
            let halo = rect.expand(3.0);
            painter.rect_filled(
                halo,
                NODE_CORNER_RADIUS + 3.0,
                theme::SEM_WARNING_HALO,
            );
        } else if let Some(a) = accent {
            let halo = rect.expand(2.5);
            painter.rect_filled(
                halo,
                NODE_CORNER_RADIUS + 2.5,
                Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), 50),
            );
        }
    }

    // Body — darker flat gray when disabled so the overlay later looks
    // uniform, regular dark when enabled.
    let body_bg = if disabled { theme::BG } else { theme::BG };
    painter.rect_filled(rect, NODE_CORNER_RADIUS, body_bg);

    // Title bar — accent-tinted when enabled; when disabled, strip the
    // accent and use a neutral darker gray so no colour leaks into the
    // title chrome.
    let title_bg = if disabled {
        theme::BG
    } else {
        accent
            .map(|a| mix_color(a, theme::BG_HIGH, 0.45))
            .unwrap_or(theme::BG_HIGH)
    };
    let title_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), NODE_TITLE_HEIGHT * zoom));
    painter.rect_filled(
        title_rect,
        egui::CornerRadius { nw: NODE_CORNER_RADIUS as u8, ne: NODE_CORNER_RADIUS as u8, sw: 0, se: 0 },
        title_bg,
    );
    // Title text is nudged slightly left so it doesn't crash into the
    // disable toggle we draw on the right edge.
    let title_text_center = Pos2::new(
        title_rect.center().x - 10.0 * zoom,
        title_rect.center().y,
    );
    let title_color = if disabled { theme::TEXT_DIM } else { theme::TEXT_BRIGHT };
    painter.text(
        title_text_center,
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(13.0 * zoom),
        title_color,
    );

    // Enable / disable toggle at the right of the title bar.
    let toggle = title_toggle_rect(rect, zoom);
    let (fg, stroke_col) = if disabled {
        (theme::STROKE, theme::TEXT_DIM)
    } else {
        (theme::SEM_SUCCESS, Color32::from_rgb(200, 240, 220))
    };
    // Outer ring.
    painter.circle_stroke(toggle.center(), toggle.width() * 0.4,
        Stroke::new(1.5 * zoom.max(0.5), stroke_col));
    // Inner "power" stub — a short vertical tick through the top of the ring.
    let tick_top = Pos2::new(toggle.center().x, toggle.center().y - toggle.height() * 0.38);
    let tick_bot = Pos2::new(toggle.center().x, toggle.center().y - toggle.height() * 0.08);
    painter.line_segment([tick_top, tick_bot], Stroke::new(1.5 * zoom.max(0.5), stroke_col));
    // Filled dot when enabled so the active state reads at a glance.
    if !disabled {
        painter.circle_filled(toggle.center(), toggle.width() * 0.15, fg);
    }

    // Border. Priority: selected > disabled > portal_peer > accent > default.
    // Disabled nodes render with neutral gray chrome — only "selected"
    // wins over it since selection must always be visible.
    let border = if selected {
        Stroke::new(2.0, theme::SEM_PRIMARY)
    } else if disabled {
        Stroke::new(1.0, theme::STROKE)
    } else if portal_peer {
        Stroke::new(2.0, theme::SEM_WARNING)
    } else if let Some(a) = accent {
        Stroke::new(1.5, a)
    } else {
        Stroke::new(1.0, theme::STROKE)
    };
    painter.rect_stroke(rect, NODE_CORNER_RADIUS, border, StrokeKind::Inside);

    // Input ports
    for (i, ui_port) in inputs.iter().enumerate() {
        let pos = port_pos_z(rect.min, rect.width(), PortDir::Input, i, zoom);
        let hi = in_highlights.get(i).copied().unwrap_or(0.0);
        draw_port(painter, pos, ui_port, hi, zoom, disabled);
    }

    // Output ports
    for (i, ui_port) in outputs.iter().enumerate() {
        let pos = port_pos_z(rect.min, rect.width(), PortDir::Output, i, zoom);
        let hi = out_highlights.get(i).copied().unwrap_or(0.0);
        draw_port(painter, pos, ui_port, hi, zoom, disabled);
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

/// Linear blend from `a` toward `b` by `t` (0..=1). Used to tint accent
/// colours with the neutral title-bar dark so white title text stays
/// readable even with punchy accents.
fn mix_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| ((1.0 - t) * x as f32 + t * y as f32).round() as u8;
    Color32::from_rgba_unmultiplied(
        lerp(a.r(), b.r()),
        lerp(a.g(), b.g()),
        lerp(a.b(), b.b()),
        255,
    )
}

/// Solid colour swatch for hovering Color ports. Reads channels 0..3 as RGB.
fn draw_color_preview(ui: &mut Ui, values: &[f32]) {
    let size = Vec2::new(180.0, 40.0);
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;
    let r = values.first().copied().unwrap_or(0.0).clamp(0.0, 1.0);
    let g = values.get(1).copied().unwrap_or(0.0).clamp(0.0, 1.0);
    let b = values.get(2).copied().unwrap_or(0.0).clamp(0.0, 1.0);
    painter.rect_filled(rect, 3.0,
        Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8));
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0, theme::STROKE), StrokeKind::Inside);
}

/// Four-swatch row for Palette ports. Channels are laid out as 4 × RGB.
fn draw_palette_preview(ui: &mut Ui, values: &[f32]) {
    let size = Vec2::new(180.0, 40.0);
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;
    let slot_w = rect.width() / 4.0;
    for i in 0..4 {
        let base = i * 3;
        let r = values.get(base).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let g = values.get(base + 1).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let b = values.get(base + 2).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let slot_rect = Rect::from_min_size(
            Pos2::new(rect.min.x + i as f32 * slot_w, rect.min.y),
            Vec2::new(slot_w, rect.height()),
        );
        painter.rect_filled(slot_rect, 0.0,
            Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8));
    }
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0, theme::STROKE), StrokeKind::Inside);
}

/// Sampled gradient preview (same renderer as Gradient Source etc., just
/// inlined here to avoid threading a `color::Gradient` helper through the
/// graph module).
fn draw_gradient_preview(ui: &mut Ui, values: &[f32]) {
    use crate::color::Gradient;
    let size = Vec2::new(200.0, 40.0);
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;

    // Checkerboard so alpha reads clearly.
    let cell = 5.0;
    let cols = (rect.width() / cell).ceil() as i32;
    let rows = (rect.height() / cell).ceil() as i32;
    for y in 0..rows {
        for x in 0..cols {
            let color = if (x + y) % 2 == 0 { theme::BG } else { theme::STROKE };
            let cr = Rect::from_min_size(
                Pos2::new(rect.min.x + x as f32 * cell, rect.min.y + y as f32 * cell),
                Vec2::splat(cell),
            ).intersect(rect);
            painter.rect_filled(cr, 0.0, color);
        }
    }

    let g = Gradient::from_channels(values);
    if !g.stops().is_empty() {
        let samples = (rect.width() as usize).max(16).min(256);
        for i in 0..samples {
            let t = i as f32 / (samples - 1).max(1) as f32;
            let x = rect.min.x + (i as f32 / samples as f32) * rect.width();
            let (rgb, alpha) = g.sample_with_alpha(t);
            let c = Color32::from_rgba_unmultiplied(
                (rgb.r.clamp(0.0, 1.0) * 255.0) as u8,
                (rgb.g.clamp(0.0, 1.0) * 255.0) as u8,
                (rgb.b.clamp(0.0, 1.0) * 255.0) as u8,
                (alpha.clamp(0.0, 1.0) * 255.0) as u8,
            );
            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(rect.width() / samples as f32 + 0.5, c),
            );
        }
    }
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0, theme::STROKE), StrokeKind::Inside);
}

/// Small pan/tilt crosshair preview. Pan 0..1 is x-axis, tilt 0..1 is y-axis.
fn draw_position_preview(ui: &mut Ui, pan: f32, tilt: f32) {
    let size = Vec2::new(80.0, 80.0);
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, 3.0, theme::BG_DEEP);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0, theme::STROKE), StrokeKind::Inside);
    let mid = rect.center();
    let guide = Stroke::new(0.5, theme::BG_HIGH);
    painter.line_segment([Pos2::new(rect.min.x, mid.y), Pos2::new(rect.max.x, mid.y)], guide);
    painter.line_segment([Pos2::new(mid.x, rect.min.y), Pos2::new(mid.x, rect.max.y)], guide);
    let dot = Pos2::new(
        rect.min.x + pan.clamp(0.0, 1.0) * rect.width(),
        rect.min.y + tilt.clamp(0.0, 1.0) * rect.height(),
    );
    painter.circle_filled(dot, 4.0, theme::SEM_SUCCESS);
    painter.circle_stroke(dot, 4.0, Stroke::new(1.0, Color32::from_gray(20)));
}

/// Mini scope for the port-hover tooltip. Plots the samples on a dark
/// background with a subtle zero line; y-scale auto-fits the observed
/// range (clamped to at least a small epsilon so constant signals still
/// render as a visible baseline).
fn draw_scope(ui: &mut Ui, samples: &[f32], line_color: Color32) {
    let size = Vec2::new(180.0, 60.0);
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, 3.0, theme::BG_DEEP);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0, theme::STROKE), StrokeKind::Inside);
    if samples.is_empty() { return; }
    let mut min_v = f32::INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    for &v in samples {
        if v < min_v { min_v = v; }
        if v > max_v { max_v = v; }
    }
    // Include 0 in the visible range when the signal is unipolar so the
    // baseline sits flush with the bottom instead of floating.
    if min_v > 0.0 { min_v = 0.0; }
    if max_v < 0.0 { max_v = 0.0; }
    let range = (max_v - min_v).max(1e-3);
    let n = samples.len();
    let dx = rect.width() / (n.max(2) - 1) as f32;

    // Zero line.
    let zero_y = rect.max.y - ((0.0 - min_v) / range) * rect.height();
    if zero_y >= rect.min.y && zero_y <= rect.max.y {
        painter.line_segment(
            [Pos2::new(rect.min.x, zero_y), Pos2::new(rect.max.x, zero_y)],
            Stroke::new(0.5, theme::BG_HIGH),
        );
    }

    // Signal line.
    let pts: Vec<Pos2> = samples.iter().enumerate().map(|(i, &v)| {
        let x = rect.min.x + i as f32 * dx;
        let y = rect.max.y - ((v - min_v) / range) * rect.height();
        Pos2::new(x, y)
    }).collect();
    if pts.len() >= 2 {
        for w in pts.windows(2) {
            painter.line_segment([w[0], w[1]], Stroke::new(1.5, line_color));
        }
    }
}

fn draw_port(
    painter: &Painter,
    pos: Pos2,
    ui_port: &UiPortDef,
    highlight: f32,
    zoom: f32,
    node_disabled: bool,
) {
    let type_color = ui_port.def.port_type.color();
    let fill = ui_port.fill_color.unwrap_or(type_color);
    let stroke_width = if ui_port.fill_color.is_some() { 3.0 } else { 1.5 };
    let r = PORT_RADIUS * zoom;
    // Outer glow when port is transiently highlighted.
    if highlight > 0.0 && !ui_port.disabled && !node_disabled {
        let alpha = (highlight.clamp(0.0, 1.0) * 255.0) as u8;
        let glow = Color32::from_rgba_unmultiplied(255, 255, 255, alpha);
        painter.circle_filled(pos, r + 4.0 * zoom, glow.linear_multiply(0.25));
        painter.circle_stroke(pos, r + 3.0 * zoom, Stroke::new(2.0, glow));
    }
    if ui_port.disabled || node_disabled {
        // Hollow grayed-out port. Node-disabled ports read the same as
        // individually-disabled ones so the whole node looks uniformly off.
        painter.circle_filled(pos, r, theme::BG);
        painter.circle_stroke(pos, r, Stroke::new(1.0, theme::STROKE));
    } else {
        painter.circle_filled(pos, r, fill);
        painter.circle_stroke(pos, r, Stroke::new(stroke_width, type_color));
    }
    // Optional centered glyph (e.g. "+" for the variadic add port).
    if let Some(glyph) = ui_port.marker {
        painter.text(
            pos,
            egui::Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional(11.0 * zoom),
            Color32::BLACK,
        );
    }
}

fn draw_grid(painter: &Painter, rect: Rect, pan: Vec2, zoom: f32) {
    painter.rect_filled(rect, 0.0, theme::BG_DEEP);

    let spacing = GRID_SPACING * zoom;
    let offset_x = pan.x.rem_euclid(spacing);
    let offset_y = pan.y.rem_euclid(spacing);

    let mut x = rect.min.x + offset_x;
    while x < rect.max.x {
        painter.line_segment(
            [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
            Stroke::new(1.0, theme::BG_GRID),
        );
        x += spacing;
    }
    let mut y = rect.min.y + offset_y;
    while y < rect.max.y {
        painter.line_segment(
            [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
            Stroke::new(1.0, theme::BG_GRID),
        );
        y += spacing;
    }
}

/// Dashed rectangle outline used to flag crossing-mode selection lassos.
fn draw_dashed_rect(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.0, color);
    let dash = 6.0;
    let gap = 4.0;
    let seg = dash + gap;

    let mut draw_dashed_segment = |from: Pos2, to: Pos2| {
        let diff = to - from;
        let len = diff.length();
        if len <= 0.0 { return; }
        let dir = diff / len;
        let mut d = 0.0f32;
        while d < len {
            let end = (d + dash).min(len);
            let a = from + dir * d;
            let b = from + dir * end;
            painter.line_segment([a, b], stroke);
            d += seg;
        }
    };

    draw_dashed_segment(rect.left_top(), rect.right_top());
    draw_dashed_segment(rect.right_top(), rect.right_bottom());
    draw_dashed_segment(rect.right_bottom(), rect.left_bottom());
    draw_dashed_segment(rect.left_bottom(), rect.left_top());
}

/// Is `inner` fully contained within `outer` (edges touching counts)?
fn rect_contains_rect(outer: Rect, inner: Rect) -> bool {
    outer.min.x <= inner.min.x
        && outer.min.y <= inner.min.y
        && outer.max.x >= inner.max.x
        && outer.max.y >= inner.max.y
}

/// Sample a wire bezier between `from` and `to` at `segments+1` points.
/// Shared by the renderer and hit-test paths so selection matches the
/// curve the user sees.
fn bezier_sample(from: Pos2, to: Pos2, segments: usize) -> Vec<Pos2> {
    let dx = (to.x - from.x).abs() * 0.5;
    let ctrl1 = Pos2::new(from.x + dx, from.y);
    let ctrl2 = Pos2::new(to.x - dx, to.y);
    let mut points = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
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
    points
}

fn draw_bezier(painter: &Painter, from: Pos2, to: Pos2, color: Color32, thickness: f32) {
    let points = bezier_sample(from, to, BEZIER_SEGMENTS);
    for w in points.windows(2) {
        painter.line_segment([w[0], w[1]], Stroke::new(thickness, color));
    }
}

/// Shortest distance from `p` to the polyline `pts`. Used to hit-test
/// clicks against wire beziers.
fn point_to_polyline_distance(p: Pos2, pts: &[Pos2]) -> f32 {
    let mut best = f32::INFINITY;
    for w in pts.windows(2) {
        let d = point_to_segment_distance(p, w[0], w[1]);
        if d < best { best = d; }
    }
    best
}

fn point_to_segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = Vec2::new(b.x - a.x, b.y - a.y);
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 < 1e-6 {
        return (p.to_vec2() - a.to_vec2()).length();
    }
    let ap = Vec2::new(p.x - a.x, p.y - a.y);
    let t = ((ap.x * ab.x + ap.y * ab.y) / len2).clamp(0.0, 1.0);
    let proj = Pos2::new(a.x + t * ab.x, a.y + t * ab.y);
    (p - proj).length()
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
