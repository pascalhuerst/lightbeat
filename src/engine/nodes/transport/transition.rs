use super::easing::EasingCurve;
use crate::engine::types::*;

const MAX_CHANNELS: usize = 12; // Palette is the widest at 4×RGB

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionMode {
    Float,
    Color,
    Palette,
}

impl TransitionMode {
    pub fn label(&self) -> &'static str {
        match self { Self::Float => "Float", Self::Color => "Color", Self::Palette => "Palette" }
    }
    pub fn value_type(&self) -> PortType {
        match self { Self::Float => PortType::Untyped, Self::Color => PortType::Color, Self::Palette => PortType::Palette }
    }
    pub fn channels(&self) -> usize {
        match self { Self::Float => 1, Self::Color => 3, Self::Palette => 12 }
    }
    pub fn from_index(i: usize) -> Self {
        match i { 1 => Self::Color, 2 => Self::Palette, _ => Self::Float }
    }
    pub fn to_index(&self) -> usize {
        match self { Self::Float => 0, Self::Color => 1, Self::Palette => 2 }
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
            progress: 0.0,
            prev_phase: 0.0,
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
            let ch = self.mode.channels();
            for i in 0..ch {
                self.output[i] = self.from[i] + (self.to[i] - self.from[i]) * eased;
            }

            if self.progress >= 1.0 {
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
                options: vec!["Float".into(), "Color".into(), "Palette".into()],
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
