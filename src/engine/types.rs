use std::any::Any;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// IDs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PortId {
    pub node: NodeId,
    pub index: usize,
    pub dir: PortDir,
}

// ---------------------------------------------------------------------------
// Ports
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortDir {
    Input,
    Output,
}

/// Signal types, modeled after Bitwig's Grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortType {
    /// Bistate signal (yellow). >=0.5 = high, <0.5 = low.
    Logic,
    /// Unipolar 0..1 signal (purple). Values wrap into range.
    Phase,
    /// Generic untyped signal (red). Any range, any purpose.
    Untyped,
    /// Accepts any signal type (grey). Monitoring ports.
    Any,
    /// RGB color (3 floats: r, g, b in 0..1). Cyan.
    Color,
    /// Pan/Tilt position (2 floats: pan, tilt in 0..1). Green-blue.
    Position,
    /// A palette: a set of 4 colors (12 floats: 4 × RGB). Warm white.
    Palette,
    /// An 8-stop gradient (40 floats: 8 × (r, g, b, a, position)). Stops
    /// with alpha < 0 are treated as unused. Turquoise.
    Gradient,
}

/// Number of stops carried by `PortType::Gradient`. Each stop is 5 floats
/// (r, g, b, a, position), giving `GRADIENT_STOP_COUNT * 5` channels.
pub const GRADIENT_STOP_COUNT: usize = 8;
/// Floats per gradient stop (r, g, b, a, position).
pub const GRADIENT_STOP_FLOATS: usize = 5;

impl PortType {
    pub fn default_range(&self) -> (f32, f32) {
        match self {
            PortType::Logic => (0.0, 1.0),
            PortType::Phase => (0.0, 1.0),
            PortType::Untyped => (-1.0, 1.0),
            PortType::Any => (-1.0, 1.0),
            PortType::Color => (0.0, 1.0),
            PortType::Position => (0.0, 1.0),
            PortType::Palette => (0.0, 1.0),
            PortType::Gradient => (0.0, 1.0),
        }
    }

    /// Number of float components this port type carries.
    pub fn channel_count(&self) -> usize {
        match self {
            PortType::Color => 3,      // R, G, B
            PortType::Position => 2,   // Pan, Tilt
            PortType::Palette => 12,   // 4 × RGB
            PortType::Gradient => GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS, // 8 × (r,g,b,a,pos)
            _ => 1,
        }
    }

    pub fn compatible_with(&self, other: &PortType) -> bool {
        if *self == PortType::Any || *other == PortType::Any {
            return true;
        }
        self == other
    }
}

/// Port definition — engine side (no color info).
#[derive(Debug, Clone)]
pub struct PortDef {
    pub name: String,
    pub port_type: PortType,
}

impl PortDef {
    pub fn new(name: impl Into<String>, port_type: PortType) -> Self {
        Self {
            name: name.into(),
            port_type,
        }
    }
}

/// Compute the internal channel base index for a given logical port index.
/// Accounts for multi-channel ports (Color=3, Position=2).
pub fn port_base_index(ports: &[PortDef], logical_index: usize) -> usize {
    let mut base = 0;
    for (i, p) in ports.iter().enumerate() {
        if i == logical_index {
            return base;
        }
        base += p.port_type.channel_count();
    }
    base
}

/// Total number of internal float channels for a list of ports.
pub fn total_channels(ports: &[PortDef]) -> usize {
    ports.iter().map(|p| p.port_type.channel_count()).sum()
}

// ---------------------------------------------------------------------------
// Connections
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Connection {
    pub from: PortId,
    pub to: PortId,
}

// ---------------------------------------------------------------------------
// Parameters
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

/// A value written back to a node parameter.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ParamValue {
    Float(f32),
    Int(i64),
    Bool(bool),
    Choice(usize),
}

impl ParamValue {
    /// Coerce to f32 regardless of variant.
    pub fn as_f32(&self) -> f32 {
        match self {
            ParamValue::Float(v) => *v,
            ParamValue::Int(v) => *v as f32,
            ParamValue::Bool(v) => if *v { 1.0 } else { 0.0 },
            ParamValue::Choice(v) => *v as f32,
        }
    }

    /// Coerce to i64 regardless of variant.
    pub fn as_i64(&self) -> i64 {
        match self {
            ParamValue::Float(v) => *v as i64,
            ParamValue::Int(v) => *v,
            ParamValue::Bool(v) => if *v { 1 } else { 0 },
            ParamValue::Choice(v) => *v as i64,
        }
    }

