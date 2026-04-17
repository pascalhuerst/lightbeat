use crate::engine::types::*;

/// SR (Set/Reset) flip-flop. Level-sensitive (works for both pulse-style
/// and held-high inputs):
/// - S high, R low → Q goes high
/// - R high, S low → Q goes low
/// - Both low → hold previous Q
/// - Both high → reset wins (Q goes low)
pub struct FlipFlopProcessNode {
    id: NodeId,
    s_in: f32,
    r_in: f32,
    state: bool,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl FlipFlopProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            s_in: 0.0,
            r_in: 0.0,
            state: false,
            inputs: vec![
                PortDef::new("S", PortType::Logic),
                PortDef::new("R", PortType::Logic),
            ],
            outputs: vec![
                PortDef::new("Q", PortType::Logic),
                PortDef::new("!Q", PortType::Logic),
            ],
        }
    }
}

impl ProcessNode for FlipFlopProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Flip-Flop" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        match pi { 0 => self.s_in = v, 1 => self.r_in = v, _ => {} }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi { 0 => self.s_in, 1 => self.r_in, _ => 0.0 }
    }

    fn process(&mut self) {
        let s = self.s_in >= 0.5;
        let r = self.r_in >= 0.5;
        if r {
            self.state = false;
        } else if s {
            self.state = true;
        }
        // else: hold
    }

    fn read_output(&self, pi: usize) -> f32 {
        match pi {
            0 => if self.state { 1.0 } else { 0.0 },
            1 => if self.state { 0.0 } else { 1.0 },
            _ => 0.0,
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "state": self.state }))
    }
    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(b) = data.get("state").and_then(|v| v.as_bool()) {
            self.state = b;
        }
    }
}

/// JK flip-flop. Edge-triggered on the rising edge of `clk`:
/// - J=0, K=0 → hold
/// - J=1, K=0 → set (Q goes high)
/// - J=0, K=1 → reset (Q goes low)
/// - J=1, K=1 → toggle (Q ↔ !Q)
pub struct JkFlipFlopProcessNode {
    id: NodeId,
    j_in: f32,
    k_in: f32,
    clk_in: f32,
    prev_clk: f32,
    state: bool,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl JkFlipFlopProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            j_in: 0.0,
            k_in: 0.0,
            clk_in: 0.0,
            prev_clk: 0.0,
            state: false,
            inputs: vec![
                PortDef::new("J", PortType::Logic),
                PortDef::new("K", PortType::Logic),
                PortDef::new("clk", PortType::Logic),
            ],
            outputs: vec![
                PortDef::new("Q", PortType::Logic),
                PortDef::new("!Q", PortType::Logic),
            ],
        }
    }
}

impl ProcessNode for JkFlipFlopProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "JK Flip-Flop" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        match pi { 0 => self.j_in = v, 1 => self.k_in = v, 2 => self.clk_in = v, _ => {} }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi { 0 => self.j_in, 1 => self.k_in, 2 => self.clk_in, _ => 0.0 }
    }

    fn process(&mut self) {
        // Rising edge on clk evaluates J / K.
        if self.clk_in >= 0.5 && self.prev_clk < 0.5 {
            let j = self.j_in >= 0.5;
            let k = self.k_in >= 0.5;
            self.state = match (j, k) {
                (true, true) => !self.state, // toggle
                (true, false) => true,        // set
                (false, true) => false,       // reset
                (false, false) => self.state, // hold
            };
        }
        self.prev_clk = self.clk_in;
    }

    fn read_output(&self, pi: usize) -> f32 {
        match pi {
            0 => if self.state { 1.0 } else { 0.0 },
            1 => if self.state { 0.0 } else { 1.0 },
            _ => 0.0,
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "state": self.state }))
    }
    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(b) = data.get("state").and_then(|v| v.as_bool()) {
            self.state = b;
        }
    }
}
