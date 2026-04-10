use std::any::Any;

use egui::Ui;

use crate::widgets::nodes::{NodeId, NodeWidget, ParamDef, ParamValue, PortDef, PortType};

pub struct PhaseScalerNode {
    id: NodeId,
    /// Power of 2 exponent. 0 = ×1, 1 = ×2, 2 = ×4, -1 = ÷2, -2 = ÷4, etc.
    exponent: i32,
    offset: f32,
    phase_in: f32,
    prev_phase_in: f32,
    phase_out: f32,
    /// Counts input phase wraps, used for division to stay in sync.
    cycle_counter: u64,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl PhaseScalerNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            exponent: 0,
            offset: 0.0,
            phase_in: 0.0,
            prev_phase_in: 0.0,
            phase_out: 0.0,
            cycle_counter: 0,
            inputs: vec![PortDef::new("phase", PortType::Phase)],
            outputs: vec![PortDef::new("phase", PortType::Phase)],
        }
    }

    fn label(&self) -> String {
        match self.exponent {
            0 => "×1".into(),
            e if e > 0 => format!("×{}", 1u64 << e),
            e => format!("÷{}", 1u64 << (-e)),
        }
    }
}

impl NodeWidget for PhaseScalerNode {
    fn node_id(&self) -> NodeId {
        self.id
    }

    fn type_name(&self) -> &'static str {
        "Phase Scaler"
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

    fn read_input(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.phase_in,
            _ => 0.0,
        }
    }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if port_index == 0 {
            self.phase_in = value;
        }
    }

    fn process(&mut self) {
        // Detect input phase wrap (crossing from ~1 back to ~0).
        let delta = self.phase_in - self.prev_phase_in;
        if delta < -0.5 {
            self.cycle_counter += 1;
        }
        self.prev_phase_in = self.phase_in;

        if self.exponent >= 0 {
            // Multiply: output cycles faster, computed directly.
            // ×1: output = input
            // ×2: output = (input * 2) % 1
            // ×4: output = (input * 4) % 1
            let multiplier = (1u64 << self.exponent) as f32;
            self.phase_out = ((self.phase_in * multiplier) + self.offset).rem_euclid(1.0);
        } else {
            // Divide: output cycles slower, stays in sync.
            // ÷2: takes 2 input cycles for 1 output cycle.
            //   cycle 0: output = input/2        → 0..0.5
            //   cycle 1: output = input/2 + 0.5  → 0.5..1.0
            // ÷4: takes 4 input cycles for 1 output cycle.
            //   cycle 0: output = input/4         → 0..0.25
            //   cycle 1: output = input/4 + 0.25  → 0.25..0.5
            //   etc.
            let divisor = (1u64 << (-self.exponent)) as f32;
            let sub_cycle = (self.cycle_counter % (divisor as u64)) as f32;
            self.phase_out = ((sub_cycle + self.phase_in) / divisor + self.offset).rem_euclid(1.0);
        }
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.phase_out,
            _ => 0.0,
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Int {
                name: "Exponent".into(),
                value: self.exponent as i64,
                min: -6, // ÷64
                max: 6,  // ×64
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
            (0, ParamValue::Int(v)) => self.exponent = v as i32,
            (1, ParamValue::Float(v)) => self.offset = v,
            _ => {}
        }
    }

    fn show_content(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.small_button("÷2").clicked() && self.exponent > -6 {
                self.exponent -= 1;
            }
            ui.label(self.label());
            if ui.small_button("×2").clicked() && self.exponent < 6 {
                self.exponent += 1;
            }
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
