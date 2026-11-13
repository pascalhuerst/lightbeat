use super::easing::EasingCurve;
use crate::engine::types::*;

const MAX_CHANNELS: usize = GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionMode {
    Neutral,
    Float,
    Color,
    Palette,
    /// No value to move: the output is the eased progress itself, a phase.
    Phase,
    Position,
    Gradient,
}

impl TransitionMode {
    pub fn value_type(&self) -> PortType {
        match self {
            Self::Neutral => PortType::Any,
            Self::Float => PortType::Untyped,
            Self::Color => PortType::Color,
            Self::Palette => PortType::Palette,
            Self::Phase => PortType::Phase,
            Self::Position => PortType::Position,
            Self::Gradient => PortType::Gradient,
        }
    }
    pub fn channels(&self) -> usize {
        match self {
            Self::Neutral => 1,
            Self::Float => 1,
            Self::Color => 3,
            Self::Palette => 12,
            Self::Phase => 1,
            Self::Position => 2,
            Self::Gradient => GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS,
        }
    }
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Float,
            2 => Self::Color,
            3 => Self::Palette,
            4 => Self::Phase,
            5 => Self::Position,
            6 => Self::Gradient,
            _ => Self::Neutral,
        }
    }
    pub fn to_index(&self) -> usize {
        match self {
            Self::Neutral => 0,
            Self::Float => 1,
            Self::Color => 2,
            Self::Palette => 3,
            Self::Phase => 4,
            Self::Position => 5,
            Self::Gradient => 6,
        }
    }
}

pub struct TransitionDisplay {
    pub mode: TransitionMode,
    pub curve: EasingCurve,
    pub phase: f32,
    pub active: bool,
}

pub struct TransitionProcessNode {
    id: NodeId,
    mode: TransitionMode,
    curve: EasingCurve,
    // Inputs: trigger, phase, value (1 or 3 channels).
    trigger_in: f32,
    phase_in: f32,
    value_in: [f32; MAX_CHANNELS],
    prev_trigger: bool,
    // Transition state.
    from: [f32; MAX_CHANNELS],
    to: [f32; MAX_CHANNELS],
    output: [f32; MAX_CHANNELS],
    active: bool,
    progress: f32,
    prev_phase: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl TransitionProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mode = TransitionMode::Neutral;
        Self {
            id,
            mode,
            curve: EasingCurve::Linear,
            trigger_in: 0.0,
            phase_in: 0.0,
            value_in: [0.0; MAX_CHANNELS],
            prev_trigger: false,
            from: [0.0; MAX_CHANNELS],
            to: [0.0; MAX_CHANNELS],
            output: [0.0; MAX_CHANNELS],
            active: false,
            progress: 0.0,
            prev_phase: 0.0,
            inputs: Self::build_inputs(mode),
            outputs: Self::build_outputs(mode),
        }
    }

    fn build_inputs(mode: TransitionMode) -> Vec<PortDef> {
        let mut ports = vec![
            PortDef::new("trigger", PortType::Logic),
            PortDef::new("phase", PortType::Phase),
        ];
        if mode != TransitionMode::Phase {
            ports.push(PortDef::new("in", mode.value_type()));
        }
        ports
    }

    fn build_outputs(mode: TransitionMode) -> Vec<PortDef> {
        let name = if mode == TransitionMode::Phase { "phase" } else { "out" };
        vec![PortDef::new(name, mode.value_type())]
    }
}

