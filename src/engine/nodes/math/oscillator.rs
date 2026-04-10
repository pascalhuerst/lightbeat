use crate::engine::types::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OscFunc {
    Sin,
    Cos,
}

impl OscFunc {
    pub fn label(&self) -> &'static str {
        match self {
            OscFunc::Sin => "Sin",
            OscFunc::Cos => "Cos",
        }
    }
}

/// Oscillator: out = amplitude * func(phase * 2π)
/// Phase input drives the oscillation position (0..1 = one full cycle).
/// Amplitude input scales the result.
pub struct OscillatorProcessNode {
    id: NodeId,
    func: OscFunc,
    phase_in: f32,
    amp_in: f32,
    out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl OscillatorProcessNode {
    pub fn new(id: NodeId, func: OscFunc) -> Self {
        Self {
            id,
            func,
            phase_in: 0.0,
            amp_in: 1.0,
            out: 0.0,
            inputs: vec![
                PortDef::new("phase", PortType::Phase),
                PortDef::new("amp", PortType::Untyped),
            ],
            outputs: vec![PortDef::new("out", PortType::Untyped)],
        }
    }
}

impl ProcessNode for OscillatorProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.func.label() }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, port_index: usize, value: f32) {
        match port_index {
            0 => self.phase_in = value,
            1 => self.amp_in = value,
            _ => {}
        }
    }

    fn read_input(&self, port_index: usize) -> f32 {
        match port_index { 0 => self.phase_in, 1 => self.amp_in, _ => 0.0 }
    }

    fn process(&mut self) {
        let angle = self.phase_in * std::f32::consts::TAU;
        let raw = match self.func {
            OscFunc::Sin => angle.sin(),
            OscFunc::Cos => angle.cos(),
        };
        self.out = self.amp_in * raw;
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index { 0 => self.out, _ => 0.0 }
    }
}
