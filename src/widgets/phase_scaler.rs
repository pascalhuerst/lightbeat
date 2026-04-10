use std::any::Any;

use egui::Ui;

use crate::widgets::nodes::{NodeId, NodeWidget, ParamDef, ParamValue, PortDef, PortType};

pub struct PhaseScalerNode {
    id: NodeId,
    multiplier: f32,
    offset: f32,
    phase_in: f32,
    phase_out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl PhaseScalerNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            multiplier: 1.0,
            offset: 0.0,
            phase_in: 0.0,
            phase_out: 0.0,
            inputs: vec![PortDef::new("phase", PortType::Phase)],
            outputs: vec![PortDef::new("phase", PortType::Phase)],
        }
    }
}

impl NodeWidget for PhaseScalerNode {
    fn node_id(&self) -> NodeId {
        self.id
    }

    fn title(&self) -> &str {
        "Phase Scaler"
    }

    fn inputs(&self) -> &[PortDef] {
        &self.inputs
    }

    fn outputs(&self) -> &[PortDef] {
        &self.outputs
    }

    fn min_width(&self) -> f32 {
        120.0
    }

    fn min_content_height(&self) -> f32 {
        20.0
    }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if port_index == 0 {
            self.phase_in = value;
        }
    }

    fn process(&mut self) {
        self.phase_out = (self.phase_in * self.multiplier + self.offset).rem_euclid(1.0);
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.phase_out,
            _ => 0.0,
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float {
                name: "Multiply".into(),
                value: self.multiplier,
                min: 0.0,
                max: 64.0,
                step: 0.25,
                unit: "×",
            },
            ParamDef::Float {
                name: "Offset".into(),
                value: self.offset,
                min: 0.0,
                max: 1.0,
                step: 0.01,
                unit: "",
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match (index, value) {
            (0, ParamValue::Float(v)) => self.multiplier = v,
            (1, ParamValue::Float(v)) => self.offset = v,
            _ => {}
        }
    }

    fn show_content(&mut self, ui: &mut Ui) {
        ui.label(format!("×{:.2}", self.multiplier));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
