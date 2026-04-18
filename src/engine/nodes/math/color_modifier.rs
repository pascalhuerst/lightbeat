//! Color Modifier — applies a single HSV / brightness / alpha operation to
//! a Color or Gradient signal. Auto-detects the input type and passes it
//! through unchanged in kind, modified in content.
//!
//! For Gradient inputs, the operation is applied to every active stop
//! (positions and unused markers preserved).

use crate::color::{ColorConvert, Rgb};
use crate::engine::types::*;

const MAX_MAIN_CHANNELS: usize = GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS; // 40

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierOp {
    HueShift,
    Saturation,
    Value,
    Brightness,
    Alpha,
}

pub const MODIFIER_OP_NAMES: &[&str] = &[
    "Hue Shift", "Saturation", "Value", "Brightness", "Alpha",
];

impl ModifierOp {
    pub fn to_index(self) -> usize {
        match self {
            ModifierOp::HueShift => 0,
            ModifierOp::Saturation => 1,
            ModifierOp::Value => 2,
            ModifierOp::Brightness => 3,
            ModifierOp::Alpha => 4,
        }
    }
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => ModifierOp::Saturation,
            2 => ModifierOp::Value,
            3 => ModifierOp::Brightness,
            4 => ModifierOp::Alpha,
            _ => ModifierOp::HueShift,
        }
    }
    /// Identity / no-op value for the amount input.
    pub fn identity_amount(self) -> f32 {
        match self {
            ModifierOp::HueShift => 0.0,
            _ => 1.0,
        }
    }
}

pub struct ColorModifierDisplay {
    pub port_type: PortType,
    pub op: ModifierOp,
    pub amount: f32,
}

pub struct ColorModifierProcessNode {
    id: NodeId,
    port_type: PortType,
    op: ModifierOp,
    /// Layout: [main input channels ...][amount (1)]
    /// Main size depends on port_type (Color=3, Gradient=40); amount is
    /// always at `port_type.channel_count()`.
    input_values: Vec<f32>,
    output_values: Vec<f32>,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl ColorModifierProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mut node = Self {
            id,
            port_type: PortType::Any,
            op: ModifierOp::HueShift,
            input_values: vec![0.0; MAX_MAIN_CHANNELS + 1],
            output_values: vec![0.0; MAX_MAIN_CHANNELS],
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        node.rebuild();
        node
    }

    fn rebuild(&mut self) {
        let pt = self.port_type;
        let main_name = match pt {
            PortType::Color => "color",
            PortType::Gradient => "gradient",
            _ => "?",
        };
        self.inputs = vec![
            PortDef::new(main_name, pt),
            PortDef::new("amount", PortType::Untyped),
        ];
        self.outputs = vec![PortDef::new(main_name, pt)];

        for v in self.input_values.iter_mut() { *v = 0.0; }
        for v in self.output_values.iter_mut() { *v = 0.0; }
        // Seed amount with op's identity so an unwired node is a pass-through.
        let amount_idx = pt.channel_count().min(self.input_values.len() - 1);
        self.input_values[amount_idx] = self.op.identity_amount();
    }
}

