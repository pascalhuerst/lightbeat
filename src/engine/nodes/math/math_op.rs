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

    fn apply(&self, a: f32, b: f32) -> f32 {
        match self {
            MathOp::Add => a + b,
            MathOp::Sub => a - b,
            MathOp::Mul => a * b,
            MathOp::Div => if b.abs() > 1e-10 { a / b } else { 0.0 },
        }
    }
}

/// Display state for the widget to know connected types.
pub struct MathDisplay {
    pub connected_types: [Option<PortType>; 2],
    pub output_type: PortType,
}

pub struct MathProcessNode {
    id: NodeId,
    op: MathOp,
    a: f32,
    b: f32,
    out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
    connected_types: [Option<PortType>; 2],
}

impl MathProcessNode {
    pub fn new(id: NodeId, op: MathOp) -> Self {
        Self {
            id,
            op,
            a: 0.0,
            b: 0.0,
            out: 0.0,
            inputs: vec![
                PortDef::new("a", PortType::Any),
                PortDef::new("b", PortType::Any),
            ],
            outputs: vec![PortDef::new("out", PortType::Any)],
            connected_types: [None, None],
        }
    }

    fn resolve_output_type(&self) -> PortType {
        // Output adopts the type of the first connected input.
        self.connected_types[0]
            .or(self.connected_types[1])
            .unwrap_or(PortType::Any)
    }
}

impl ProcessNode for MathProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.op.label() }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, port_index: usize, value: f32) {
        match port_index {
            0 => self.a = value,
            1 => self.b = value,
            _ => {}
        }
    }

    fn read_input(&self, port_index: usize) -> f32 {
        match port_index { 0 => self.a, 1 => self.b, _ => 0.0 }
    }

    fn process(&mut self) {
        self.out = self.op.apply(self.a, self.b);
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index { 0 => self.out, _ => 0.0 }
    }

    fn on_connect(&mut self, input_port: usize, source_type: PortType) {
        if input_port < 2 {
            self.connected_types[input_port] = Some(source_type);
        }
    }

    fn on_disconnect(&mut self, input_port: usize) {
        if input_port < 2 {
            self.connected_types[input_port] = None;
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(MathDisplay {
            connected_types: self.connected_types,
            output_type: self.resolve_output_type(),
        }));
    }
}
