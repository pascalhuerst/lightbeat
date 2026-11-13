//! Builds a 4-stop Gradient from a Palette input plus one position per
//! palette slot. The positions are settings (`Pos 1..Pos 4`); a new node has
//! them exposed as inputs (see `PaletteToGradientWidget::default_promoted`).

use crate::color::{Gradient, GradientStop, Rgb};
use crate::engine::types::*;

const N: usize = 4;

pub struct PaletteToGradientProcessNode {
    id: NodeId,
    /// Flat palette channels (12 floats = 4 × RGB).
    palette_in: [f32; 12],
    pos_param: [f32; N],
    /// Output channels (40 floats = 8 × (r, g, b, a, position)). Unused
    /// stops are marked with alpha = -1 via `Gradient::to_channels`.
    channels: [f32; GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS],
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports() -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![
            PortDef::new("palette", PortType::Palette),
        ],
        vec![PortDef::new("gradient", PortType::Gradient)],
    )
}

impl PaletteToGradientProcessNode {
    pub fn new(id: NodeId) -> Self {
        let (inputs, outputs) = ports();
        Self {
            id,
            palette_in: [0.0; 12],
            // Default: evenly spread across 0..1.
            pos_param: [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0],
            channels: [0.0; GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS],
            inputs,
            outputs,
        }
    }

    fn effective_pos(&self, i: usize) -> f32 {
        self.pos_param[i].clamp(0.0, 1.0)
    }
}

impl ProcessNode for PaletteToGradientProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Palette to Gradient" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, channel: usize, v: f32) {
        if channel < 12 {
            self.palette_in[channel] = v;
        }
    }
    fn read_input(&self, channel: usize) -> f32 {
        self.palette_in.get(channel).copied().unwrap_or(0.0)
    }

    fn process(&mut self) {
        let mut stops = Vec::with_capacity(N);
        for i in 0..N {
            let base = i * 3;
            let color = Rgb::new(
                self.palette_in[base],
                self.palette_in[base + 1],
                self.palette_in[base + 2],
            );
            stops.push(GradientStop {
                position: self.effective_pos(i),
                color,
                alpha: 1.0,
            });
        }
        let g = Gradient::new(stops);
        self.channels = g.to_channels();
    }

    fn read_output(&self, channel: usize) -> f32 {
        self.channels.get(channel).copied().unwrap_or(0.0)
    }

    fn params(&self) -> Vec<ParamDef> {
        (0..N).map(|i| ParamDef::Float {
            name: format!("Pos {}", i + 1).into(),
            value: self.pos_param[i],
            min: 0.0, max: 1.0, step: 0.01, unit: "",
        }).collect()
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index < N {
            self.pos_param[index] = value.as_f32().clamp(0.0, 1.0);
        }
    }
}
