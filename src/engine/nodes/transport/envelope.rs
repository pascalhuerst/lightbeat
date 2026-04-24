use crate::engine::types::*;

/// Which kind of envelope behavior this node runs.
///
/// Numeric param index is `to_index()` / `from_index()` so loads remain
/// stable across added variants as long as new entries are appended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeKind {
    /// Attack / Decay / Sustain / Release — classic gated envelope.
    Adsr,
    /// Attack / Decay one-shot — rising edge of the gate fires a full A→D
    /// cycle, gate duration is ignored.
    Ad,
    /// Attack / hold-while-high / Release — no decay/sustain stage.
    Ar,
    /// Symmetric envelope follower — low-pass `|signal|` with one time
    /// constant for both directions (reuses the Attack slider).
    Follower,
    /// Asymmetric envelope follower — separate rise / fall time constants
    /// (reuses Attack as Rise, Release as Fall).
    FollowerRf,
    /// Slew limiter that slows the *rising* edge only. Falling edges pass
    /// through instantly. Uses Attack as the rise time.
    SlopeUp,
    /// Slew limiter that slows the *falling* edge only. Rising edges pass
    /// through instantly. Uses Release as the fall time.
    SlopeDown,
}

impl EnvelopeKind {
    pub const ALL: [Self; 7] = [
        Self::Adsr, Self::Ad, Self::Ar,
        Self::Follower, Self::FollowerRf,
        Self::SlopeUp, Self::SlopeDown,
    ];
    pub fn to_index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }
    pub fn from_index(i: usize) -> Self {
        Self::ALL.get(i).copied().unwrap_or(Self::Adsr)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Adsr => "ADSR",
            Self::Ad => "AD",
            Self::Ar => "AR",
            Self::Follower => "Follower",
            Self::FollowerRf => "Follower-RF",
            Self::SlopeUp => "Slope \u{2197}",
            Self::SlopeDown => "Slope \u{2198}",
        }
    }
    /// Whether the first input port is interpreted as a gate/trigger this
    /// mode. Follower/Slope modes only look at `signal`.
    pub fn uses_gate(self) -> bool {
        matches!(self, Self::Adsr | Self::Ad | Self::Ar)
    }
}

/// ADSR stages — meaningful only for gated modes (ADSR, AD, AR). Other
/// modes leave it at `Idle`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum EnvStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// Snapshot of engine state for the widget.
pub struct EnvelopeDisplay {
    pub kind: EnvelopeKind,
    pub stage: u8, // 0=idle, 1=attack, 2=decay, 3=sustain, 4=release (AD/AR/ADSR only)
    pub envelope_value: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

pub struct EnvelopeProcessNode {
    id: NodeId,
    kind: EnvelopeKind,
    // Parameters — interpretation depends on `kind`.
    attack: f32,   // Attack / Rise / Tau
    decay: f32,    // Decay (ADSR, AD)
    sustain: f32,  // Sustain level 0..1 (ADSR)
    release: f32,  // Release / Fall

    // Triggered-mode state.
    stage: EnvStage,
    envelope: f32,
    prev_gate: bool,

