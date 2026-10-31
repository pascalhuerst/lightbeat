use std::any::Any;

use egui::{Pos2, Rect, Ui, Vec2};

use crate::engine::types::{NodeId, ParamDef, ParamValue, PortDef, PortDir, SharedState};

use super::types::UiPortDef;

/// An effect as its stack lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct StackMember {
    pub node_id: NodeId,
    pub label: String,
    pub kind: &'static str,
}

// ---------------------------------------------------------------------------
// NodeWidget trait (UI-only)
// ---------------------------------------------------------------------------

/// Trait for the UI side of a node. Handles rendering only.
/// Processing logic lives in the engine's ProcessNode.
#[allow(dead_code)]
pub trait NodeWidget: Any {
    fn node_id(&self) -> NodeId;
    fn type_name(&self) -> &'static str;
    /// User-assigned label for this specific instance. Default: empty,
    /// meaning "no custom title set — fall back to the type name wherever
    /// a display label is needed". Nodes with a name/label field should
    /// return that field's current value; empty string signals "unset".
    fn title(&self) -> &str { "" }

    /// What to render in the canvas title bar and breadcrumbs: the user's
    /// title if they've set one, otherwise the node type. Not meant to be
    /// overridden by individual widgets.
    fn display_title(&self) -> &str {
        let t = self.title();
        if t.is_empty() { self.type_name() } else { t }
    }

    /// UI port definitions (may include fill colors for display).
    fn ui_inputs(&self) -> Vec<UiPortDef>;
    fn ui_outputs(&self) -> Vec<UiPortDef>;

    /// Draw the custom content area inside the node body.
    /// `zoom` is the current canvas zoom level for scaling text/UI elements.
    fn show_content(&mut self, ui: &mut Ui, zoom: f32);

    fn min_width(&self) -> f32 { 140.0 }
    fn min_content_height(&self) -> f32 { 0.0 }
    fn resizable(&self) -> bool { false }

    /// Optional accent colour. When set, the node's title bar is tinted
    /// with a darkened mix of this colour (white title text stays readable)
    /// and its border uses the colour at full saturation. Useful for
    /// making special node kinds (subgraphs, portals) stand out against
    /// the default gray chrome.
    fn accent_color(&self) -> Option<egui::Color32> { None }

    /// Optional icon glyph rendered in the title bar before the node's
    /// title text. Typically an `egui_phosphor::regular::*` constant. Used
    /// by special node kinds (portals, macros) to add a visual hook that
    /// distinguishes them at a glance from generic nodes.
    fn accent_icon(&self) -> Option<&'static str> { None }

    /// Nodes that belong to a "linked set" (portals with the same name,
    /// effects and the stack they belong to) return the key that identifies
    /// the set. When the user selects a node with a link key, the graph
    /// renderer draws a matching outline around every other node sharing the
    /// same key, so you can see the "wireless" peers at a glance. Default:
    /// not part of any linked set.
    fn link_key(&self) -> Option<String> { None }

    /// An Effect Stack returns the name its effects link to.
    fn stack_name(&self) -> Option<String> { None }

    /// An effect returns the name of the stack it is linked to, if any.
    fn linked_stack(&self) -> Option<String> { None }

    /// Called when the node is created from a port's right-click menu, with
    /// that port's name, so a control can be labelled after what it drives.
    fn set_name(&mut self, _name: &str) {}

    /// What a constant feeding this node's input `port` should start at, so
    /// it changes nothing until the user edits it: 1 for a multiplier, 0
    /// for most others.
    fn neutral_input_value(&self, _port: usize) -> f32 { 0.0 }

    /// Called on a constant created from a port's right-click menu.
    fn set_initial_value(&mut self, _value: f32) {}

    /// "Loop" or "Burst" for an effect, shown in its stack's list.
    fn effect_kind(&self) -> &'static str { "" }

    /// A stack that was renamed since the last call returns (old, new), so
    /// the graph can move its effects over to the new name.
    fn take_stack_rename(&mut self) -> Option<(String, String)> { None }

    /// An effect linked to `from` follows the stack to its new name.
    fn rename_linked_stack(&mut self, _from: &str, _to: &str) {}