impl ProcessNode for TransitionProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Transition" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, channel: usize, value: f32) {
        // Layout: [trigger] [phase] [value channels...]
        // With port_base_index: trigger=0, phase=1, value=2 (or 2,3,4 for Color)
        match channel {
            0 => self.trigger_in = value,
            1 => self.phase_in = value,
            c => {
                let vi = c - 2;
                if vi < MAX_CHANNELS { self.value_in[vi] = value; }
            }
        }
    }

    fn read_input(&self, channel: usize) -> f32 {
        match channel {
            0 => self.trigger_in,
            1 => self.phase_in,
            c => {
                let vi = c - 2;
                if vi < MAX_CHANNELS { self.value_in[vi] } else { 0.0 }
            }
        }
    }

    fn process(&mut self) {
        let gate = self.trigger_in >= 0.5;

        // On rising edge: capture from (current output) and to (current input).
        if gate && !self.prev_trigger {
            if self.mode == TransitionMode::Phase {
                self.output[0] = 0.0;
            }
            self.from = self.output;
            self.to = self.value_in;
            self.active = true;
            self.progress = 0.0;
        }
        self.prev_trigger = gate;

        // Accumulate progress from phase delta.
        let mut delta = self.phase_in - self.prev_phase;
        if delta < -0.5 { delta += 1.0; }
        if delta > 0.5 { delta -= 1.0; }
        self.prev_phase = self.phase_in;

        if self.active {
            if delta > 0.0 {
                self.progress += delta;
            }
            let p = self.progress.clamp(0.0, 1.0);

            let eased = self.curve.apply(p);
            if self.mode == TransitionMode::Phase {
                self.output[0] = eased;
            } else {
                let ch = self.mode.channels();
                for i in 0..ch {
                    self.output[i] = self.from[i] + (self.to[i] - self.from[i]) * eased;
                }
            }

            if self.progress >= 1.0 {
                if self.mode == TransitionMode::Phase {
                    self.output[0] = 1.0;
                } else {
                    self.output = self.to;
                }
                self.active = false;
            }
        }
    }

    fn read_output(&self, channel: usize) -> f32 {
        self.output.get(channel).copied().unwrap_or(0.0)
    }

    fn non_promotable_params(&self) -> &'static [&'static str] {
        &["Type"]
    }

    fn params(&self) -> Vec<ParamDef> {
        let curve_options: Vec<String> = EasingCurve::all().iter().map(|c| c.label().into()).collect();
        vec![
            ParamDef::Choice {
                name: "Type".into(),
                value: self.mode.to_index(),
                // Position and Gradient come last because saved settings go by position.
                options: ["Auto", "Number", "Color", "Palette", "Phase out", "Position", "Gradient"]
                    .iter().map(|s| s.to_string()).collect(),
            },
            ParamDef::Choice {
                name: "Curve".into(),
                value: self.curve.to_index(),
                options: curve_options,
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => {
                let new_mode = TransitionMode::from_index(value.as_usize());
                if new_mode != self.mode {
                    self.mode = new_mode;
                    self.inputs = Self::build_inputs(new_mode);
                    self.outputs = Self::build_outputs(new_mode);
                }
            }
            1 => self.curve = EasingCurve::from_index(value.as_usize()),
            _ => {}
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        let progress = if self.active {
            self.progress.clamp(0.0, 1.0)
        } else {
            0.0
        };
        shared.display = Some(Box::new(TransitionDisplay {
            mode: self.mode,
            curve: self.curve,
            phase: progress,
            active: self.active,
        }));
    }
}

#[cfg(test)]
mod phase_mode_tests {
    use super::*;

    #[test]
    fn a_position_moves_both_axes_toward_the_new_value() {
        let mut n = TransitionProcessNode::new(NodeId(1));
        n.set_param(0, ParamValue::Choice(TransitionMode::Position.to_index()));
        assert_eq!(n.outputs()[0].port_type, PortType::Position);
        n.write_input(1, 0.0);
        n.process();
        n.write_input(2, 1.0);
        n.write_input(3, 0.5);
        n.write_input(0, 1.0);
        n.process();
        n.write_input(0, 0.0);
        n.write_input(1, 0.5);
        n.process();
        assert!((n.read_output(0) - 0.5).abs() < 1e-4);
        assert!((n.read_output(1) - 0.25).abs() < 1e-4);
    }

    #[test]
    fn phase_out_follows_the_phase_through_the_curve_after_a_trigger() {
        let mut n = TransitionProcessNode::new(NodeId(1));
        n.set_param(0, ParamValue::Choice(TransitionMode::Phase.to_index()));
        n.set_param(1, ParamValue::Choice(EasingCurve::EaseInQuad.to_index()));
        assert_eq!(n.inputs().len(), 2);
        n.write_input(1, 0.0);
        n.process();
        n.write_input(0, 1.0);
        n.process();
        n.write_input(0, 0.0);
        for i in 1..=5 {
            n.write_input(1, i as f32 * 0.1);
            n.process();
        }
        // Halfway along an ease-in quad is a quarter.
        assert!((n.read_output(0) - 0.25).abs() < 1e-3, "{}", n.read_output(0));
        for i in 6..=11 {
            n.write_input(1, (i as f32 * 0.1).min(1.0));
            n.process();
        }
        assert_eq!(n.read_output(0), 1.0);
    }
}
