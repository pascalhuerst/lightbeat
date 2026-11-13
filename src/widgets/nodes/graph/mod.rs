mod quick_add;
mod frames;
mod links;
mod connections;
mod clipboard;
mod menus;
mod interact;
mod subgraph_ops;
mod draw;

use draw::*;

use egui::{self, Color32, CursorIcon, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::node::*;
use super::types::*;
use crate::engine::nodes::meta::subgraph::{SubgraphPortDef, port_type_to_idx, BRIDGE_IN_NODE_ID, BRIDGE_OUT_NODE_ID};
use crate::engine::types::{Connection, NodeId, ParamDef, ParamValue, PortDir, PortId, PortType, SubgraphInnerCmd};
use crate::widgets::nodes::meta::portal::{
    PortalReceiveWidget, PortalSendWidget,
};
use crate::widgets::nodes::meta::subgraph::{SubgraphWidget, GraphInputWidget, GraphOutputWidget};

const MAGNETIC_RADIUS: f32 = 20.0;
const BEZIER_SEGMENTS: usize = 40;
const MIN_BEZIER_SEGMENTS: usize = 8;
/// Straight-line distance covered by one bezier segment before it is subdivided.
const WIRE_SEGMENT_PX: f32 = 10.0;
const CONNECTION_THICKNESS: f32 = 2.5;
const NODE_CORNER_RADIUS: f32 = 6.0;

use crate::theme;

const GRID_SPACING: f32 = 30.0;

// ---------------------------------------------------------------------------
// Interaction state
// ---------------------------------------------------------------------------

const RESIZE_HANDLE_SIZE: f32 = 10.0;
/// Same distance egui uses to tell a click from a drag.
const BODY_DRAG_THRESHOLD: f32 = 6.0;

#[derive(Default)]
struct DragState {
    /// Dragging selected nodes by title bar.
    dragging_nodes: bool,
    /// Press on a header-less node's body: becomes a node drag once the
    /// pointer has moved further than a click allows.
    body_press: Option<Pos2>,
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

struct PortMenu {
    port: PortId,
    name: String,
    port_type: PortType,
    pos: Pos2,
}

/// Asking for the name of a portal being added: a portal that has none does
/// nothing, so it is not created until it has one.
struct PortalPrompt {
    registry_idx: usize,
    pos: Pos2,
    /// Receive picks an existing Send; Send types a new, unused name.
    receive: bool,
    name: String,
    focused: bool,
}

struct QuickAdd {
    /// Where the box sits on screen, and where the node will go.
    screen: Pos2,
    world: Pos2,
    query: String,
    selected: usize,
    focused: bool,
}


/// Gap between a node and a control added from one of its ports.
const ATTACH_GAP: f32 = 30.0;

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
    /// Where it is found by name: `input/midi`, `math/mul`.
    pub path: String,
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
    promoted: Vec<PromotedParam>,
    snapshots: crate::engine::types::SnapshotSet,
    hide_unused_ports: bool,
    data: Option<serde_json::Value>,
    /// Offset from the first copied node's position.
    offset: Vec2,
    /// The NodeId the snapshot was taken from. Used on paste to rewrite
    /// any captured connections (`ClipboardConn`) onto the freshly
    /// allocated ids.
    original_id: NodeId,
    /// For Subgraph copies: the inner level's `ProjectFile` so duplicate
    /// recreates the nested content (nodes, connections, frames, nested
    /// subgraphs, ...). `None` for non-subgraph copies.
    inner_graph: Option<crate::project::ProjectFile>,
}

/// A connection snapshot captured alongside `ClipboardNode`s — remembers
/// the source/destination by their *original* NodeIds so paste can remap
/// them onto the new node ids.
#[derive(Clone)]
struct ClipboardConn {
    from_node: NodeId,
    from_index: usize,
    to_node: NodeId,
    to_index: usize,
}

pub struct NodeGraph {
    levels: Vec<GraphLevel>,
    /// Names of all effect stacks, for the pickers in the effects' inspectors.
    stack_names: Option<crate::widgets::nodes::output::effect::SharedStackNames>,
    active_level: usize,
    drag: DragState,
    next_id: u64,
    registry: Vec<NodeEntry>,
    new_nodes: Vec<NewNode>,
    pending_engine_cmds: Vec<EngineCommand>,
    context_menu_pos: Option<Pos2>,
    /// Right-click menu on a port, offering a control or display to attach.
    port_menu: Option<PortMenu>,
    /// Type-to-add search opened by right-clicking empty canvas.
    quick_add: Option<QuickAdd>,
    /// A portal waiting for its name before it is put on the canvas.
    portal_prompt: Option<PortalPrompt>,
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
    /// Hash of each node's port names and types as of the last frame, to
    /// notice a mode switch (which reshapes the ports) and let go of the
    /// selected wires of that node.
    port_signatures: std::collections::HashMap<NodeId, u64>,
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
    /// Connections between the clipboard's nodes, stored by original
    /// NodeIds. On paste we rewrite these onto the newly-allocated ids so
    /// the duplicate keeps its internal wiring.
    clipboard_connections: Vec<ClipboardConn>,
    /// Set to true to fit the view to content on the next frame.
    fit_pending: bool,
    /// The overview in the corner of the canvas (toggle with M).
    pub show_minimap: bool,
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
            stack_names: None,
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
            port_menu: None,
            quick_add: None,
            portal_prompt: None,
            pending_macro_request: None,
            selected_nodes: Vec::new(),
            selected_frames: std::collections::HashSet::new(),
            selected_connections: std::collections::HashSet::new(),
            port_signatures: std::collections::HashMap::new(),
            hover_port: None,
            hover_samples: std::collections::VecDeque::with_capacity(128),
            frame_drag: None,
            canvas_rect: Rect::NOTHING,
            clipboard: Vec::new(),
            clipboard_connections: Vec::new(),
            fit_pending: false,
            show_minimap: true,
        }
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
        let (label, category): (String, String) = (label.into(), category.into());
        let path = super::paths::node_path(&category, &label);
        debug_assert!(self.registry.iter().all(|e| e.path != path), "two nodes have the path {path}");
        self.registry.push(NodeEntry {
            path,
            label,
            category,
            description,
            factory: Box::new(factory),
        });
    }

    /// Immutable view of the registered node types — used by the external
    /// "Add Nodes" panel to render the list without touching internals.
    pub fn registry_entries(&self) -> &[NodeEntry] {
        &self.registry
    }

    /// Spawn a new node from the registry at the given canvas-local position.
    /// Silently no-ops if the index is out of bounds.
    pub fn spawn_from_registry(&mut self, registry_idx: usize, canvas_pos: Pos2) -> Option<usize> {
        if registry_idx >= self.registry.len() { return None; }
        let label = self.registry[registry_idx].label.as_str();
        if matches!(label, "Portal Send" | "Portal Receive") {
            self.portal_prompt = Some(PortalPrompt {
                registry_idx,
                pos: canvas_pos,
                receive: label == "Portal Receive",
                name: String::new(),
                focused: false,
            });
            return None;
        }
        let id = self.alloc_id();
        let node = (self.registry[registry_idx].factory)(id);
        Some(self.add_node(node, canvas_pos))
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

    pub fn node_and_state(&self, index: usize) -> (&dyn NodeWidget, &NodeState) {
        let level = self.active();
        (level.nodes[index].as_ref(), &level.states[index])
    }

    #[allow(dead_code)]
    pub fn set_node_size(&mut self, index: usize, size: egui::Vec2) {
        self.active_mut().states[index].size_override = Some(size);
    }

    /// Move a node (identified by `NodeId`) on the active level to `pos`.
    /// No-op if the id isn't present. Used by project load to restore
    /// positions of nodes that aren't in the normal `nodes` vec (bridge
    /// pseudo-nodes).
    pub fn set_node_pos(&mut self, id: NodeId, pos: Pos2) {
        let level = self.active_mut();
        if let Some(s) = level.states.iter_mut().find(|s| s.id == id) {
            s.pos = pos;
        }
    }

    /// Access root level directly (for save/load which always operates on root).
    pub fn root_level(&self) -> &GraphLevel { &self.levels[0] }

    /// Get the active level index.
    pub fn active_level_index(&self) -> usize { self.active_level }

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

    pub fn show(&mut self, ui: &mut Ui, snap_to_grid: bool) {
        // -- Breadcrumb bar when inside a subgraph --
        // Walk from the active level up via `parent_level_idx` to get the
        // actual ancestor chain — `self.levels` is a flat Vec where
        // siblings live at adjacent indices, so iterating `1..=active_level`
        // does NOT give a valid path.
        if self.active_level > 0 {
            let mut crumbs: Vec<(usize, String)> = Vec::new();
            let mut idx = self.active_level;
            // Walk up through ancestor subgraph levels. Stop before level 0
            // (root) — it's rendered as a hardcoded "Root" button below
            // rather than a crumb, so including it here would duplicate it.
            while idx != 0 {
                crumbs.push((idx, self.levels[idx].label.clone()));
                match self.levels[idx].parent_level_idx {
                    Some(p) if p != idx => idx = p,
                    _ => break,
                }
            }
            crumbs.reverse(); // root-most first.
            ui.horizontal(|ui| {
                if ui.small_button("Root").clicked() {
                    self.navigate_to_level(0);
                }
                let last = crumbs.len().saturating_sub(1);
                for (i, (level_idx, label)) in crumbs.iter().enumerate() {
                    ui.label(egui_phosphor::regular::CARET_RIGHT);
                    if i < last {
                        if ui.small_button(label).clicked() {
                            self.navigate_to_level(*level_idx);
                        }
                    } else {
                        ui.strong(label);
                    }
                }
            });
            ui.separator();
        }

        self.reconcile_effect_stacks();
        self.remap_promoted_connections();
        self.remap_removed_ports();
        self.refresh_hidden_ports();
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

        // A mouse wheel zooms around the pointer, Ctrl+wheel and pinch too; a
        // trackpad (scrolling in points) pans, so two fingers move the view.
        if response.contains_pointer() && !self.minimap_rect(canvas_rect).contains(ui.input(|i| i.pointer.hover_pos()).unwrap_or_default()) {
            let (wheel_lines, wheel_points, ctrl, pinch) = ui.input(|i| {
                let mut lines = 0.0_f32;
                let mut points = Vec2::ZERO;
                for e in &i.events {
                    if let egui::Event::MouseWheel { unit, delta, .. } = e {
                        match unit {
                            egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => lines += delta.y,
                            egui::MouseWheelUnit::Point => points += *delta,
                        }
                    }
                }
                (lines, points, i.modifiers.ctrl || i.modifiers.command, i.zoom_delta())
            });
            let mut zoom_factor = pinch;
            if wheel_lines != 0.0 {
                zoom_factor *= 1.0 + wheel_lines * 0.1;
            }
            if wheel_points != Vec2::ZERO {
                if ctrl {
                    zoom_factor *= 1.0 + wheel_points.y * 0.004;
                } else {
                    self.active_mut().pan += wheel_points;
                }
            }
            if (zoom_factor - 1.0).abs() > f32::EPSILON {
                let level = self.active_mut();
                let new_zoom = (level.zoom * zoom_factor).clamp(0.2, 3.0);
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

        // Right-click on a selected node opens the selection actions menu
        // ("Move to Subgraph", ...). Right-click on empty canvas is a no-op —
        // nodes are added via the "Add Nodes" side panel (drag onto canvas).
        if response.secondary_clicked()
            && let Some(pos) = ui.input(|i| i.pointer.hover_pos())
            && self.is_pos_on_selected_node(pos, canvas_rect) {
                self.context_menu_pos = Some(pos);
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
                    egui::Stroke::new(1.0_f32, dc.from_type.color()),
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
                let size = node_size(level.nodes[i].as_ref(), level.states[i].size_override);
                let pos = Pos2::new(
                    level.states[i].pos.x * z,
                    level.states[i].pos.y * z,
                ) + origin;
                Rect::from_min_size(pos, size * z)
            })
            .collect();

        // Figure out which portal keys are currently selected so we can
        // outline their peers — the "wireless cable" hint.
        let highlighted_link_keys: std::collections::HashSet<String> = self
            .selected_nodes
            .iter()
            .filter_map(|&i| level.nodes.get(i).and_then(|n| n.link_key()))
            .collect();

        // -- Draw node chrome (painter-based, immutable) --
        let now = ui.ctx().input(|i| i.time);
        let mut any_peer_pulsing = false;
        for i in 0..level.nodes.len() {
            let selected = self.selected_nodes.contains(&i);
            // Every member of the group pulses, the selected one too: seeing
            // all of them at once is the point.
            let portal_peer = level.nodes[i].link_key()
                .map(|k| highlighted_link_keys.contains(&k))
                .unwrap_or(false);
            if portal_peer { any_peer_pulsing = true; }
            let inputs = level.nodes[i].ui_inputs_full();
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
                level.nodes[i].display_title(),
                level.nodes[i].resizable(),
                level.nodes[i].accent_color(),
                level.nodes[i].accent_icon(),
                &inputs,
                &outputs,
                &level.nodes[i].hidden_ports(PortDir::Input),
                &level.nodes[i].hidden_ports(PortDir::Output),
                &in_highlights,
                &out_highlights,
                disabled,
                node_rects[i],
                &level.nodes[i].layout(),
                selected,
                portal_peer,
                level.nodes[i].show_port_labels(),
                now,
                z,
            );
        }
        // While any portal peer is pulsing, keep the canvas repainting so
        // the animation is visible without further user input.
        if any_peer_pulsing {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }

        // -- Draw node content (needs &mut node) --
        let canvas_clip = canvas_rect;
        let level = self.active_mut();
        for i in 0..level.nodes.len() {
            let rect = node_rects[i];
            let content_rect = content_rect(rect, &level.nodes[i].layout(), z);
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
                Pos2::new(rect.min.x, rect.min.y + level.nodes[i].layout().title_h * z),
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
                    painter.rect_stroke(sel_rect, 0.0, Stroke::new(1.0_f32, theme::SEM_PRIMARY), StrokeKind::Inside);
                }
            }

        // -- Handle interactions --
        self.handle_interactions(ui, &response, &node_rects, snap_to_grid);
        self.show_minimap(ui, canvas_rect);

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
            self.fit_pending = true;
        }

        if !text_has_focus && !ctrl && ui.input(|i| i.key_pressed(egui::Key::M)) {
            self.show_minimap = !self.show_minimap;
        }

        // -- Hide / show unused ports (H) --
        if !text_has_focus
            && !ctrl
            && ui.input(|i| i.key_pressed(egui::Key::H))
            && !self.selected_nodes.is_empty()
        {
            self.toggle_hide_unused_ports();
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
        if just_opened
            && let Some(p) = ui.input(|i| i.pointer.hover_pos())
        {
            self.port_menu = self.port_menu_at(p, &node_rects);
            if self.port_menu.is_some() {
                self.context_menu_pos = None;
            }
        }
        // Right-click on empty canvas: type the name of a node to add it.
        if just_opened
            && self.port_menu.is_none()
            && self.context_menu_pos.is_none()
            && let Some(p) = ui.input(|i| i.pointer.hover_pos())
            && !node_rects.iter().any(|r| r.contains(p))
        {
            self.quick_add = Some(QuickAdd {
                screen: p,
                world: self.screen_to_world(p),
                query: String::new(),
                selected: 0,
                focused: false,
            });
        } else if just_opened {
            self.quick_add = None;
        }
        self.show_context_menu(ui, canvas_rect, just_opened);
        self.show_port_menu(ui, just_opened);
        self.show_quick_add(ui, just_opened);
        self.show_portal_prompt(ui.ctx());
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
            }
    }
}