    /// Called every frame on a stack with the effects currently linked to it.
    fn set_stack_members(&mut self, _members: Vec<StackMember>) {}

    /// How the node uses its rectangle. Most nodes keep the standard layout
    /// (title bar with an enable switch, ports below, a padded body); a
    /// control that is the whole point of its node, like a button or a fader,
    /// uses `NodeLayout::compact()` instead.
    fn layout(&self) -> NodeLayout { NodeLayout::STANDARD }

    /// Short human-readable description of what this node does.
    /// Shown in the inspector and as a tooltip in the context menu.
    fn description(&self) -> &'static str { "" }

    /// Show extra info/visuals in the inspector (e.g. scope waveform).
    fn show_inspector(&mut self, _ui: &mut Ui) {}

    /// Called by the UI when an INPUT connection is made/broken
    /// (for updating port colors and auto-detecting node mode).
    fn on_ui_connect(&mut self, _input_port: usize, _source_type: crate::engine::types::PortType) {}
    fn on_ui_disconnect(&mut self, _input_port: usize) {}

    /// Called by the UI when an OUTPUT connection is made/broken.
    /// Enables auto-detection on the output side as well.
    fn on_ui_output_connect(&mut self, _output_port: usize, _dest_type: crate::engine::types::PortType) {}
    fn on_ui_output_disconnect(&mut self, _output_port: usize) {}

    /// Access the shared state for reading port values and pushing param changes.
    fn shared_state(&self) -> &SharedState;

    /// Transient highlight intensity (0..=1) for output port `port_idx`.
    /// Used to visually flash the corresponding port when a UI element
    /// (button in a group, fader cell) is interacted with. Default: no highlight.
    fn output_highlight(&self, _port_idx: usize, _now: f64) -> f32 { 0.0 }

    /// Same as `output_highlight` but for input ports.
    fn input_highlight(&self, _port_idx: usize, _now: f64) -> f32 { 0.0 }

    /// When true, the inspector skips the default Inputs/Outputs section.
    /// Use when the widget renders its own per-port summary in `show_inspector`.
    fn inspector_hides_default_ports(&self) -> bool { false }

    /// When true, the canvas renderer draws each port's name next to its
    /// port dot (labels auto-truncate with ellipsis if they don't fit, and
    /// are skipped entirely if nothing can fit). Meant for nodes that
    /// render nothing in their content area (Portal, Graph Input/Output)
    /// — otherwise the label would overlap the widget's own UI. Default: off.
    fn show_port_labels(&self) -> bool { false }

    /// Param indices that are currently overridden by a wired input and
    /// should be hidden from the auto-inspector. Pattern: when an input
    /// port is wired, the wire's value supersedes the matching param;
    /// the param is omitted from the inspector to avoid showing a value
    /// the user can't actually change. Implementations typically read
    /// `shared_state().lock().inputs_connected[port_idx]` to compute this.
    /// Default: nothing is overridden.
    fn overridden_param_indices(&self) -> Vec<usize> { Vec::new() }

    /// How the inspector and a promoted port show a param: an optional group
    /// heading it is listed under, and its label. Params are keyed by name,
    /// so a node whose names are internal can present them differently here.
    fn param_display(&self, name: &str) -> (Option<String>, String) {
        (None, name.to_string())
    }

    fn as_any_mut(&mut self) -> &mut dyn Any;
}

// Convenience methods that delegate to shared state.
#[allow(dead_code)]
impl dyn NodeWidget {
    pub fn read_input(&self, port_index: usize) -> f32 {
        let shared = self.shared_state().lock().unwrap();
        shared.inputs.get(port_index).copied().unwrap_or(0.0)
    }

    pub fn read_output(&self, port_index: usize) -> f32 {
        let shared = self.shared_state().lock().unwrap();
        shared.outputs.get(port_index).copied().unwrap_or(0.0)
    }

    pub fn params(&self) -> Vec<ParamDef> {
        self.shared_state().lock().unwrap().current_params.clone()
    }

