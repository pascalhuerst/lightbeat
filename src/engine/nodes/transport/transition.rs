use super::easing::EasingCurve;
use crate::engine::types::*;

const MAX_CHANNELS: usize = 3; // Color is the widest at 3

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionMode {
    Float,
    Color,
}

impl TransitionMode {
    pub fn label(&self) -> &'static str {
        match self { Self::Float => "Float", Self::Color => "Color" }
    }
    pub fn value_type(&self) -> PortType {
        match self { Self::Float => PortType::Untyped, Self::Color => PortType::Color }
    }
    pub fn channels(&self) -> usize {
        match self { Self::Float => 1, Self::Color => 3 }
    }
    pub fn from_index(i: usize) -> Self {
        match i { 1 => Self::Color, _ => Self::Float }
    }
    pub fn to_index(&self) -> usize {
        match self { Self::Float => 0, Self::Color => 1 }
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
    phase_at_trigger: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl TransitionProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mode = TransitionMode::Color;
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
            phase_at_trigger: 0.0,
            inputs: Self::build_inputs(mode),
            outputs: Self::build_outputs(mode),
        }
    }

    fn build_inputs(mode: TransitionMode) -> Vec<PortDef> {
        vec![
            PortDef::new("trigger", PortType::Logic),
            PortDef::new("phase", PortType::Phase),
            PortDef::new("value", mode.value_type()),
        ]
    }

    fn build_outputs(mode: TransitionMode) -> Vec<PortDef> {
        vec![PortDef::new("out", mode.value_type())]
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
            self.from = self.output;
            self.to = self.value_in;
            self.active = true;
            self.phase_at_trigger = self.phase_in;
        }
        self.prev_trigger = gate;

        if self.active {
            // Compute progress: how far the phase has advanced since trigger,
            // normalized to 0..1 over one full phase cycle.
            let mut progress = self.phase_in - self.phase_at_trigger;
            if progress < 0.0 { progress += 1.0; }
            let progress = progress.clamp(0.0, 1.0);

            let eased = self.curve.apply(progress);
            let ch = self.mode.channels();
            for i in 0..ch {
                self.output[i] = self.from[i] + (self.to[i] - self.from[i]) * eased;
            }

            // Complete when phase wraps back.
            if progress >= 0.999 {
                self.output = self.to;
                self.active = false;
            }
        }
    }

    fn read_output(&self, channel: usize) -> f32 {
        self.output.get(channel).copied().unwrap_or(0.0)
    }

    fn params(&self) -> Vec<ParamDef> {
        let curve_options: Vec<String> = EasingCurve::all().iter().map(|c| c.label().into()).collect();
        vec![
            ParamDef::Choice {
                name: "Mode".into(),
                value: self.mode.to_index(),
                options: vec!["Float".into(), "Color".into()],
            },
            ParamDef::Choice {
                name: "Curve".into(),
                value: self.curve.to_index(),
                options: curve_options,
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match (index, value) {
            (0, ParamValue::Choice(v)) => {
                let new_mode = TransitionMode::from_index(v);
                if new_mode != self.mode {
                    self.mode = new_mode;
                    self.inputs = Self::build_inputs(new_mode);
                    self.outputs = Self::build_outputs(new_mode);
                }
            }
            (1, ParamValue::Choice(v)) => {
                self.curve = EasingCurve::from_index(v);
            }
            _ => {}
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        let progress = if self.active {
            let mut p = self.phase_in - self.phase_at_trigger;
            if p < 0.0 { p += 1.0; }
            p.clamp(0.0, 1.0)
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
