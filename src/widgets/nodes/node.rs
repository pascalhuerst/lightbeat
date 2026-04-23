use std::any::Any;

use egui::{Pos2, Ui};

use crate::engine::types::{NodeId, ParamDef, ParamValue, PortDir, SharedState};

use super::types::UiPortDef;

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

    /// Nodes that belong to a "linked set" (currently just portals) return
    /// the key that identifies the set. When the user selects a node with a
    /// portal key, the graph renderer draws a matching outline around every
    /// other node sharing the same key, so you can see the "wireless" peers
    /// at a glance. Default: not part of any linked set.
    fn portal_key(&self) -> Option<String> { None }

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

    /// Param indices that are currently overridden by a wired input and
    /// should be hidden from the auto-inspector. Pattern: when an input
    /// port is wired, the wire's value supersedes the matching param;
    /// the param is omitted from the inspector to avoid showing a value
    /// the user can't actually change. Implementations typically read
    /// `shared_state().lock().inputs_connected[port_idx]` to compute this.
    /// Default: nothing is overridden.
    fn overridden_param_indices(&self) -> Vec<usize> { Vec::new() }

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
/// Every N ports, an extra blank slot is inserted so long port lists are
/// easier to scan. Keeps input/output indexing untouched — only the
/// rendered Y position is affected.
pub const PORT_GROUP_SIZE: usize = 8;

/// Extra vertical offset (in unscaled pixels) accumulated from group gaps
/// up to and including `index`. Each completed group of `PORT_GROUP_SIZE`
/// contributes one extra `PORT_SPACING` of blank space above the next port.
fn port_group_offset(index: usize) -> f32 {
    (index / PORT_GROUP_SIZE) as f32 * PORT_SPACING
}

/// Compute port position at zoom=1.
#[allow(dead_code)]
pub fn port_pos(node_pos: Pos2, node_width: f32, dir: PortDir, index: usize) -> Pos2 {
    port_pos_z(node_pos, node_width, dir, index, 1.0)
}

pub fn port_pos_z(node_pos: Pos2, node_width: f32, dir: PortDir, index: usize, zoom: f32) -> Pos2 {
    let base = PORT_START_Y + index as f32 * PORT_SPACING + port_group_offset(index);
    let y = node_pos.y + base * zoom;
    let x = match dir {
        PortDir::Input => node_pos.x,
        PortDir::Output => node_pos.x + node_width,
    };
    Pos2::new(x, y)
}

pub fn ports_height(num_inputs: usize, num_outputs: usize) -> f32 {
    let max_ports = num_inputs.max(num_outputs).max(1);
    // `port_group_offset(max_ports)` accounts for every gap that sits
    // *above* port index `max_ports` — equivalent to the gaps between
    // groups when there are `max_ports` ports in total.
    PORT_START_Y + max_ports as f32 * PORT_SPACING + port_group_offset(max_ports) + NODE_PADDING
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
