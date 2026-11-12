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
    pub attack_curve: f32,
    pub decay_curve: f32,
    pub release_curve: f32,
}

pub struct EnvelopeProcessNode {
    id: NodeId,
    kind: EnvelopeKind,
    // Parameters — interpretation depends on `kind`.
    attack: f32,   // Attack / Rise / Tau
    decay: f32,    // Decay (ADSR, AD)
    sustain: f32,  // Sustain level 0..1 (ADSR)
    release: f32,  // Release / Fall
    /// Bend of each segment, -1..1 (0 = straight); see `shape`.
    attack_curve: f32,
    decay_curve: f32,
    release_curve: f32,
    /// Where the running segment started and how far along it is (0..1).
    seg_from: f32,
    seg_pos: f32,

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
            attack_curve: 0.0,
            decay_curve: 0.0,
            release_curve: 0.0,
            seg_from: 0.0,
            seg_pos: 0.0,
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

    fn enter(&mut self, stage: EnvStage) {
        self.stage = stage;
        self.seg_from = self.envelope;
        self.seg_pos = 0.0;
    }

    /// Moves the running segment on by one tick towards `to`, bent by `curve`.
    /// True when it has arrived.
    fn advance(&mut self, secs: f32, curve: f32, to: f32) -> bool {
        self.seg_pos += if secs > 0.0 { DT / secs } else { 1.0 };
        if self.seg_pos >= 1.0 {
            self.envelope = to;
            return true;
        }
        self.envelope = self.seg_from + (to - self.seg_from) * shape(self.seg_pos, curve);
        false
    }

    fn reset_triggered_state(&mut self) {
        self.seg_from = 0.0;
        self.seg_pos = 0.0;
        self.stage = EnvStage::Idle;
        self.envelope = 0.0;
        self.prev_gate = false;
    }
}

// Engine tick rate ~1kHz, so dt ~0.001s
const DT: f32 = 0.001;

/// How far along a segment's height the point at time fraction `t` is.
/// `curve` 0 is a straight line, above 0 starts slowly and ends fast, below 0
/// the opposite. The widget draws the same function, so the picture is the
/// sound.
pub fn shape(t: f32, curve: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let k = curve.clamp(-1.0, 1.0) * CURVE_SCALE;
    if k.abs() < 1e-3 { t } else { ((k * t).exp() - 1.0) / (k.exp() - 1.0) }
}

/// The curve value that makes `shape(0.5, c)` equal `mid` (0..1): what the
/// widget needs when the user drags the middle of a line to a height.
pub fn curve_for_mid(mid: f32) -> f32 {
    let mid = mid.clamp(0.02, 0.98);
    (2.0 * (1.0 / mid - 1.0).ln() / CURVE_SCALE).clamp(-1.0, 1.0)
}

const CURVE_SCALE: f32 = 6.0;

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
            ParamDef::Float { name: "Attack curve".into(), value: self.attack_curve, min: -1.0, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Float { name: "Decay curve".into(), value: self.decay_curve, min: -1.0, max: 1.0, step: 0.01, unit: "" },
            ParamDef::Float { name: "Release curve".into(), value: self.release_curve, min: -1.0, max: 1.0, step: 0.01, unit: "" },
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
            5 => self.attack_curve = value.as_f32().clamp(-1.0, 1.0),
            6 => self.decay_curve = value.as_f32().clamp(-1.0, 1.0),
            7 => self.release_curve = value.as_f32().clamp(-1.0, 1.0),
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
            attack_curve: self.attack_curve,
            decay_curve: self.decay_curve,
            release_curve: self.release_curve,
        }));
    }
}

// ---------------------------------------------------------------------------
// Per-mode processing
// ---------------------------------------------------------------------------