    /// The widget's own inputs followed by one port per promoted param. The
    /// graph must use this, not `ui_inputs`, wherever it deals with ports.
    pub fn ui_inputs_full(&self) -> Vec<UiPortDef> {
        let mut ports = self.ui_inputs();
        let shared = self.shared_state().lock().unwrap();
        ports.extend(shared.promoted.iter().map(|p| {
            let (group, label) = self.param_display(&p.name);
            let name = match group {
                Some(g) => format!("{g}: {label}"),
                None => p.name.clone(),
            };
            UiPortDef::from_def(&PortDef::new(name, p.kind.port_type()))
        }));
        ports
    }

    pub fn set_param(&self, index: usize, value: ParamValue) {
        // Push param change into shared state; engine will pick it up.
        // We store it in outputs temporarily — but actually we need a
        // dedicated channel. For now, we'll use the engine command channel
        // from the graph level. This method is here for the trait API.
        let _ = (index, value); // handled at graph level
    }
}

// ---------------------------------------------------------------------------
// NodeState (UI-only: position, size)
// ---------------------------------------------------------------------------

pub struct NodeState {
    pub id: NodeId,
    pub pos: Pos2,
    pub size_override: Option<egui::Vec2>,
}

impl NodeState {
    pub fn new(id: NodeId, pos: Pos2) -> Self {
        Self {
            id,
            pos,
            size_override: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Layout constants and helpers
// ---------------------------------------------------------------------------

pub const NODE_TITLE_HEIGHT: f32 = 24.0;
pub const PORT_RADIUS: f32 = 6.0;
pub const PORT_SPACING: f32 = 22.0;
pub const PORT_START_Y: f32 = NODE_TITLE_HEIGHT + 14.0;
pub const NODE_PADDING: f32 = 8.0;
const COMPACT_TITLE_HEIGHT: f32 = 14.0;

/// How a node uses its rectangle, in unzoomed pixels. The canvas asks the
/// node for this wherever it needs a size, a port position or a hit area, so a
/// node can change its shape without the canvas special-casing it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeLayout {
    /// Height of the title bar, which is also the handle for moving the node.
    pub title_h: f32,
    /// Distance from the top of the node to the top of the widget's area.
    pub content_top: f32,
    /// Distance from the top of the node to the first port.
    pub port_start_y: f32,
    /// Space left under the content.
    pub bottom_pad: f32,
    /// Space left to either side of the content.
    pub side_inset: f32,
    /// Show the switch that disables the node.
    pub toggle: bool,
    /// No body fill, shadow or border of its own: the widget is the node.
    /// Selection and links still outline it.
    pub bare: bool,
    /// A lone port on a side sits at the middle of the content, on its edge.
    pub centre_single_port: bool,
    /// Write the node's title in the title bar.
    pub show_title: bool,
    /// No title bar at all: the node is moved by dragging its body, and a
    /// click without movement still reaches the widget.
    pub drag_by_body: bool,
}

impl NodeLayout {
    pub const STANDARD: Self = Self {
        title_h: NODE_TITLE_HEIGHT,
        content_top: PORT_START_Y,
        port_start_y: PORT_START_Y,
        bottom_pad: NODE_PADDING,
        side_inset: PORT_RADIUS + 8.0,
        toggle: true,
        bare: false,
        centre_single_port: false,
        show_title: true,
        drag_by_body: false,
    };

    /// For a control that fills its node: a slim header to grab, no frame, no
    /// switch, and the ports on the control's own edge.
    pub const fn compact() -> Self {
        Self {
            title_h: COMPACT_TITLE_HEIGHT,
            content_top: COMPACT_TITLE_HEIGHT,
            port_start_y: COMPACT_TITLE_HEIGHT + 12.0,
            bottom_pad: 0.0,
            side_inset: 0.0,
            toggle: false,
            bare: true,
            centre_single_port: true,
            show_title: true,
            drag_by_body: false,
        }
    }

    /// Keep the content clear of the ports, for controls with text or scales
    /// near their edges.
    pub const fn with_side_inset(mut self, inset: f32) -> Self {
        self.side_inset = inset;
        self
    }

    /// Keep the slim header empty when the control already shows its name.
    pub const fn without_title(mut self) -> Self {
        self.show_title = false;
        self
    }

    /// Drop the header altogether; dragging the control moves the node.
    pub const fn headerless(mut self) -> Self {
        self.title_h = 0.0;
        self.content_top = 0.0;
        self.port_start_y = 12.0;
        self.show_title = false;
        self.drag_by_body = true;
        self
    }
}

/// Port position for a node drawn in `rect` (screen space) with `layout`.
pub fn port_pos_in(
    rect: Rect,
    layout: &NodeLayout,
    dir: PortDir,
    port_types: &[crate::engine::types::PortType],
    index: usize,
    zoom: f32,
) -> Pos2 {
    let x = match dir {
        PortDir::Input => rect.min.x,
        PortDir::Output => rect.max.x,
    };
    if layout.centre_single_port && port_types.len() == 1 {
        let top = rect.min.y + layout.content_top * zoom;
        return Pos2::new(x, top + (rect.max.y - top) * 0.5);
    }
    let offsets = compute_port_offsets(port_types);
    let offset = offsets.get(index).copied().unwrap_or(index as f32 * PORT_SPACING);
    Pos2::new(x, rect.min.y + (layout.port_start_y + offset) * zoom)
}

/// Smallest height that shows every port of a node with this layout.
fn ports_min_height(
    layout: &NodeLayout,
    input_types: &[crate::engine::types::PortType],
    output_types: &[crate::engine::types::PortType],
) -> f32 {
    if layout.centre_single_port && input_types.len() <= 1 && output_types.len() <= 1 {
        return layout.content_top + PORT_SPACING;
    }
    let in_h = compute_port_offsets(input_types).last().copied().unwrap_or(PORT_SPACING);
    let out_h = compute_port_offsets(output_types).last().copied().unwrap_or(PORT_SPACING);
    layout.port_start_y + in_h.max(out_h).max(PORT_SPACING) + layout.bottom_pad
}

/// The node's size in unzoomed pixels: what its widget asks for, unless the
/// user has resized it, and never less than its ports need.
pub fn node_size(node: &dyn NodeWidget, size_override: Option<Vec2>) -> Vec2 {
    let layout = node.layout();
    let in_types: Vec<_> = node.ui_inputs_full().iter().map(|p| p.def.port_type).collect();
    let out_types: Vec<_> = node.ui_outputs().iter().map(|p| p.def.port_type).collect();
    let min_w = node.min_width();
    let content_h = layout.content_top + node.min_content_height() + layout.bottom_pad;
    let min_h = ports_min_height(&layout, &in_types, &out_types).max(content_h);
    size_override
        .map(|s| Vec2::new(s.x.max(min_w), s.y.max(min_h)))
        .unwrap_or(Vec2::new(min_w, min_h))
}

/// Where the widget draws, in screen space.
pub fn content_rect(rect: Rect, layout: &NodeLayout, zoom: f32) -> Rect {
    Rect::from_min_max(
        Pos2::new(rect.min.x + layout.side_inset * zoom, rect.min.y + layout.content_top * zoom),
        Pos2::new(rect.max.x - layout.side_inset * zoom, rect.max.y - layout.bottom_pad * zoom),
    )
}

/// The title bar, which also moves the node.
pub fn title_bar_rect(rect: Rect, layout: &NodeLayout, zoom: f32) -> Rect {
    Rect::from_min_size(rect.min, Vec2::new(rect.width(), layout.title_h * zoom))
}
/// Within a run of same-typed ports, insert an extra blank slot every
/// `PORT_GROUP_SIZE` ports so long lists are easier to scan. A change of
/// port type adds no gap: the colours show it.
pub const PORT_GROUP_SIZE: usize = 8;

/// Compute a per-port Y-offset (unscaled pixels, relative to
/// `PORT_START_Y`) for a list of ports given their types. A gap is
/// inserted every `PORT_GROUP_SIZE` ports within a run of one `PortType`;
/// a change of type adds none. The returned vec
/// has `types.len() + 1` entries — the extra trailing value is the
/// cumulative height (used by `ports_height`).
fn compute_port_offsets(types: &[crate::engine::types::PortType]) -> Vec<f32> {
    let mut out = Vec::with_capacity(types.len() + 1);
    let mut y = 0.0f32;
    let mut prev: Option<crate::engine::types::PortType> = None;
    let mut run_idx = 0usize;
    for &t in types {
        if let Some(pt) = prev {
            if pt != t {
                // The port colours already tell the types apart, so a new
                // type only starts a new run of groups, with no gap.
                run_idx = 0;
            } else if run_idx != 0 && run_idx % PORT_GROUP_SIZE == 0 {
                // Group break within a homogeneous run.
                y += PORT_SPACING;
            }
        }
        out.push(y);
        y += PORT_SPACING;
        run_idx += 1;
        prev = Some(t);
    }
    out.push(y); // cumulative height
    out
}

pub fn make_port_id(
    node_id: NodeId,
    dir: PortDir,
    index: usize,
) -> crate::engine::types::PortId {
    crate::engine::types::PortId {
        node: node_id,
        index,
        dir,
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::engine::types::PortType;

    fn rect() -> Rect {
        Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(120.0, 80.0))
    }

    #[test]
    fn the_standard_layout_is_the_one_nodes_have_always_had() {
        let l = NodeLayout::STANDARD;
        assert_eq!((l.title_h, l.content_top, l.port_start_y, l.bottom_pad), (24.0, 38.0, 38.0, 8.0));
        assert!(l.toggle && !l.bare && l.show_title);
        // First of two input ports: 38 px below the top; the second 22 px lower.
        let types = [PortType::Untyped, PortType::Untyped];
        let p0 = port_pos_in(rect(), &l, PortDir::Input, &types, 0, 1.0);
        let p1 = port_pos_in(rect(), &l, PortDir::Input, &types, 1, 1.0);
        assert_eq!((p0.x, p0.y, p1.y), (100.0, 88.0, 110.0));
        // Outputs sit on the right edge, and everything scales with zoom.
        let out = port_pos_in(rect(), &l, PortDir::Output, &types, 0, 2.0);
        assert_eq!((out.x, out.y), (220.0, 50.0 + 38.0 * 2.0));
    }

    #[test]
    fn a_compact_node_centres_a_lone_port_on_its_content() {
        let l = NodeLayout::compact();
        let one = [PortType::Logic];
        let p = port_pos_in(rect(), &l, PortDir::Output, &one, 0, 1.0);
        // Content runs from 14 px below the top to the bottom edge.
        assert_eq!((p.x, p.y), (220.0, 50.0 + 14.0 + (80.0 - 14.0) / 2.0));
        // Input and output of a button line up on the same height.
        let q = port_pos_in(rect(), &l, PortDir::Input, &one, 0, 1.0);
        assert_eq!(p.y, q.y);
    }

    #[test]
    fn ports_of_different_types_sit_next_to_each_other_and_long_runs_still_break_into_groups() {
        use crate::engine::types::PortType::{Logic, Untyped};
        let tight = compute_port_offsets(&[Logic, Untyped, Untyped]);
        assert_eq!(tight, vec![0.0, PORT_SPACING, 2.0 * PORT_SPACING, 3.0 * PORT_SPACING]);
        let long = compute_port_offsets(&[Untyped; 9]);
        assert_eq!(long[8], 9.0 * PORT_SPACING, "a blank slot after the eighth");
    }

    #[test]
    fn several_ports_on_a_compact_node_hang_from_just_under_the_header() {
        let l = NodeLayout::compact();
        let two = [PortType::Untyped, PortType::Untyped];
        let p0 = port_pos_in(rect(), &l, PortDir::Output, &two, 0, 1.0);
        let p1 = port_pos_in(rect(), &l, PortDir::Output, &two, 1, 1.0);
        assert_eq!(p0.y, 50.0 + 26.0);
        assert_eq!(p1.y - p0.y, PORT_SPACING);
    }

    #[test]
    fn compact_nodes_have_no_switch_no_frame_and_use_all_their_width() {
        let l = NodeLayout::compact();
        assert!(!l.toggle && l.bare);
        let content = content_rect(rect(), &l, 1.0);
        assert_eq!((content.min.x, content.max.x, content.max.y), (100.0, 220.0, 130.0));
        assert_eq!(content.min.y, 50.0 + 14.0);
        assert!(!NodeLayout::compact().without_title().show_title);
    }

    #[test]
    fn the_title_bar_height_follows_the_layout() {
        assert_eq!(title_bar_rect(rect(), &NodeLayout::STANDARD, 1.0).height(), 24.0);
        assert_eq!(title_bar_rect(rect(), &NodeLayout::compact(), 2.0).height(), 28.0);
    }
}