    // Inputs.
    gate_in: f32,
    signal_in: f32,

    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl EnvelopeProcessNode {
    pub fn new(id: NodeId) -> Self {
        // Defaults tuned for beat-triggered use at ~110..140 BPM (beat
        // period 0.43..0.55 s, half-beat 0.21..0.27 s).
        Self {
            id,
            kind: EnvelopeKind::Adsr,
            attack: 0.02,
            decay: 0.08,
            sustain: 0.6,
            release: 0.15,
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

    fn reset_triggered_state(&mut self) {
        self.stage = EnvStage::Idle;
        self.envelope = 0.0;
        self.prev_gate = false;
    }
}

// Engine tick rate ~1kHz, so dt ~0.001s
const DT: f32 = 0.001;

/// One-pole time-constant coefficient for a smoothing filter: values closer
/// to 1.0 mean slower response. `tau` is the 1/e time in seconds.
fn tau_coeff(tau: f32) -> f32 {
    if tau <= 0.0 { 0.0 } else { (-DT / tau).exp() }
}

impl ProcessNode for EnvelopeProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Envelope" }
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
        match self.kind {
            EnvelopeKind::Adsr => self.process_adsr(),
            EnvelopeKind::Ad => self.process_ad(),
            EnvelopeKind::Ar => self.process_ar(),
            EnvelopeKind::Follower => self.process_follower_sym(),
            EnvelopeKind::FollowerRf => self.process_follower_rf(),
            EnvelopeKind::SlopeUp => self.process_slope_up(),
            EnvelopeKind::SlopeDown => self.process_slope_down(),
        }
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.envelope,
            // For gated modes the second output is the input modulated by
            // the envelope. For follower/slope modes the envelope *is* the
            // processed signal, so emit the envelope itself on both ports —
            // `signal` then mirrors what most users want downstream.
            1 => match self.kind {
                EnvelopeKind::Follower
                | EnvelopeKind::FollowerRf
                | EnvelopeKind::SlopeUp
                | EnvelopeKind::SlopeDown => self.envelope,
                _ => self.signal_in * self.envelope,
            },
            _ => 0.0,
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        // Labels track the active mode so the inspector reads correctly;
        // the widget hides the slots that don't apply. Indices are stable
        // across modes so load/save stays clean.
        let (attack_name, release_name) = match self.kind {
            EnvelopeKind::Adsr | EnvelopeKind::Ad | EnvelopeKind::Ar => ("Attack", "Release"),
            EnvelopeKind::Follower => ("Tau", "Release"),
            EnvelopeKind::FollowerRf => ("Rise", "Fall"),
            EnvelopeKind::SlopeUp => ("Rise", "Release"),
            EnvelopeKind::SlopeDown => ("Attack", "Fall"),
        };
        vec![
            ParamDef::Float { name: attack_name.into(), value: self.attack, min: 0.001, max: 1.0, step: 0.005, unit: "s" },
            ParamDef::Float { name: "Decay".into(), value: self.decay, min: 0.001, max: 1.0, step: 0.005, unit: "s" },
            ParamDef::Float { name: "Sustain".into(), value: self.sustain, min: 0.0, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Float { name: release_name.into(), value: self.release, min: 0.001, max: 2.0, step: 0.005, unit: "s" },
            ParamDef::Choice {
                name: "Mode".into(),
                value: self.kind.to_index(),
                options: EnvelopeKind::ALL.iter().map(|k| k.label().to_string()).collect(),
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.attack = value.as_f32(),
            1 => self.decay = value.as_f32(),
            2 => self.sustain = value.as_f32(),
            3 => self.release = value.as_f32(),
            4 => {
                let new_kind = EnvelopeKind::from_index(value.as_usize());
                if new_kind != self.kind {
                    self.kind = new_kind;
                    self.reset_triggered_state();
                }
            }
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
            kind: self.kind,
            stage,
            envelope_value: self.envelope,
            attack: self.attack,
            decay: self.decay,
            sustain: self.sustain,
            release: self.release,
        }));
    }
}

// ---------------------------------------------------------------------------
// Per-mode processing
// ---------------------------------------------------------------------------

impl EnvelopeProcessNode {
    fn process_adsr(&mut self) {
        let gate_high = self.gate_in >= 0.5;
        if gate_high && !self.prev_gate { self.stage = EnvStage::Attack; }
        if !gate_high && self.prev_gate { self.stage = EnvStage::Release; }
        self.prev_gate = gate_high;

        match self.stage {
            EnvStage::Idle => { self.envelope = 0.0; }
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
            EnvStage::Sustain => { self.envelope = self.sustain; }
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

    fn process_ad(&mut self) {
        // One-shot: rising edge arms Attack; we ignore the falling edge and
        // let Decay run to zero before idling again. Re-triggering during
        // Decay restarts from Attack.
        let gate_high = self.gate_in >= 0.5;
        if gate_high && !self.prev_gate {
            self.stage = EnvStage::Attack;
        }
        self.prev_gate = gate_high;

        match self.stage {
            EnvStage::Idle => { self.envelope = 0.0; }
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
                self.envelope -= rate;
                if self.envelope <= 0.0 {
                    self.envelope = 0.0;
                    self.stage = EnvStage::Idle;
                }
            }
            // AD collapses Sustain/Release into Decay — those branches
            // shouldn't occur but fall through defensively.
            EnvStage::Sustain | EnvStage::Release => { self.stage = EnvStage::Decay; }
        }
    }

    fn process_ar(&mut self) {
        let gate_high = self.gate_in >= 0.5;
        if gate_high && !self.prev_gate { self.stage = EnvStage::Attack; }
        if !gate_high && self.prev_gate { self.stage = EnvStage::Release; }
        self.prev_gate = gate_high;

        match self.stage {
            EnvStage::Idle => { self.envelope = 0.0; }
            EnvStage::Attack => {
                let rate = if self.attack > 0.0 { DT / self.attack } else { 1.0 };
                self.envelope += rate;
                if self.envelope >= 1.0 {
                    self.envelope = 1.0;
                    // `Sustain` is reused as the "hold while gate high"
                    // stage — stage=3 lights the right indicator in the UI.
                    self.stage = EnvStage::Sustain;
                }
            }
            EnvStage::Sustain => { self.envelope = 1.0; }
            EnvStage::Release => {
                let rate = if self.release > 0.0 { DT / self.release } else { 1.0 };
                self.envelope -= rate;
                if self.envelope <= 0.0 {
                    self.envelope = 0.0;
                    self.stage = EnvStage::Idle;
                }
            }
            EnvStage::Decay => { self.stage = EnvStage::Attack; }
        }
    }

    fn process_follower_sym(&mut self) {
        // One-pole tracker on |signal| with a single time constant
        // (`attack` slider, labeled "Tau" in this mode).
        let target = self.signal_in.abs();
        let a = tau_coeff(self.attack);
        self.envelope = a * self.envelope + (1.0 - a) * target;
        self.stage = EnvStage::Idle;
    }

    fn process_follower_rf(&mut self) {
        // Asymmetric: Rise tracks rising moves, Fall tracks falling moves.
        let target = self.signal_in.abs();
        let coeff = if target > self.envelope {
            tau_coeff(self.attack)
        } else {
            tau_coeff(self.release)
        };
        self.envelope = coeff * self.envelope + (1.0 - coeff) * target;
        self.stage = EnvStage::Idle;
    }

    fn process_slope_up(&mut self) {
        // Slew the rising edge; fall instantly.
        let target = self.signal_in;
        if target > self.envelope {
            let a = tau_coeff(self.attack);
            self.envelope = a * self.envelope + (1.0 - a) * target;
        } else {
            self.envelope = target;
        }
        self.stage = EnvStage::Idle;
    }

    fn process_slope_down(&mut self) {
        // Rise instantly; slew the falling edge.
        let target = self.signal_in;
        if target < self.envelope {
            let a = tau_coeff(self.release);
            self.envelope = a * self.envelope + (1.0 - a) * target;
        } else {
            self.envelope = target;
        }
        self.stage = EnvStage::Idle;
    }
}
