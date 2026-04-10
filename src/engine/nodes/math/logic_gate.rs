use crate::engine::types::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogicOp {
    And,
    Or,
    Xor,
    Not,
}

impl LogicOp {
    pub fn label(&self) -> &'static str {
        match self {
            LogicOp::And => "AND",
            LogicOp::Or => "OR",
            LogicOp::Xor => "XOR",
            LogicOp::Not => "NOT",
        }
    }

}

pub struct LogicGateProcessNode {
    id: NodeId,
    op: LogicOp,
    a: f32,
    b: f32,
    out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl LogicGateProcessNode {
    pub fn new(id: NodeId, op: LogicOp) -> Self {
        let inputs = if op == LogicOp::Not {
            vec![PortDef::new("in", PortType::Logic)]
        } else {
            vec![
                PortDef::new("a", PortType::Logic),
                PortDef::new("b", PortType::Logic),
            ]
        };
        Self {
            id, op,
            a: 0.0, b: 0.0, out: 0.0,
            inputs,
            outputs: vec![PortDef::new("out", PortType::Logic)],
        }
    }
}

impl ProcessNode for LogicGateProcessNode {
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

    fn process(&mut self) {
        let a_high = self.a >= 0.5;
        let b_high = self.b >= 0.5;
        let result = match self.op {
            LogicOp::And => a_high && b_high,
            LogicOp::Or => a_high || b_high,
            LogicOp::Xor => a_high ^ b_high,
            LogicOp::Not => !a_high,
        };
        self.out = if result { 1.0 } else { 0.0 };
    }

    fn read_output(&self, pi: usize) -> f32 {
        match pi { 0 => self.out, _ => 0.0 }
    }
}
