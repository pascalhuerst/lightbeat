use crate::engine::types::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompareOp {
    Gte,
    Lte,
    Eq,
    Neq,
}

impl CompareOp {
    pub fn label(&self) -> &'static str {
        match self {
            CompareOp::Gte => ">=",
            CompareOp::Lte => "<=",
            CompareOp::Eq => "==",
            CompareOp::Neq => "!=",
        }
    }

    fn apply(&self, a: f32, b: f32) -> f32 {
        let result = match self {
            CompareOp::Gte => a >= b,
            CompareOp::Lte => a <= b,
            CompareOp::Eq => (a - b).abs() < 1e-6,
            CompareOp::Neq => (a - b).abs() >= 1e-6,
        };
        if result { 1.0 } else { 0.0 }
    }
}

pub struct CompareProcessNode {
    id: NodeId,
    op: CompareOp,
    a: f32,
    b: f32,
    out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl CompareProcessNode {
    pub fn new(id: NodeId, op: CompareOp) -> Self {
        Self {
            id, op,
            a: 0.0, b: 0.0, out: 0.0,
            inputs: vec![
                PortDef::new("a", PortType::Untyped),
                PortDef::new("b", PortType::Untyped),
            ],
            outputs: vec![PortDef::new("out", PortType::Logic)],
        }
    }
}

impl ProcessNode for CompareProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.op.label() }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        match pi { 0 => self.a = v, 1 => self.b = v, _ => {} }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi { 0 => self.a, 1 => self.b, _ => 0.0 }
    }
    fn process(&mut self) { self.out = self.op.apply(self.a, self.b); }
    fn read_output(&self, pi: usize) -> f32 {
        match pi { 0 => self.out, _ => 0.0 }
    }
}
