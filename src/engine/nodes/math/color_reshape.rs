//! Palette to Colors, Colors to Palette, Gradient to Colors, Colors to Gradient.
//!
//! Palettes and gradients are bundles of floats (a palette is four colors; a
//! gradient is up to eight stops of color, alpha and position). These nodes
//! take them apart and put them together again, so a color or a stop
//! position can be driven from the graph.

use crate::color::Gradient;
use crate::engine::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReshapeKind {
    PaletteToColors,
    ColorsToPalette,
    GradientToColors,
    ColorsToGradient,
}

pub const MIN_STOPS: usize = 2;
pub const MAX_STOPS: usize = GRADIENT_STOP_COUNT;
pub const DEFAULT_STOPS: usize = 4;
/// Channels per stop on the colors side: color (3), position, alpha.
const STOP_CHANNELS: usize = 5;

impl ReshapeKind {
    pub const ALL: [Self; 4] =
        [Self::PaletteToColors, Self::ColorsToPalette, Self::GradientToColors, Self::ColorsToGradient];

    pub fn type_name(self) -> &'static str {
        match self {
            Self::PaletteToColors => "Palette to Colors",
            Self::ColorsToPalette => "Colors to Palette",
            Self::GradientToColors => "Gradient to Colors",
            Self::ColorsToGradient => "Colors to Gradient",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::PaletteToColors => "Splits a palette into its four colors.",
            Self::ColorsToPalette => "Combines four colors into a palette.",
            Self::GradientToColors => {
                "Splits a gradient into the color, position and alpha of its stops (in order of position)."
            }
            Self::ColorsToGradient => {
                "Builds a gradient from stops, each with a color, a position and an alpha. Stops without a wired position are spread evenly; without a wired alpha they are opaque."
            }
        }
    }

    /// The gradient nodes let you choose how many stops to expose.
    pub fn has_stops(self) -> bool {
        matches!(self, Self::GradientToColors | Self::ColorsToGradient)
    }
}

pub fn clamp_stops(n: usize) -> usize {
    n.clamp(MIN_STOPS, MAX_STOPS)
}

fn palette_color_ports() -> Vec<PortDef> {
    ["primary", "secondary", "third", "fourth"]
        .iter()
        .map(|n| PortDef::new(*n, PortType::Color))
        .collect()
}

/// Per stop: color, position, alpha.
fn stop_ports(stops: usize) -> Vec<PortDef> {
    (1..=stops)
        .flat_map(|i| {
            [
                PortDef::new(format!("color {i}"), PortType::Color),
                PortDef::new(format!("position {i}"), PortType::Untyped),
                PortDef::new(format!("alpha {i}"), PortType::Untyped),
            ]
        })
        .collect()
}

/// (inputs, outputs). The widget builds its ports from the same function, so
/// the two halves cannot disagree.
pub fn ports(kind: ReshapeKind, stops: usize) -> (Vec<PortDef>, Vec<PortDef>) {
    match kind {
        ReshapeKind::PaletteToColors => (vec![PortDef::new("palette", PortType::Palette)], palette_color_ports()),
        ReshapeKind::ColorsToPalette => (palette_color_ports(), vec![PortDef::new("palette", PortType::Palette)]),
        ReshapeKind::GradientToColors => (vec![PortDef::new("gradient", PortType::Gradient)], stop_ports(stops)),
        ReshapeKind::ColorsToGradient => (stop_ports(stops), vec![PortDef::new("gradient", PortType::Gradient)]),
    }
}

const BUF: usize = GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS;

pub struct ColorReshapeProcessNode {
    id: NodeId,
    kind: ReshapeKind,
    stops: usize,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
    values: [f32; BUF],
    out: [f32; BUF],
    position_wired: [bool; MAX_STOPS],
    alpha_wired: [bool; MAX_STOPS],
}

