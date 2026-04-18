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
//! `range` and `center` follow the "input-overrides-param" pattern — when
//! the corresponding input port is wired, the wire value takes over and
//! the inspector param is hidden.

use crate::engine::types::*;

pub struct BipolarProcessNode {
    id: NodeId,
    in_value: f32,
    range: f32,
    center: f32,
    range_in: f32,
    center_in: f32,
    range_connected: bool,
    center_connected: bool,
    out_value: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl BipolarProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            in_value: 0.5,
            range: 1.0,
            center: 0.0,
            range_in: 1.0,
            center_in: 0.0,
            range_connected: false,
            center_connected: false,
            out_value: 0.0,
            inputs: vec![
                PortDef::new("in", PortType::Untyped),
                PortDef::new("range", PortType::Untyped),
                PortDef::new("center", PortType::Untyped),
            ],
            outputs: vec![PortDef::new("out", PortType::Untyped)],
        }
    }

    fn effective_range(&self) -> f32 {
        if self.range_connected { self.range_in } else { self.range }
    }
    fn effective_center(&self) -> f32 {
        if self.center_connected { self.center_in } else { self.center }
    }
}

impl ProcessNode for BipolarProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Bipolar" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        match pi {
            0 => self.in_value = v,
            1 => self.range_in = v,
            2 => self.center_in = v,
            _ => {}
        }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi {
            0 => self.in_value,
            1 => self.range_in,
            2 => self.center_in,
            _ => 0.0,
        }
    }

    fn set_input_connections(&mut self, connected: &[bool]) {
        self.range_connected = connected.get(1).copied().unwrap_or(false);
        self.center_connected = connected.get(2).copied().unwrap_or(false);
    }

    fn process(&mut self) {
        let r = self.effective_range();
        let c = self.effective_center();
        self.out_value = (self.in_value - 0.5) * r + c;
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.out_value } else { 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float {
                name: "Range".into(),
                value: self.range,
                min: -10.0, max: 10.0, step: 0.01, unit: "",
            },
            ParamDef::Float {
                name: "Center".into(),
                value: self.center,
                min: -10.0, max: 10.0, step: 0.01, unit: "",
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.range = value.as_f32(),
            1 => self.center = value.as_f32(),
            _ => {}
        }
    }
}
