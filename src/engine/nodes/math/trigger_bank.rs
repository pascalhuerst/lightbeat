//! Trigger Bank — N value-to-trigger channels in one node.
//!
//! Each input accepts a continuous 0..1 value; whenever the value rises
//! across the 0.5 threshold, the matching output emits a one-tick Logic
//! pulse. Mirror-image of the Toggle Bank — use it to feed triggers into
//! the graph from held values (fader group, palette select, etc.) with
//! no manual edge-detect wiring.

use crate::engine::types::*;

pub const MIN_CHANNELS: usize = 1;
pub const MAX_CHANNELS: usize = 16;
const THRESHOLD: f32 = 0.5;

pub struct TriggerBankDisplay {
    pub n: usize,
    /// True on the tick an output is pulsing — used by the widget's LED
    /// row to flash as triggers fire.
    pub pulses: Vec<bool>,
}

pub struct TriggerBankProcessNode {
    id: NodeId,
    n: usize,
    in_vals: Vec<f32>,
    prev_in: Vec<f32>,
    /// True on the tick the corresponding output is pulsing. Reset to
    /// `false` at the start of each `process()` before edge detection.
    out_pulses: Vec<bool>,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl TriggerBankProcessNode {
    pub fn new(id: NodeId) -> Self {
        let n = 4;
        let mut s = Self {
            id,
            n,
            in_vals: Vec::new(),
            prev_in: Vec::new(),
            out_pulses: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        s.rebuild_ports();
        s
    }

    fn rebuild_ports(&mut self) {
        self.inputs = (0..self.n)
            .map(|i| PortDef::new(format!("In {}", i + 1), PortType::Untyped))
            .collect();
        self.outputs = (0..self.n)
            .map(|i| PortDef::new(format!("T{}", i + 1), PortType::Logic))
            .collect();
        self.in_vals.resize(self.n, 0.0);
        self.prev_in.resize(self.n, 0.0);
        self.out_pulses.resize(self.n, false);
    }
}

impl ProcessNode for TriggerBankProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Trigger Bank" }
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
            // Rising-edge pulse: low → high across 0.5.
            self.out_pulses[i] = cur >= THRESHOLD && prev < THRESHOLD;
            self.prev_in[i] = cur;
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        if let Some(&p) = self.out_pulses.get(pi) {
            if p { 1.0 } else { 0.0 }
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
        Some(serde_json::json!({ "n": self.n }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("n").and_then(|v| v.as_u64()) {
            let n = (n as usize).clamp(MIN_CHANNELS, MAX_CHANNELS);
            if n != self.n {
                self.n = n;
                self.rebuild_ports();
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(TriggerBankDisplay {
            n: self.n,
            pulses: self.out_pulses.clone(),
        }));
    }
}
