use crate::engine::types::*;

/// Latch: emits the previous tick's input value. Adds exactly one tick of
/// delay on top of the engine's natural per-connection delay.
///
/// Use this in feedback paths to make the data flow explicit (and
/// independent of any future scheduling changes).
pub struct LatchProcessNode {
    id: NodeId,
    /// Buffered input from this tick's propagation phase.
    pending_in: f32,
    /// What read_output emits — captured from `pending_in` in `process()`.
    held: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl LatchProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            pending_in: 0.0,
            held: 0.0,
            inputs: vec![PortDef::new("in", PortType::Untyped)],
            outputs: vec![PortDef::new("out", PortType::Untyped)],
        }
    }
}

impl ProcessNode for LatchProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Latch" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.pending_in = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.pending_in } else { 0.0 }
    }

    fn process(&mut self) {
        // Shift previously-buffered input into the visible output.
        // Anything written this tick will be visible next tick.
        self.held = self.pending_in;
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.held } else { 0.0 }
    }
}
