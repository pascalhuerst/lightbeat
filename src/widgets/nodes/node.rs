use std::any::Any;

use egui::{Pos2, Ui};

use super::types::{NodeId, PortDef, PortDir, PortId, PortType};

// ---------------------------------------------------------------------------
// Node parameters
// ---------------------------------------------------------------------------

/// Describes one editable parameter on a node.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ParamDef {
    Float {
        name: String,
        value: f32,
        min: f32,
        max: f32,
        step: f32,
        unit: &'static str,
    },
    Int {
        name: String,
        value: i64,
        min: i64,
        max: i64,
    },
    Bool {
        name: String,
        value: bool,
    },
    Choice {
        name: String,
        value: usize,
        options: Vec<String>,
    },
}

impl ParamDef {
    pub fn name(&self) -> &str {
        match self {
            ParamDef::Float { name, .. } => name,
            ParamDef::Int { name, .. } => name,
            ParamDef::Bool { name, .. } => name,
            ParamDef::Choice { name, .. } => name,
        }
    }
}

/// A value written back from the inspector to a node parameter.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ParamValue {
    Float(f32),
    Int(i64),
    Bool(bool),
    Choice(usize),
}

// ---------------------------------------------------------------------------
// NodeWidget trait
// ---------------------------------------------------------------------------

/// Trait implemented by every node. Provides metadata (title, ports)
/// and custom content rendering.
pub trait NodeWidget: Any {
    fn node_id(&self) -> NodeId;
    /// Serialization type tag — must match the label used in `register_node`.
    fn type_name(&self) -> &'static str;
    fn title(&self) -> &str;
    fn inputs(&self) -> &[PortDef];
    fn outputs(&self) -> &[PortDef];

    /// Draw the custom content area inside the node body.
    fn show_content(&mut self, ui: &mut Ui);

    fn min_width(&self) -> f32 {
        140.0
    }

    fn min_content_height(&self) -> f32 {
        0.0
    }

    /// Whether this node can be resized by dragging the bottom-right corner.
    fn resizable(&self) -> bool {
        false
    }

    fn read_output(&self, _port_index: usize) -> f32 {
        0.0
    }

    /// Read the last value written to an input port.
    fn read_input(&self, _port_index: usize) -> f32 {
        0.0
    }

    fn write_input(&mut self, _port_index: usize, _value: f32) {}

    fn process(&mut self) {}

    /// Called when a connection is made to an input port.
    fn on_connect(&mut self, _input_port: usize, _source_type: PortType) {}

    /// Called when a connection to an input port is removed.
    fn on_disconnect(&mut self, _input_port: usize) {}

    /// Return the node's editable parameters for the inspector.
    fn params(&self) -> Vec<ParamDef> {
        vec![]
    }

    /// Apply a parameter change from the inspector.
    fn set_param(&mut self, _index: usize, _value: ParamValue) {}

    /// Show extra info/visuals in the inspector (e.g. scope display).
    fn show_inspector(&mut self, _ui: &mut Ui) {}

    /// Save custom node data beyond params (e.g. step values).
    /// Returns a JSON value, or None if no extra data.
    fn save_data(&self) -> Option<serde_json::Value> {
        None
    }

    /// Load custom node data.
    fn load_data(&mut self, _data: &serde_json::Value) {}

    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Runtime state the graph editor keeps per node (position, etc.).
/// Separate from the widget so node implementations stay clean.
pub struct NodeState {
    pub id: NodeId,
    pub pos: Pos2,
    /// User-resized dimensions, if any. None = use minimum size.
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
