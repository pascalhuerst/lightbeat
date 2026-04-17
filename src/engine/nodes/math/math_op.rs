use crate::engine::types::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MathOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl MathOp {
    pub fn label(&self) -> &'static str {
        match self {
            MathOp::Add => "Add",
            MathOp::Sub => "Sub",
            MathOp::Mul => "Mul",
            MathOp::Div => "Div",
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            MathOp::Add => "+",
            MathOp::Sub => "−",
            MathOp::Mul => "×",
            MathOp::Div => "÷",
        }
    }
}

/// Display state for the widget.
#[allow(dead_code)]
pub struct MathDisplay {
    pub input_count: usize,
    pub connected_types: Vec<Option<PortType>>,
    pub output_type: PortType,
}

/// Variadic arithmetic node. Starts at 2 inputs; the widget grows the count
/// by connecting wires to the special "+" add port. Operator is folded
/// across all values (left-to-right for Sub/Div).
pub struct MathProcessNode {
    id: NodeId,
    op: MathOp,
    values: Vec<f32>,
    out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
    connected_types: Vec<Option<PortType>>,
}

fn port_name(i: usize) -> String {
    // a, b, c, ... z, then in27, in28, ...
    if i < 26 {
        ((b'a' + i as u8) as char).to_string()
    } else {
        format!("in{}", i + 1)
    }
}

impl MathProcessNode {
    pub fn new(id: NodeId, op: MathOp) -> Self {
        let mut node = Self {
            id, op,
            values: Vec::new(),
            out: 0.0,
            inputs: Vec::new(),
            outputs: vec![PortDef::new("out", PortType::Any)],
            connected_types: Vec::new(),
        };
        node.set_input_count(2);
        node
    }

    fn set_input_count(&mut self, n: usize) {
        let n = n.max(1);
        self.inputs = (0..n).map(|i| PortDef::new(port_name(i), PortType::Any)).collect();
        self.values.resize(n, 0.0);
        self.connected_types.resize(n, None);
    }

    fn resolve_output_type(&self) -> PortType {
        self.connected_types.iter().find_map(|t| *t).unwrap_or(PortType::Any)
    }
}

impl ProcessNode for MathProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.op.label() }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if let Some(slot) = self.values.get_mut(port_index) {
            *slot = value;
        }
    }

    fn read_input(&self, port_index: usize) -> f32 {
        self.values.get(port_index).copied().unwrap_or(0.0)
    }

    fn process(&mut self) {
        if self.values.is_empty() { self.out = 0.0; return; }
        self.out = match self.op {
            MathOp::Add => self.values.iter().sum(),
            MathOp::Mul => self.values.iter().product(),
            MathOp::Sub => self.values.iter().enumerate().fold(0.0, |acc, (i, v)| {
                if i == 0 { *v } else { acc - v }
            }),
            MathOp::Div => self.values.iter().enumerate().fold(0.0, |acc, (i, v)| {
                if i == 0 { *v }
                else if v.abs() > 1e-10 { acc / v }
                else { 0.0 }
            }),
        };
    }

    fn read_output(&self, port_index: usize) -> f32 {
        if port_index == 0 { self.out } else { 0.0 }
    }

    fn on_connect(&mut self, input_port: usize, source_type: PortType) {
        // Auto-grow if the widget connected to a port beyond our current set.
        if input_port >= self.inputs.len() {
            self.set_input_count(input_port + 1);
        }
        self.connected_types[input_port] = Some(source_type);
    }

    fn on_disconnect(&mut self, input_port: usize) {
        if let Some(slot) = self.connected_types.get_mut(input_port) {
            *slot = None;
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
        shared.display = Some(Box::new(MathDisplay {
            input_count: self.inputs.len(),
            connected_types: self.connected_types.clone(),
            output_type: self.resolve_output_type(),
        }));
    }
}
