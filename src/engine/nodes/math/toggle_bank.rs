//! Toggle Bank — N independent toggle flip-flops in one node.
//!
//! Each input is a rising-edge trigger; every rising edge flips the
//! corresponding output between 0 and 1. Output state survives across
//! ticks (held), making this useful for building latch-style button
//! banks that mirror a physical surface (Launchpad / Push pads).

use crate::engine::types::*;

pub const MIN_CHANNELS: usize = 1;
pub const MAX_CHANNELS: usize = 16;

pub struct ToggleBankDisplay {
    pub n: usize,
    pub states: Vec<bool>,
}

pub struct ToggleBankProcessNode {
    id: NodeId,
    n: usize,
    in_vals: Vec<f32>,
    prev_in: Vec<f32>,
    states: Vec<bool>,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl ToggleBankProcessNode {
    pub fn new(id: NodeId) -> Self {
        let n = 4;
        let mut s = Self {
            id,
            n,
            in_vals: Vec::new(),
            prev_in: Vec::new(),
            states: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        s.rebuild_ports();
        s
    }

    fn rebuild_ports(&mut self) {
        // Triggers in (Logic — the rising edge is what matters), held
        // value out (Untyped — the output is a persistent 0 or 1, not a
        // one-tick pulse).
        self.inputs = (0..self.n)
            .map(|i| PortDef::new(format!("T{}", i + 1), PortType::Logic))
            .collect();
        self.outputs = (0..self.n)
            .map(|i| PortDef::new(format!("O{}", i + 1), PortType::Untyped))
            .collect();
        self.in_vals.resize(self.n, 0.0);
        self.prev_in.resize(self.n, 0.0);
        self.states.resize(self.n, false);
    }
}

impl ProcessNode for ToggleBankProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Toggle Bank" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if let Some(slot) = self.in_vals.get_mut(pi) { *slot = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        self.in_vals.get(pi).copied().unwrap_or(0.0)
    }

    fn process(&mut self) {
        for i in 0..self.n {
            let cur = self.in_vals[i];
            let prev = self.prev_in[i];
            // Rising edge: below threshold → above threshold.
            if cur >= 0.5 && prev < 0.5 {
                self.states[i] = !self.states[i];
            }
            self.prev_in[i] = cur;
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        if let Some(&b) = self.states.get(pi) {
            if b { 1.0 } else { 0.0 }
        } else {
            0.0
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Int {
            name: "Channels".into(),
            value: self.n as i64,
            min: MIN_CHANNELS as i64,
            max: MAX_CHANNELS as i64,
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 {
            let new_n = (value.as_i64() as usize).clamp(MIN_CHANNELS, MAX_CHANNELS);
            if new_n != self.n {
                self.n = new_n;
                self.rebuild_ports();
            }
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "n": self.n,
            "states": self.states,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("n").and_then(|v| v.as_u64()) {
            let n = (n as usize).clamp(MIN_CHANNELS, MAX_CHANNELS);
            if n != self.n {
                self.n = n;
                self.rebuild_ports();
            }
        }
        if let Some(arr) = data.get("states").and_then(|v| v.as_array()) {
            for (i, v) in arr.iter().enumerate() {
                if let (Some(b), Some(slot)) = (v.as_bool(), self.states.get_mut(i)) {
                    *slot = b;
                }
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(ToggleBankDisplay {
            n: self.n,
            states: self.states.clone(),
        }));
    }
}
