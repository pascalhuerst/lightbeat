//! Unipolar → bipolar remapper.
//!
//! Formula: `out = (in - 0.5) * range + center`.
//!
//! Typical use: a MIDI knob produces a **unipolar** 0..1 value; you want it
//! to swing ±half-range around a `center` (e.g. "hue shift by ±range/2
//! around the current hue"). Defaults (range=1, center=0) map a unipolar
//! input to the **bipolar** range -0.5..+0.5. Setting range=2 gives a full
//! -1..+1 converter.
//!
//! A new node has `range` and `center` exposed as inputs (see
//! `BipolarWidget::default_promoted`).

use crate::engine::param_spec::FloatParam;
use crate::engine::types::*;


const RANGE: FloatParam = FloatParam::new("Range", -10.0, 10.0, 0.01, "");
const CENTER: FloatParam = FloatParam::new("Center", -10.0, 10.0, 0.01, "");

pub struct BipolarProcessNode {
    id: NodeId,
    in_value: f32,
    range: f32,
    center: f32,
    out_value: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports() -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![
            PortDef::new("in", PortType::Untyped),
        ],
        vec![PortDef::new("out", PortType::Untyped)],
    )
}

impl BipolarProcessNode {
    pub fn new(id: NodeId) -> Self {
        let (inputs, outputs) = ports();
        Self {
            id,
            in_value: 0.5,
            range: 1.0,
            center: 0.0,
            out_value: 0.0,
            inputs,
            outputs,
        }
    }
}

impl ProcessNode for BipolarProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Bipolar" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.in_value = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.in_value } else { 0.0 }
    }

    fn process(&mut self) {
        self.out_value = (self.in_value - 0.5) * self.range + self.center;
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.out_value } else { 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![RANGE.def(self.range), CENTER.def(self.center)]
    }

    fn unclamped_input_params(&self) -> &'static [&'static str] { &["Range", "Center"] }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.range = RANGE.apply(&value),
            1 => self.center = CENTER.apply(&value),
            _ => {}
        }
    }
}