impl ColorReshapeProcessNode {
    pub fn new(id: NodeId, kind: ReshapeKind) -> Self {
        let stops = DEFAULT_STOPS;
        let (inputs, outputs) = ports(kind, stops);
        Self {
            id,
            kind,
            stops,
            inputs,
            outputs,
            values: [0.0; BUF],
            out: [0.0; BUF],
            position_wired: [false; MAX_STOPS],
            alpha_wired: [false; MAX_STOPS],
        }
    }
}

impl ProcessNode for ColorReshapeProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.kind.type_name() }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, channel: usize, value: f32) {
        if let Some(slot) = self.values.get_mut(channel) {
            *slot = value;
        }
    }

    fn read_input(&self, channel: usize) -> f32 {
        self.values.get(channel).copied().unwrap_or(0.0)
    }

    fn set_input_connections(&mut self, connected: &[bool]) {
        if self.kind == ReshapeKind::ColorsToGradient {
            for i in 0..MAX_STOPS {
                // Ports per stop are color, position, alpha.
                self.position_wired[i] = connected.get(i * 3 + 1).copied().unwrap_or(false);
                self.alpha_wired[i] = connected.get(i * 3 + 2).copied().unwrap_or(false);
            }
        }
    }

    fn process(&mut self) {
        match self.kind {
            // A palette is four colors laid end to end, so these just copy.
            ReshapeKind::PaletteToColors | ReshapeKind::ColorsToPalette => {
                self.out[..12].copy_from_slice(&self.values[..12]);
            }
            ReshapeKind::GradientToColors => {
                self.out.fill(0.0);
                let gradient = Gradient::from_channels(&self.values);
                for (i, stop) in gradient.stops().iter().take(self.stops).enumerate() {
                    let base = i * STOP_CHANNELS;
                    self.out[base] = stop.color.r;
                    self.out[base + 1] = stop.color.g;
                    self.out[base + 2] = stop.color.b;
                    self.out[base + 3] = stop.position;
                    self.out[base + 4] = stop.alpha;
                }
            }
            ReshapeKind::ColorsToGradient => {
                for i in 0..MAX_STOPS {
                    let dst = i * GRADIENT_STOP_FLOATS;
                    if i >= self.stops {
                        // Marks the stop unused.
                        self.out[dst..dst + GRADIENT_STOP_FLOATS].copy_from_slice(&[0.0, 0.0, 0.0, -1.0, 0.0]);
                        continue;
                    }
                    let src = i * STOP_CHANNELS;
                    let position = if self.position_wired[i] {
                        self.values[src + 3]
                    } else {
                        i as f32 / (self.stops - 1) as f32
                    };
                    let alpha = if self.alpha_wired[i] { self.values[src + 4] } else { 1.0 };
                    self.out[dst] = self.values[src].clamp(0.0, 1.0);
                    self.out[dst + 1] = self.values[src + 1].clamp(0.0, 1.0);
                    self.out[dst + 2] = self.values[src + 2].clamp(0.0, 1.0);
                    self.out[dst + 3] = alpha.clamp(0.0, 1.0);
                    self.out[dst + 4] = position.clamp(0.0, 1.0);
                }
            }
        }
    }

    fn read_output(&self, channel: usize) -> f32 {
        self.out.get(channel).copied().unwrap_or(0.0)
    }

    fn non_promotable_params(&self) -> &'static [&'static str] {
        &["Stops"]
    }

    fn params(&self) -> Vec<ParamDef> {
        if !self.kind.has_stops() {
            return Vec::new();
        }
        vec![ParamDef::Int {
            name: "Stops".into(),
            value: self.stops as i64,
            min: MIN_STOPS as i64,
            max: MAX_STOPS as i64,
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 && self.kind.has_stops() {
            let stops = clamp_stops(value.as_i64().max(0) as usize);
            if stops != self.stops {
                self.stops = stops;
                (self.inputs, self.outputs) = ports(self.kind, stops);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{Gradient, GradientStop, Rgb};

    fn node(kind: ReshapeKind, stops: usize) -> ColorReshapeProcessNode {
        let mut n = ColorReshapeProcessNode::new(NodeId(1), kind);
        if kind.has_stops() {
            n.set_param(0, ParamValue::Int(stops as i64));
        }
        n
    }

    fn write(n: &mut ColorReshapeProcessNode, values: &[f32]) {
        for (i, v) in values.iter().enumerate() {
            n.write_input(i, *v);
        }
        n.process();
    }

    fn outputs(n: &ColorReshapeProcessNode, count: usize) -> Vec<f32> {
        (0..count).map(|i| n.read_output(i)).collect()
    }

    #[test]
    fn a_palette_survives_the_round_trip_through_colors() {
        let palette: Vec<f32> = (0..12).map(|i| i as f32 / 12.0).collect();
        let mut split = node(ReshapeKind::PaletteToColors, 0);
        write(&mut split, &palette);
        let colors = outputs(&split, 12);
        let mut merge = node(ReshapeKind::ColorsToPalette, 0);
        write(&mut merge, &colors);
        assert_eq!(outputs(&merge, 12), palette);
        assert_eq!(ports(ReshapeKind::PaletteToColors, 0).1.len(), 4);
    }

    #[test]
    fn splitting_a_gradient_lists_its_stops_in_order_of_position() {
        let gradient = Gradient::new(vec![
            GradientStop { position: 1.0, color: Rgb::new(0.0, 0.0, 1.0), alpha: 0.5 },
            GradientStop { position: 0.0, color: Rgb::new(1.0, 0.0, 0.0), alpha: 1.0 },
        ]);
        let mut n = node(ReshapeKind::GradientToColors, 3);
        write(&mut n, &gradient.to_channels());
        let out = outputs(&n, 15);
        assert_eq!(&out[0..5], &[1.0, 0.0, 0.0, 0.0, 1.0], "first stop: red at 0, opaque");
        assert_eq!(&out[5..10], &[0.0, 0.0, 1.0, 1.0, 0.5], "second stop: blue at 1, half alpha");
        assert_eq!(&out[10..15], &[0.0; 5], "no third stop");
    }

    #[test]
    fn colors_to_gradient_spreads_unwired_positions_and_marks_extra_stops_unused() {
        let mut n = node(ReshapeKind::ColorsToGradient, 3);
        n.set_input_connections(&[false; 9]);
        let mut inputs = vec![0.0; 15];
        inputs[0] = 1.0; // stop 1: red
        inputs[5 + 2] = 1.0; // stop 2: blue
        write(&mut n, &inputs);
        let g = Gradient::from_channels(&outputs(&n, 40));
        let stops = g.stops();
        assert_eq!(stops.len(), 3, "only the three exposed stops are used");
        assert_eq!((stops[0].color.r, stops[1].color.b), (1.0, 1.0));
        assert_eq!(stops.iter().map(|s| s.position).collect::<Vec<_>>(), vec![0.0, 0.5, 1.0]);
        assert!(stops.iter().all(|s| s.alpha == 1.0));
    }

    #[test]
    fn wired_positions_and_alpha_are_used_and_clamped() {
        let mut n = node(ReshapeKind::ColorsToGradient, 2);
        // Wire position and alpha of both stops (ports: color, position, alpha per stop).
        n.set_input_connections(&[false, true, true, false, true, true]);
        write(&mut n, &[0.0, 0.0, 0.0, 0.25, 0.5, 0.0, 0.0, 0.0, 3.0, 0.8]);
        let g = Gradient::from_channels(&outputs(&n, 40));
        let stops = g.stops();
        assert_eq!((stops[0].position, stops[0].alpha), (0.25, 0.5));
        assert_eq!((stops[1].position, stops[1].alpha), (1.0, 0.8), "position clamped to 1");
    }

    #[test]
    fn the_number_of_stops_decides_the_ports() {
        let n = node(ReshapeKind::GradientToColors, 6);
        assert_eq!(n.outputs().len(), 18);
        let n = node(ReshapeKind::ColorsToGradient, 2);
        assert_eq!(n.inputs().len(), 6);
        let n = node(ReshapeKind::ColorsToGradient, 99);
        assert_eq!(n.inputs().len(), MAX_STOPS * 3, "clamped to the eight a gradient holds");
    }
}