impl EnvelopeProcessNode {
    fn process_adsr(&mut self) {
        let gate_high = self.gate_in >= 0.5;
        if gate_high && !self.prev_gate { self.enter(EnvStage::Attack); }
        if !gate_high && self.prev_gate { self.enter(EnvStage::Release); }
        self.prev_gate = gate_high;

        match self.stage {
            EnvStage::Idle => { self.envelope = 0.0; }
            EnvStage::Attack => {
                if self.advance(self.attack, self.attack_curve, 1.0) {
                    self.enter(EnvStage::Decay);
                }
            }
            EnvStage::Decay => {
                if self.advance(self.decay, self.decay_curve, self.sustain) {
                    self.stage = EnvStage::Sustain;
                }
            }
            EnvStage::Sustain => { self.envelope = self.sustain; }
            EnvStage::Release => {
                if self.advance(self.release, self.release_curve, 0.0) {
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
            self.enter(EnvStage::Attack);
        }
        self.prev_gate = gate_high;

        match self.stage {
            EnvStage::Idle => { self.envelope = 0.0; }
            EnvStage::Attack => {
                if self.advance(self.attack, self.attack_curve, 1.0) {
                    self.enter(EnvStage::Decay);
                }
            }
            EnvStage::Decay => {
                if self.advance(self.decay, self.decay_curve, 0.0) {
                    self.stage = EnvStage::Idle;
                }
            }
            // AD collapses Sustain/Release into Decay — those branches
            // shouldn't occur but fall through defensively.
            EnvStage::Sustain | EnvStage::Release => { self.enter(EnvStage::Decay); }
        }
    }

    fn process_ar(&mut self) {
        let gate_high = self.gate_in >= 0.5;
        if gate_high && !self.prev_gate { self.enter(EnvStage::Attack); }
        if !gate_high && self.prev_gate { self.enter(EnvStage::Release); }
        self.prev_gate = gate_high;

        match self.stage {
            EnvStage::Idle => { self.envelope = 0.0; }
            EnvStage::Attack => {
                if self.advance(self.attack, self.attack_curve, 1.0) {
                    // `Sustain` is reused as the "hold while gate high"
                    // stage — stage=3 lights the right indicator in the UI.
                    self.stage = EnvStage::Sustain;
                }
            }
            EnvStage::Sustain => { self.envelope = 1.0; }
            EnvStage::Release => {
                if self.advance(self.release, self.release_curve, 0.0) {
                    self.stage = EnvStage::Idle;
                }
            }
            EnvStage::Decay => { self.enter(EnvStage::Attack); }
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

#[cfg(test)]
mod curve_tests {
    use super::*;

    #[test]
    fn shape_is_linear_at_zero_and_hits_the_ends() {
        assert!((shape(0.3, 0.0) - 0.3).abs() < 1e-6);
        for c in [-1.0, -0.4, 0.7, 1.0] {
            assert!(shape(0.0, c).abs() < 1e-6);
            assert!((shape(1.0, c) - 1.0).abs() < 1e-6);
        }
        assert!(shape(0.5, 0.8) < 0.5 && shape(0.5, -0.8) > 0.5);
    }

    #[test]
    fn curve_for_mid_inverts_shape() {
        for c in [-0.9, -0.3, 0.0, 0.5, 0.9] {
            let back = curve_for_mid(shape(0.5, c));
            assert!((back - c).abs() < 1e-3, "{c} -> {back}");
        }
    }

    #[test]
    fn adsr_goes_up_down_to_sustain_and_releases() {
        let mut e = EnvelopeProcessNode::new(NodeId(1));
        e.set_param(0, ParamValue::Float(0.01));
        e.set_param(1, ParamValue::Float(0.01));
        e.set_param(3, ParamValue::Float(0.01));
        e.write_input(0, 1.0);
        let mut peak = 0.0f32;
        for _ in 0..100 { e.process(); peak = peak.max(e.read_output(0)); }
        assert!((peak - 1.0).abs() < 1e-3);
        assert!((e.read_output(0) - 0.6).abs() < 1e-3);
        e.write_input(0, 0.0);
        for _ in 0..30 { e.process(); }
        assert_eq!(e.read_output(0), 0.0);
    }
}
