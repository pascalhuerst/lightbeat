use crate::engine::param_spec::FloatParam;
use crate::engine::types::*;


// Wide because the old input ports passed any value through (BPM, DMX values, counts).
const IN_MIN: FloatParam = FloatParam::new("In Min", -100.0, 100.0, 0.1, "");
const IN_MAX: FloatParam = FloatParam::new("In Max", -100.0, 100.0, 0.1, "");

/// Maps an input from [in_min, in_max] to [0, 1]. A new node has both ends
/// exposed as inputs (see `ScalerWidget::default_promoted`).
pub struct ScalerProcessNode {
    id: NodeId,
    in_min: f32,
    in_max: f32,
    value_in: f32,
    value_out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports() -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![
            PortDef::new("in", PortType::Any),
        ],
        vec![PortDef::new("out", PortType::Untyped)],
    )
}

impl ScalerProcessNode {
    pub fn new(id: NodeId) -> Self {
        let (inputs, outputs) = ports();
        Self {
            id,
            in_min: 0.0,
            in_max: 1.0,
            value_in: 0.0,
            value_out: 0.0,
            inputs,
            outputs,
        }
    }
}

impl ProcessNode for ScalerProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Range to Unit" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.value_in = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.value_in } else { 0.0 }
    }

    fn process(&mut self) {
        let lo = self.in_min;
        let hi = self.in_max;
        let range = hi - lo;
        if range.abs() > 1e-10 {
            self.value_out = ((self.value_in - lo) / range).clamp(0.0, 1.0);
        } else {
            self.value_out = 0.0;
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.value_out } else { 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![IN_MIN.def(self.in_min), IN_MAX.def(self.in_max)]
    }

    fn unclamped_input_params(&self) -> &'static [&'static str] { &["In Min", "In Max"] }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.in_min = IN_MIN.apply(&value),
            1 => self.in_max = IN_MAX.apply(&value),
            _ => {}
        }
    }
}