    /// Coerce to usize regardless of variant.
    pub fn as_usize(&self) -> usize {
        match self {
            ParamValue::Float(v) => *v as usize,
            ParamValue::Int(v) => *v as usize,
            ParamValue::Bool(v) => if *v { 1 } else { 0 },
            ParamValue::Choice(v) => *v,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared state between engine and UI per node
// ---------------------------------------------------------------------------

/// State shared between an engine ProcessNode and its UI NodeWidget.
/// The engine writes output/input values and display state.
/// The UI reads them and pushes parameter changes.
pub struct NodeSharedState {
    /// Current output port values (written by engine, read by UI).
    pub outputs: Vec<f32>,
    /// Current input port values (written by engine, read by UI).
    pub inputs: Vec<f32>,
    /// Per-logical-input-port connectedness (written by engine, read by UI).
    /// Indexed by `inputs()` vec index — true iff at least one wire ends at
    /// that port on this node. Used by the auto-inspector to hide params
    /// that are overridden by a wired input, and by widgets that need to
    /// render differently when a port is wired.
    pub inputs_connected: Vec<bool>,
    /// When true, the engine skips this node's `process` each tick. Output
    /// values freeze at whatever they were last computed — downstream
    /// nodes therefore see a held signal rather than a sudden zero drop.
    /// Toggled from the node title bar; persisted with the project.
    pub disabled: bool,
    /// Current parameter definitions with values (written by engine, read by UI).
    pub current_params: Vec<ParamDef>,
    /// Custom display state (e.g. scope buffers, step sequencer values).
    pub display: Option<Box<dyn Any + Send>>,
    /// Parameter changes pushed by UI, consumed by engine each tick.
    pub pending_params: Vec<(usize, ParamValue)>,
    /// Serializable custom data for save/load (written by engine, read by UI for saving).
    pub save_data: Option<serde_json::Value>,
    /// Config pushed by widget for the engine to apply via load_data.
    /// Separate from save_data to avoid the engine overwriting its own config.
    pub pending_config: Option<serde_json::Value>,
}

impl NodeSharedState {
    pub fn new(num_inputs: usize, num_outputs: usize) -> Self {
        Self {
            outputs: vec![0.0; num_outputs],
            inputs: vec![0.0; num_inputs],
            inputs_connected: Vec::new(),
            disabled: false,
            current_params: Vec::new(),
            display: None,
            pending_params: Vec::new(),
            save_data: None,
            pending_config: None,
        }
    }
}

pub type SharedState = Arc<Mutex<NodeSharedState>>;

pub fn new_shared_state(num_inputs: usize, num_outputs: usize) -> SharedState {
    Arc::new(Mutex::new(NodeSharedState::new(num_inputs, num_outputs)))
}

// ---------------------------------------------------------------------------
// Engine commands (sent from UI to engine via ring buffer)
// ---------------------------------------------------------------------------

/// Commands sent from the UI thread to the engine thread.
#[allow(dead_code)]
pub enum EngineCommand {
    AddNode {
        node: Box<dyn ProcessNode>,
        shared: SharedState,
    },
    RemoveNode(NodeId),
    AddConnection(Connection),
    RemoveConnectionTo(PortId),
    SetParam {
        node_id: NodeId,
        index: usize,
        value: ParamValue,
    },
    /// Notify engine of a connection for on_connect/on_disconnect callbacks.
    NotifyConnect {
        node_id: NodeId,
        input_port: usize,
        source_type: PortType,
    },
    NotifyDisconnect {
        node_id: NodeId,
        input_port: usize,
    },
    LoadData {
        node_id: NodeId,
        data: serde_json::Value,
    },
    RemoveAllNodes,
    /// Set DMX output interfaces (id, output).
    SetInterfaces(Vec<(u32, Box<dyn crate::interfaces::DmxOutput>)>),
    /// Command targeting a subgraph's inner graph.
    SubgraphInnerCommand {
        /// Path from root to the target subgraph (supports nested subgraphs).
        subgraph_path: Vec<NodeId>,
        command: Box<SubgraphInnerCmd>,
    },
}

/// Commands for a subgraph's inner graph (routed via SubgraphInnerCommand).
pub enum SubgraphInnerCmd {
    AddNode {
        node: Box<dyn ProcessNode>,
        shared: SharedState,
    },
    RemoveNode(NodeId),
    AddConnection(Connection),
    RemoveConnectionTo(PortId),
    LoadData {
        node_id: NodeId,
        data: serde_json::Value,
    },
    NotifyConnect {
        node_id: NodeId,
        input_port: usize,
        source_type: PortType,
    },
    NotifyDisconnect {
        node_id: NodeId,
        input_port: usize,
    },
}

// ---------------------------------------------------------------------------
// ProcessNode trait
// ---------------------------------------------------------------------------

/// Trait for the engine-side processing of a node. Runs on the engine thread.
#[allow(dead_code)]
pub trait ProcessNode: Send {
    fn node_id(&self) -> NodeId;
    fn type_name(&self) -> &'static str;
    fn inputs(&self) -> &[PortDef];
    fn outputs(&self) -> &[PortDef];

    /// Called once per engine tick (~1kHz).
    fn process(&mut self);

    fn read_output(&self, _port_index: usize) -> f32 { 0.0 }
    fn read_input(&self, _port_index: usize) -> f32 { 0.0 }
    fn write_input(&mut self, _port_index: usize, _value: f32) {}

    /// Called by the engine at the start of each tick, before `process()`.
    /// `connected` is indexed by `inputs()` logical port index — `true`
    /// when at least one wire ends at that port on this node. Nodes that
    /// use the "input-overrides-param" pattern store this to decide
    /// whether to use the wired value or the param each tick.
    fn set_input_connections(&mut self, _connected: &[bool]) {}

    fn on_connect(&mut self, _input_port: usize, _source_type: PortType) {}
    fn on_disconnect(&mut self, _input_port: usize) {}

    fn params(&self) -> Vec<ParamDef> { vec![] }
    fn set_param(&mut self, _index: usize, _value: ParamValue) {}

    fn save_data(&self) -> Option<serde_json::Value> { None }
    fn load_data(&mut self, _data: &serde_json::Value) {}

    fn update_display(&self, _shared: &mut NodeSharedState) {}

    fn as_any_mut(&mut self) -> &mut dyn Any { unimplemented!("as_any_mut not implemented") }
    /// Default returns a placeholder that downcasts to nothing — nodes that
    /// want to be introspected (e.g. for in-subgraph display) override this
    /// to return `self`.
    fn as_any(&self) -> &dyn Any {
        static EMPTY: () = ();
        &EMPTY
    }
}
