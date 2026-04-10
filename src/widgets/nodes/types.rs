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

/// Signal types, modeled after Bitwig's Grid.
/// Any signal can connect anywhere — types are hints for semantics and color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortType {
    /// Bistate signal (yellow). ≥0.5 = high, <0.5 = low.
    /// Inputs react to transitions (rising/falling edge).
    /// High = 1.0, low = 0.0 on outputs.
    Logic,
    /// Unipolar 0..1 signal (purple). Values wrap into range.
    /// Used for driving data lookup / sequencer position.
    Phase,
    /// Generic untyped signal (red). Any range, any purpose.
    Untyped,
}

impl PortType {
    pub fn color(&self) -> Color32 {
        match self {
            PortType::Logic => Color32::from_rgb(240, 200, 40),
            PortType::Phase => Color32::from_rgb(180, 100, 220),
            PortType::Untyped => Color32::from_rgb(220, 80, 80),
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
