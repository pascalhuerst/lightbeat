use crate::engine::types::*;

/// Merges 2 float inputs (Pan, Tilt) into a single Position output.
pub struct PositionMergeProcessNode {
    id: NodeId,
    pan: f32,
    tilt: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn merge_ports() -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![
            PortDef::new("pan", PortType::Untyped),
            PortDef::new("tilt", PortType::Untyped),
        ],
        vec![PortDef::new("position", PortType::Position)],
    )
}

impl PositionMergeProcessNode {
    pub fn new(id: NodeId) -> Self {
        let (inputs, outputs) = merge_ports();
        Self {
            id, pan: 0.0, tilt: 0.0,
            inputs,
            outputs,
        }
    }
}

impl ProcessNode for PositionMergeProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Pan Tilt to Position" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        match pi { 0 => self.pan = v, 1 => self.tilt = v, _ => {} }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi { 0 => self.pan, 1 => self.tilt, _ => 0.0 }
    }
    fn process(&mut self) {}
    fn read_output(&self, ch: usize) -> f32 {
        match ch { 0 => self.pan, 1 => self.tilt, _ => 0.0 }
    }
}

/// Splits a Position input into 2 float outputs (Pan, Tilt).
pub struct PositionSplitProcessNode {
    id: NodeId,
    pan: f32,
    tilt: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn split_ports() -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![PortDef::new("position", PortType::Position)],
        vec![
            PortDef::new("pan", PortType::Untyped),
            PortDef::new("tilt", PortType::Untyped),
        ],
    )
}

impl PositionSplitProcessNode {
    pub fn new(id: NodeId) -> Self {
        let (inputs, outputs) = split_ports();
        Self {
            id, pan: 0.0, tilt: 0.0,
            inputs,
            outputs,
        }
    }
}

impl ProcessNode for PositionSplitProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Position to Pan Tilt" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, ch: usize, v: f32) {
        match ch { 0 => self.pan = v, 1 => self.tilt = v, _ => {} }
    }
    fn read_input(&self, ch: usize) -> f32 {
        match ch { 0 => self.pan, 1 => self.tilt, _ => 0.0 }
    }
    fn process(&mut self) {}
    fn read_output(&self, pi: usize) -> f32 {
        match pi { 0 => self.pan, 1 => self.tilt, _ => 0.0 }
    }
}