impl ProcessNode for ColorModifierProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color Modifier" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, channel: usize, v: f32) {
        if channel < self.input_values.len() { self.input_values[channel] = v; }
    }
    fn read_input(&self, channel: usize) -> f32 {
        self.input_values.get(channel).copied().unwrap_or(0.0)
    }

    fn process(&mut self) {
        let n = self.port_type.channel_count();
        let amount = self.input_values.get(n).copied().unwrap_or(self.op.identity_amount());

        match self.port_type {
            PortType::Color => {
                let rgb = Rgb::new(self.input_values[0], self.input_values[1], self.input_values[2]);
                let (out_rgb, _) = apply_op(self.op, rgb, 1.0, amount);
                self.output_values[0] = out_rgb.r;
                self.output_values[1] = out_rgb.g;
                self.output_values[2] = out_rgb.b;
            }
            PortType::Gradient => {
                for i in 0..GRADIENT_STOP_COUNT {
                    let base = i * GRADIENT_STOP_FLOATS;
                    let r = self.input_values[base];
                    let g = self.input_values[base + 1];
                    let b = self.input_values[base + 2];
                    let a = self.input_values[base + 3];
                    let pos = self.input_values[base + 4];

                    if a < 0.0 {
                        // Unused slot — preserve marker.
                        self.output_values[base] = 0.0;
                        self.output_values[base + 1] = 0.0;
                        self.output_values[base + 2] = 0.0;
                        self.output_values[base + 3] = -1.0;
                        self.output_values[base + 4] = pos;
                        continue;
                    }

                    let (out_rgb, out_a) = apply_op(self.op, Rgb::new(r, g, b), a, amount);
                    self.output_values[base] = out_rgb.r;
                    self.output_values[base + 1] = out_rgb.g;
                    self.output_values[base + 2] = out_rgb.b;
                    self.output_values[base + 3] = out_a;
                    self.output_values[base + 4] = pos;
                }
            }
            _ => {}
        }
    }

    fn read_output(&self, channel: usize) -> f32 {
        self.output_values.get(channel).copied().unwrap_or(0.0)
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Choice {
                name: "Type".into(),
                value: match self.port_type {
                    PortType::Color => 1,
                    PortType::Gradient => 2,
                    _ => 0,
                },
                options: vec!["Auto".into(), "Color".into(), "Gradient".into()],
            },
            ParamDef::Choice {
                name: "Op".into(),
                value: self.op.to_index(),
                options: MODIFIER_OP_NAMES.iter().map(|s| s.to_string()).collect(),
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => {
                let new_pt = match value.as_usize() {
                    1 => PortType::Color,
                    2 => PortType::Gradient,
                    _ => PortType::Any,
                };
                if new_pt != self.port_type {
                    self.port_type = new_pt;
                    self.rebuild();
                }
            }
            1 => {
                let new_op = ModifierOp::from_index(value.as_usize());
                if new_op != self.op {
                    self.op = new_op;
                    // Reseed amount with new identity for the new op.
                    let idx = self.port_type.channel_count().min(self.input_values.len() - 1);
                    self.input_values[idx] = self.op.identity_amount();
                }
            }
            _ => {}
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        let amount_idx = self.port_type.channel_count();
        let amount = self.input_values.get(amount_idx).copied().unwrap_or(0.0);
        shared.display = Some(Box::new(ColorModifierDisplay {
            port_type: self.port_type,
            op: self.op,
            amount,
        }));
    }
}

fn apply_op(op: ModifierOp, rgb: Rgb, alpha: f32, amount: f32) -> (Rgb, f32) {
    match op {
        ModifierOp::HueShift => {
            let mut hsv = rgb.to_hsv();
            hsv.h = (hsv.h + amount).rem_euclid(1.0);
            (hsv.to_rgb(), alpha)
        }
        ModifierOp::Saturation => {
            let mut hsv = rgb.to_hsv();
            hsv.s = (hsv.s * amount).clamp(0.0, 1.0);
            (hsv.to_rgb(), alpha)
        }
        ModifierOp::Value => {
            let mut hsv = rgb.to_hsv();
            hsv.v = (hsv.v * amount).clamp(0.0, 1.0);
            (hsv.to_rgb(), alpha)
        }
        ModifierOp::Brightness => {
            let scaled = Rgb::new(
                (rgb.r * amount).clamp(0.0, 1.0),
                (rgb.g * amount).clamp(0.0, 1.0),
                (rgb.b * amount).clamp(0.0, 1.0),
            );
            (scaled, alpha)
        }
        ModifierOp::Alpha => {
            (rgb, (alpha * amount).clamp(0.0, 1.0))
        }
    }
}
