use egui::{Pos2, Ui};

use super::types::{PortDef, PortDir, PortId, NodeId};

/// Trait implemented by every node. Provides metadata (title, ports)
/// and custom content rendering.
pub trait NodeWidget {
    fn node_id(&self) -> NodeId;
    fn title(&self) -> &str;
    fn inputs(&self) -> &[PortDef];
    fn outputs(&self) -> &[PortDef];

    /// Draw the custom content area inside the node body.
    /// The available width is constrained by the node frame.
    fn show_content(&mut self, ui: &mut Ui);

    /// Minimum width for this node (content may request more).
    fn min_width(&self) -> f32 {
        140.0
    }

    /// Minimum content height (beyond what ports require).
    fn min_content_height(&self) -> f32 {
        0.0
    }
}

/// Runtime state the graph editor keeps per node (position, etc.).
/// Separate from the widget so node implementations stay clean.
pub struct NodeState {
    pub id: NodeId,
    pub pos: Pos2,
}

impl NodeState {
    pub fn new(id: NodeId, pos: Pos2) -> Self {
        Self { id, pos }
    }
}

// ---------------------------------------------------------------------------
// Helpers for port positioning (used by the graph renderer)
// ---------------------------------------------------------------------------

pub const NODE_TITLE_HEIGHT: f32 = 24.0;
pub const PORT_RADIUS: f32 = 6.0;
pub const PORT_SPACING: f32 = 22.0;
pub const PORT_START_Y: f32 = NODE_TITLE_HEIGHT + 14.0;
pub const NODE_PADDING: f32 = 8.0;

/// Compute the screen position of a port circle's center.
pub fn port_pos(node_pos: Pos2, node_width: f32, dir: PortDir, index: usize) -> Pos2 {
    let y = node_pos.y + PORT_START_Y + index as f32 * PORT_SPACING;
    let x = match dir {
        PortDir::Input => node_pos.x,
        PortDir::Output => node_pos.x + node_width,
    };
    Pos2::new(x, y)
}

/// Height of the port section (inputs or outputs, whichever is taller).
pub fn ports_height(num_inputs: usize, num_outputs: usize) -> f32 {
    let max_ports = num_inputs.max(num_outputs).max(1);
    PORT_START_Y + max_ports as f32 * PORT_SPACING + NODE_PADDING
}

/// Build a PortId for a given node, direction, and index.
pub fn make_port_id(node_id: NodeId, dir: PortDir, index: usize) -> PortId {
    PortId {
        node: node_id,
        index,
        dir,
    }
}
