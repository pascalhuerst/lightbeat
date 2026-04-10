use crate::engine::types::*;

/// ADSR envelope states.
#[derive(Debug, Clone, Copy, PartialEq)]
enum EnvStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// Display state for the UI.
pub struct EnvelopeDisplay {
    pub stage: u8, // 0=idle, 1=attack, 2=decay, 3=sustain, 4=release
    pub envelope_value: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

pub struct EnvelopeProcessNode {
    id: NodeId,
    // Parameters (in seconds / level)
    attack: f32,
    decay: f32,
    sustain: f32, // level 0..1
    release: f32,
    // State
    stage: EnvStage,
    envelope: f32,
    prev_gate: bool,
    gate_in: f32,
    signal_in: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl EnvelopeProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            attack: 0.01,
            decay: 0.1,
            sustain: 0.7,
            release: 0.3,
            stage: EnvStage::Idle,
            envelope: 0.0,
            prev_gate: false,
            gate_in: 0.0,
            signal_in: 0.0,
            inputs: vec![
                PortDef::new("gate", PortType::Logic),
                PortDef::new("signal", PortType::Untyped),
            ],
            outputs: vec![
                PortDef::new("envelope", PortType::Untyped),
                PortDef::new("signal", PortType::Untyped),
            ],
        }
    }
}

// Engine tick rate ~1kHz, so dt ~0.001s
const DT: f32 = 0.001;

impl ProcessNode for EnvelopeProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "ADSR" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, port_index: usize, value: f32) {
        match port_index {
            0 => self.gate_in = value,
            1 => self.signal_in = value,
            _ => {}
        }
    }

    fn read_input(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.gate_in,
            1 => self.signal_in,
            _ => 0.0,
        }
    }

    fn process(&mut self) {
        let gate_high = self.gate_in >= 0.5;

        // Detect rising edge -> start attack.
        if gate_high && !self.prev_gate {
            self.stage = EnvStage::Attack;
        }
        // Detect falling edge -> start release.
        if !gate_high && self.prev_gate {
            self.stage = EnvStage::Release;
        }
        self.prev_gate = gate_high;

        match self.stage {
            EnvStage::Idle => {
                self.envelope = 0.0;
            }
            EnvStage::Attack => {
                let rate = if self.attack > 0.0 { DT / self.attack } else { 1.0 };
                self.envelope += rate;
                if self.envelope >= 1.0 {
                    self.envelope = 1.0;
                    self.stage = EnvStage::Decay;
                }
            }
            EnvStage::Decay => {
                let rate = if self.decay > 0.0 { DT / self.decay } else { 1.0 };
                self.envelope -= rate * (1.0 - self.sustain);
                if self.envelope <= self.sustain {
                    self.envelope = self.sustain;
                    self.stage = EnvStage::Sustain;
                }
            }
            EnvStage::Sustain => {
                self.envelope = self.sustain;
            }
            EnvStage::Release => {
                let rate = if self.release > 0.0 { DT / self.release } else { 1.0 };
                self.envelope -= rate * self.envelope.max(0.01);
                if self.envelope <= 0.001 {
                    self.envelope = 0.0;
                    self.stage = EnvStage::Idle;
                }
            }
        }
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.envelope,
            1 => self.signal_in * self.envelope,
            _ => 0.0,
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float { name: "Attack".into(), value: self.attack, min: 0.001, max: 5.0, step: 0.01, unit: "s" },
            ParamDef::Float { name: "Decay".into(), value: self.decay, min: 0.001, max: 5.0, step: 0.01, unit: "s" },
            ParamDef::Float { name: "Sustain".into(), value: self.sustain, min: 0.0, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Float { name: "Release".into(), value: self.release, min: 0.001, max: 10.0, step: 0.01, unit: "s" },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match (index, value) {
            (0, ParamValue::Float(v)) => self.attack = v,
            (1, ParamValue::Float(v)) => self.decay = v,
            (2, ParamValue::Float(v)) => self.sustain = v,
            (3, ParamValue::Float(v)) => self.release = v,
            _ => {}
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        let stage = match self.stage {
            EnvStage::Idle => 0,
            EnvStage::Attack => 1,
            EnvStage::Decay => 2,
            EnvStage::Sustain => 3,
            EnvStage::Release => 4,
        };
        shared.display = Some(Box::new(EnvelopeDisplay {
            stage,
            envelope_value: self.envelope,
            attack: self.attack,
            decay: self.decay,
            sustain: self.sustain,
            release: self.release,
        }));
    }
}
