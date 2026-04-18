//! Builds a 4-stop Gradient from a Palette input plus one position per
//! palette slot. Each slot's position follows the input-overrides-param
//! pattern: wire `pos1..pos4` to drive the stop positions dynamically, or
//! dial them in from the inspector when unwired.

use crate::color::{Gradient, GradientStop, Rgb};
use crate::engine::types::*;

const N: usize = 4;

pub struct PaletteToGradientProcessNode {
    id: NodeId,
    /// Flat palette channels (12 floats = 4 × RGB).
    palette_in: [f32; 12],
    /// Wired stop positions (one per palette slot).
    pos_in: [f32; N],
    /// Inspector defaults, used when the matching pos port isn't wired.
    pos_param: [f32; N],
    pos_connected: [bool; N],
    /// Output channels (40 floats = 8 × (r, g, b, a, position)). Unused
    /// stops are marked with alpha = -1 via `Gradient::to_channels`.
    channels: [f32; GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS],
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl PaletteToGradientProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            palette_in: [0.0; 12],
            pos_in: [0.0; N],
            // Default: evenly spread across 0..1.
            pos_param: [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0],
            pos_connected: [false; N],
            channels: [0.0; GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS],
            inputs: vec![
                PortDef::new("palette", PortType::Palette),
                PortDef::new("pos1", PortType::Untyped),
                PortDef::new("pos2", PortType::Untyped),
                PortDef::new("pos3", PortType::Untyped),
                PortDef::new("pos4", PortType::Untyped),
            ],
            outputs: vec![PortDef::new("gradient", PortType::Gradient)],
        }
    }

    fn effective_pos(&self, i: usize) -> f32 {
        let p = if self.pos_connected[i] { self.pos_in[i] } else { self.pos_param[i] };
        p.clamp(0.0, 1.0)
    }
}

impl ProcessNode for PaletteToGradientProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Palette to Gradient" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, channel: usize, v: f32) {
        // Palette occupies channels 0..12 (flat), then one channel each for
        // pos1..pos4 at 12, 13, 14, 15.
        if channel < 12 {
            self.palette_in[channel] = v;
        } else if channel < 12 + N {
            self.pos_in[channel - 12] = v;
        }
    }
    fn read_input(&self, channel: usize) -> f32 {
        if channel < 12 {
            self.palette_in[channel]
        } else if channel < 12 + N {
            self.pos_in[channel - 12]
        } else { 0.0 }
    }

    fn set_input_connections(&mut self, connected: &[bool]) {
        // Port 0 = palette; ports 1..=4 = pos1..pos4.
        for i in 0..N {
            self.pos_connected[i] = connected.get(1 + i).copied().unwrap_or(false);
        }
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
            name: format!("Pos {}", i + 1),
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
