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

/// Display state for the widget.
#[allow(dead_code)]
pub struct LogicDisplay {
    pub input_count: usize,
}

/// Variadic logic gate (AND/OR/XOR over N inputs); NOT stays unary.
pub struct LogicGateProcessNode {
    id: NodeId,
    op: LogicOp,
    values: Vec<f32>,
    out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

fn input_label(i: usize) -> String {
    if i < 26 {
        ((b'a' + i as u8) as char).to_string()
    } else {
        format!("in{}", i + 1)
    }
}

impl LogicGateProcessNode {
    pub fn new(id: NodeId, op: LogicOp) -> Self {
        let mut node = Self {
            id, op,
            values: Vec::new(),
            out: 0.0,
            inputs: Vec::new(),
            outputs: vec![PortDef::new("out", PortType::Logic)],
        };
        let init = if op == LogicOp::Not { 1 } else { 2 };
        node.set_input_count(init);
        node
    }

    fn set_input_count(&mut self, n: usize) {
        let n = n.max(1);
        if self.op == LogicOp::Not {
            self.inputs = vec![PortDef::new("in", PortType::Logic)];
            self.values = vec![0.0];
        } else {
            self.inputs = (0..n).map(|i| PortDef::new(input_label(i), PortType::Logic)).collect();
            self.values.resize(n, 0.0);
        }
    }
}

impl ProcessNode for LogicGateProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.op.label() }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if let Some(slot) = self.values.get_mut(pi) {
            *slot = v;
        }
    }
    fn read_input(&self, pi: usize) -> f32 {
        self.values.get(pi).copied().unwrap_or(0.0)
    }

    fn process(&mut self) {
        let result = match self.op {
            LogicOp::Not => self.values.first().copied().unwrap_or(0.0) < 0.5,
            LogicOp::And => self.values.iter().all(|v| *v >= 0.5),
            LogicOp::Or => self.values.iter().any(|v| *v >= 0.5),
            LogicOp::Xor => self.values.iter().filter(|v| **v >= 0.5).count() % 2 == 1,
        };
        self.out = if result { 1.0 } else { 0.0 };
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.out } else { 0.0 }
    }

    fn on_connect(&mut self, input_port: usize, _source_type: PortType) {
        if self.op == LogicOp::Not { return; }
        if input_port >= self.inputs.len() {
            self.set_input_count(input_port + 1);
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "input_count": self.inputs.len() }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("input_count").and_then(|v| v.as_u64()) {
            self.set_input_count(n as usize);
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(LogicDisplay {
            input_count: self.inputs.len(),
        }));
    }
}
