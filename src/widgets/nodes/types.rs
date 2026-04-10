use egui::Color32;

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

/// Extensible port type. Each variant carries its own color and
/// compatibility rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortType {
    /// Instantaneous event (beat, gate on/off, etc.)
    Trigger,
    /// Continuous value, 0.0–1.0 (like CV in modular synths)
    Value,
}

impl PortType {
    pub fn color(&self) -> Color32 {
        match self {
            PortType::Trigger => Color32::from_rgb(240, 200, 40),
            PortType::Value => Color32::from_rgb(80, 180, 240),
        }
    }

    /// Whether an output of this type can connect to an input of `other`.
    pub fn compatible_with(&self, other: &PortType) -> bool {
        self == other
    }
}

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

// ---------------------------------------------------------------------------
// Connections
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Connection {
    pub from: PortId,
    pub to: PortId,
}
