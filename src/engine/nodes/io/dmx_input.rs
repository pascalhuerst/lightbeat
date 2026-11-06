//! DMX Input: what a lighting desk sends us over Art-Net, as outputs.
//!
//! The node has a list of *outputs*. Each has a kind (a value, a colour, a
//! gate, ...) and, for each thing the kind is made of, a *source*: a channel
//! of a universe of the DMX input, optionally with a fine channel for 16 bits,
//! inverted and rescaled. So a colour can be red, green and blue from any
//! three channels, in any universe, or hue, saturation and value; a gate is
//! one channel with an "on above" and an "off below" level.

use serde::{Deserialize, Serialize};

use crate::color::{ColorConvert, Hsv};
use crate::dmx_input::DmxInputHub;
use crate::engine::nodes::io::status::{level, with_status_port};
use crate::engine::types::*;

/// What an output makes of its sources.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DmxKind {
    /// One source as 0..1.
    #[default]
    Value,
    /// One source cut into equal ranges: the number of the range it is in.
    Index,
    /// High when the source goes above a level, low when it comes below another.
    Gate,
    /// A pulse each time the source goes above the level.
    Trigger,
    /// Red, green and blue, or hue, saturation and value, with an optional dimmer.
    Color,
    /// Pan and tilt.
    Position,
}

impl DmxKind {
    pub const ALL: [Self; 6] = [Self::Value, Self::Color, Self::Position, Self::Gate, Self::Trigger, Self::Index];

    pub fn label(self) -> &'static str {
        match self {
            DmxKind::Value => "Value",
            DmxKind::Index => "Selector (ranges to a number)",
            DmxKind::Gate => "Gate (high above a level, low below another)",
            DmxKind::Trigger => "Trigger (pulse when it goes above a level)",
            DmxKind::Color => "Color",
            DmxKind::Position => "Position (pan, tilt)",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            DmxKind::Value => "Value",
            DmxKind::Index => "Selector",
            DmxKind::Gate => "Gate",
            DmxKind::Trigger => "Trigger",
            DmxKind::Color => "Color",
            DmxKind::Position => "Position",
        }
    }

    pub fn port_type(self) -> PortType {
        match self {
            DmxKind::Gate | DmxKind::Trigger => PortType::Logic,
            DmxKind::Color => PortType::Color,
            DmxKind::Position => PortType::Position,
            DmxKind::Value | DmxKind::Index => PortType::Untyped,
        }
    }

    /// Sources it always needs; a colour may have a dimmer on top.
    fn base_sources(self) -> usize {
        match self {
            DmxKind::Color => 3,
            DmxKind::Position => 2,
            _ => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorModel {
    #[default]
    Rgb,
    Hsv,
}

fn first_address() -> u16 {
    1
}

fn one() -> f32 {
    1.0
}

/// One thing read from the desk: a channel, or a coarse and a fine channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// The universe (15-bit port address) of the input.
    #[serde(default)]
    pub universe: u16,
    /// DMX channel, 1 to 512.
    #[serde(default = "first_address")]
    pub address: u16,
    /// The fine channel, in the same universe, for 16 bits.
    #[serde(default)]
    pub fine: Option<u16>,
    #[serde(default)]
    pub invert: bool,
    /// The part of the range that counts: this level is 0 and `high` is 1.
    #[serde(default)]
    pub low: f32,
    #[serde(default = "one")]
    pub high: f32,
}

impl Source {
    pub fn new(universe: u16, address: u16) -> Self {
        Self { universe, address, fine: None, invert: false, low: 0.0, high: 1.0 }
    }

    /// The channel, zero based, kept inside the universe.
    pub fn coarse_index(&self) -> usize {
        usize::from(self.address.clamp(1, 512)) - 1
    }

    pub fn fine_index(&self) -> Option<usize> {
        self.fine.map(|f| usize::from(f.clamp(1, 512)) - 1)
    }

    /// 0..1 from the raw channel values.
    pub fn value(&self, coarse: u8, fine: Option<u8>) -> f32 {
        let raw = match fine {
            Some(f) => (f32::from(coarse) * 256.0 + f32::from(f)) / 65535.0,
            None => f32::from(coarse) / 255.0,
        };
        let scaled = if self.high > self.low { ((raw - self.low) / (self.high - self.low)).clamp(0.0, 1.0) } else { raw };
        if self.invert { 1.0 - scaled } else { scaled }
    }
}

fn half() -> f32 {
    0.5
}

fn a_bit_less() -> f32 {
    0.4
}

fn four() -> u32 {
    4
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DmxOutput {
    /// Stays with the output when others are added or removed.
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: DmxKind,
    /// What it is made of, in the order of `source_labels`.
    #[serde(default)]
    pub sources: Vec<Source>,
    /// Gate and trigger: on above this level, 0..1.
    #[serde(default = "half")]
    pub on_above: f32,
    /// Gate and trigger: off again below this level, 0..1.
    #[serde(default = "a_bit_less")]
    pub off_below: f32,
    /// Selector: how many ranges.
    #[serde(default = "four")]
    pub bins: u32,
    #[serde(default)]
    pub model: ColorModel,
    /// Color: a source that dims it.
    #[serde(default)]
    pub dimmer: bool,
}

impl DmxOutput {
    /// A new output of `kind` whose sources start at `first_address` in `universe`.
    pub fn new(id: u32, kind: DmxKind, universe: u16, first_address: u16) -> Self {
        let mut o = Self {
            id,
            name: String::new(),
            kind,
            sources: Vec::new(),
            on_above: 0.5,
            off_below: 0.4,
            bins: 4,
            model: ColorModel::Rgb,
            dimmer: false,
        };
        o.sources = (0..o.source_count()).map(|i| Source::new(universe, (first_address + i as u16).min(512))).collect();
        o
    }

    pub fn port_name(&self, index: usize) -> String {
        if self.name.is_empty() { format!("{} {}", self.kind.short().to_lowercase(), index + 1) } else { self.name.clone() }
    }

    pub fn source_count(&self) -> usize {
        self.kind.base_sources() + usize::from(self.kind == DmxKind::Color && self.dimmer)
    }

    /// What each source is, for the editor.
    pub fn source_labels(&self) -> Vec<&'static str> {
        let mut labels = match (self.kind, self.model) {
            (DmxKind::Color, ColorModel::Rgb) => vec!["Red", "Green", "Blue"],
            (DmxKind::Color, ColorModel::Hsv) => vec!["Hue", "Saturation", "Value"],
            (DmxKind::Position, _) => vec!["Pan", "Tilt"],
            (DmxKind::Index, _) => vec!["Selector"],
            (DmxKind::Gate | DmxKind::Trigger, _) => vec!["Level"],
            (DmxKind::Value, _) => vec!["Value"],
        };
        if self.kind == DmxKind::Color && self.dimmer {
            labels.push("Dimmer");
        }
        labels
    }

    /// Give it exactly the sources its kind needs after the kind, the model or
    /// the dimmer changed, keeping those it had and placing new ones after the last.
    pub fn fit_sources(&mut self) {
        let want = self.source_count();
        self.sources.truncate(want);
        while self.sources.len() < want {
            let next = self.sources.last().map_or(Source::new(0, 1), |s| Source::new(s.universe, (s.address + 1).min(512)));
            self.sources.push(next);
        }
    }

    pub fn bins(&self) -> u32 {
        self.bins.clamp(1, 256)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DmxInputNodeConfig {
    /// The DMX input of the setup; 0 for none.
    #[serde(default)]
    pub input: u32,
    #[serde(default)]
    pub outputs: Vec<DmxOutput>,
}

/// The node's output ports: the status port, then one per output.
pub fn output_ports(outputs: &[DmxOutput]) -> Vec<PortDef> {
    with_status_port(outputs.iter().enumerate().map(|(i, o)| PortDef::new(o.port_name(i), o.kind.port_type())).collect())
}

/// Per output, the state that outlasts a tick.
#[derive(Clone, Copy, Default)]
struct OutputState {
    on: bool,
}

pub struct DmxInputProcessNode {
    id: NodeId,
    hub: DmxInputHub,
    config: DmxInputNodeConfig,
    outputs: Vec<PortDef>,
    values: Vec<f32>,
    /// One channel per request: each source's coarse channel, then its fine one.
    requests: Vec<(u16, usize, usize)>,
    raw: Vec<u8>,
    states: Vec<OutputState>,
    /// The first live tick sets the gates, so one already high is not a press.
    baselined: bool,
}

impl DmxInputProcessNode {
    pub fn new(id: NodeId, hub: DmxInputHub) -> Self {
        let mut n = Self {
            id,
            hub,
            config: DmxInputNodeConfig::default(),
            outputs: Vec::new(),
            values: Vec::new(),
            requests: Vec::new(),
            raw: Vec::new(),
            states: Vec::new(),
            baselined: false,
        };
        n.rebuild();
        n
    }

    fn rebuild(&mut self) {
        for o in &mut self.config.outputs {
            o.fit_sources();
        }
        self.outputs = output_ports(&self.config.outputs);
        self.values = vec![0.0; total_channels(&self.outputs)];
        self.requests = self
            .config
            .outputs
            .iter()
            .flat_map(|o| &o.sources)
            .flat_map(|s| {
                std::iter::once((s.universe, s.coarse_index(), 1)).chain(s.fine_index().map(|f| (s.universe, f, 1)))
            })
            .collect();
        self.states = vec![OutputState::default(); self.config.outputs.len()];
        self.baselined = false;
    }
}

/// What an output makes of its sources' values (each 0..1), into `out`.
fn evaluate(o: &DmxOutput, v: &[f32], state: &mut OutputState, baselined: bool, out: &mut [f32]) {
    match o.kind {
        DmxKind::Value => out[0] = v[0],
        DmxKind::Index => {
            let n = o.bins() as f32;
            out[0] = (v[0] * n).floor().min(n - 1.0);
        }
        DmxKind::Gate | DmxKind::Trigger => {
            let was = state.on;
            if v[0] >= o.on_above {
                state.on = true;
            } else if v[0] < o.off_below.min(o.on_above) {
                state.on = false;
            }
            out[0] = match o.kind {
                DmxKind::Gate => f32::from(u8::from(state.on)),
                _ => f32::from(u8::from(state.on && !was && baselined)),
            };
        }
        DmxKind::Color => {
            let (r, g, b) = match o.model {
                ColorModel::Rgb => (v[0], v[1], v[2]),
                ColorModel::Hsv => {
                    let c = Hsv::new(v[0], v[1], v[2]).to_rgb();
                    (c.r, c.g, c.b)
                }
            };
            let dim = if o.dimmer { v[3] } else { 1.0 };
            out[..3].copy_from_slice(&[r * dim, g * dim, b * dim]);
        }
        DmxKind::Position => out[..2].copy_from_slice(&v[..2]),
    }
}

impl ProcessNode for DmxInputProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "DMX Input" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn process(&mut self) {
        let live = self.hub.input_live(self.config.input);
        self.values[0] = level(live);
        // Values hold when the desk goes quiet, like a node's outputs do.
        if self.config.input == 0 {
            return;
        }
        self.hub.read_channels(&self.requests, &mut self.raw);

        let (mut at, mut base) = (0, 1);
        let mut sources = [0.0f32; 4];
        for (i, o) in self.config.outputs.iter().enumerate() {
            for (k, s) in o.sources.iter().enumerate() {
                let coarse = self.raw.get(at).copied().unwrap_or(0);
                at += 1;
                let fine = s.fine.map(|_| {
                    at += 1;
                    self.raw.get(at - 1).copied().unwrap_or(0)
                });
                sources[k.min(3)] = s.value(coarse, fine);
            }
            let width = o.kind.port_type().channel_count();
            evaluate(o, &sources, &mut self.states[i], self.baselined, &mut self.values[base..base + width]);
            base += width;
        }
        self.baselined = live;
    }

    fn read_output(&self, pi: usize) -> f32 {
        self.values.get(pi).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.config).ok()
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Ok(config) = serde_json::from_value::<DmxInputNodeConfig>(data.clone()) {
            self.config = config;
            self.rebuild();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmx_input::{DmxInputConfig, DmxInputKind};
    use std::net::Ipv4Addr;

    const DESK: Ipv4Addr = Ipv4Addr::new(2, 0, 0, 7);

    fn setup(outputs: serde_json::Value) -> (DmxInputProcessNode, DmxInputHub) {
        let hub = DmxInputHub::idle();
        let mut input = DmxInputConfig::new_art_net(1, "Desk");
        input.kind = DmxInputKind::ArtNet { universes: vec![0, 1], interface: String::new() };
        hub.set_inputs(&[input]);
        let mut n = DmxInputProcessNode::new(NodeId(1), hub.clone());
        n.load_data(&serde_json::json!({ "input": 1, "outputs": outputs }));
        (n, hub)
    }

    fn out(n: &DmxInputProcessNode) -> Vec<f32> {
        (0..total_channels(n.outputs())).map(|i| n.read_output(i)).collect()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn src(universe: u16, address: u16) -> serde_json::Value {
        serde_json::json!({ "universe": universe, "address": address })
    }

    #[test]
    fn ports_are_the_status_port_and_one_per_output_typed_by_kind() {
        let (n, _) = setup(serde_json::json!([
            { "id": 1, "name": "Dim", "kind": "value", "sources": [src(0, 1)] },
            { "id": 2, "kind": "color", "sources": [src(0, 2), src(0, 3), src(0, 4)] },
            { "id": 3, "kind": "gate", "sources": [src(0, 5)] },
            { "id": 4, "kind": "position", "sources": [src(0, 6), src(0, 7)] },
            { "id": 5, "kind": "trigger", "sources": [src(0, 8)] },
            { "id": 6, "kind": "index", "sources": [src(0, 9)] },
        ]));
        let ports: Vec<(String, PortType)> = n.outputs().iter().map(|p| (p.name.clone(), p.port_type)).collect();
        assert_eq!(
            ports,
            vec![
                ("connected".into(), PortType::Logic),
                ("Dim".into(), PortType::Untyped),
                ("color 2".into(), PortType::Color),
                ("gate 3".into(), PortType::Logic),
                ("position 4".into(), PortType::Position),
                ("trigger 5".into(), PortType::Logic),
                ("selector 6".into(), PortType::Untyped),
            ]
        );
        assert_eq!(total_channels(n.outputs()), 1 + 1 + 3 + 1 + 2 + 1 + 1);
    }

    #[test]
    fn a_color_takes_its_three_values_from_any_three_channels() {
        let (mut n, hub) = setup(serde_json::json!([
            { "id": 1, "kind": "color", "sources": [src(1, 40), src(0, 2), src(0, 300)] },
        ]));
        let mut u0 = vec![0u8; 512];
        u0[1] = 128;
        u0[299] = 51;
        let mut u1 = vec![0u8; 512];
        u1[39] = 255;
        hub.apply_dmx(0, &u0, DESK);
        hub.apply_dmx(1, &u1, DESK);
        n.process();
        let o = out(&n);
        assert!(close(o[1], 1.0) && close(o[2], 128.0 / 255.0) && close(o[3], 0.2), "{o:?}");
    }

    #[test]
    fn a_source_can_be_sixteen_bit_inverted_and_rescaled() {
        let (mut n, hub) = setup(serde_json::json!([
            { "id": 1, "kind": "value", "sources": [{ "address": 1, "fine": 2 }] },
            { "id": 2, "kind": "value", "sources": [{ "address": 3, "fine": 10 }] },
            { "id": 3, "kind": "value", "sources": [{ "address": 4, "invert": true }] },
            { "id": 4, "kind": "value", "sources": [{ "address": 5, "low": 0.5, "high": 1.0 }] },
        ]));
        let mut frame = vec![0u8; 20];
        frame[0..2].copy_from_slice(&[0x80, 0x00]);
        // The fine channel of the second one is far away.
        frame[2] = 0xFF;
        frame[9] = 0xFF;
        frame[3] = 255;
        frame[4] = 191;
        hub.apply_dmx(0, &frame, DESK);
        n.process();
        let o = out(&n);
        assert!(close(o[1], 32768.0 / 65535.0), "coarse and the next channel: {}", o[1]);
        assert!(close(o[2], 1.0), "coarse and a fine channel elsewhere: {}", o[2]);
        assert!(close(o[3], 0.0), "inverted full: {}", o[3]);
        assert!(close(o[4], (191.0 / 255.0 - 0.5) / 0.5), "the upper half stretched to the whole: {}", o[4]);
    }

    #[test]
    fn a_color_can_be_hue_saturation_value_with_a_dimmer() {
        let (mut n, hub) = setup(serde_json::json!([
            { "id": 1, "kind": "color", "model": "hsv", "dimmer": true, "sources": [src(0, 1), src(0, 2), src(0, 3), src(0, 4)] },
        ]));
        // Hue 0 is red; full saturation and value; half dimmed.
        hub.apply_dmx(0, &[0, 255, 255, 128], DESK);
        n.process();
        let o = out(&n);
        let dim = 128.0 / 255.0;
        assert!(close(o[1], dim) && close(o[2], 0.0) && close(o[3], 0.0), "{o:?}");
        assert_eq!(n.outputs().len(), 2);
    }

    #[test]
    fn a_gate_goes_high_above_one_level_and_low_below_another() {
        let (mut n, hub) = setup(serde_json::json!([
            { "id": 1, "kind": "gate", "on_above": 0.6, "off_below": 0.2, "sources": [src(0, 1)] },
        ]));
        let at = |n: &mut DmxInputProcessNode, v: u8| {
            hub.apply_dmx(0, &[v], DESK);
            n.process();
            n.read_output(1)
        };
        assert_eq!(at(&mut n, 0), 0.0);
        assert_eq!(at(&mut n, 100), 0.0, "0.39 is below the on level");
        assert_eq!(at(&mut n, 160), 1.0, "0.63 is above it");
        assert_eq!(at(&mut n, 100), 1.0, "between the two levels it stays on");
        assert_eq!(at(&mut n, 40), 0.0, "0.16 is below the off level");
        assert_eq!(at(&mut n, 100), 0.0, "and stays off between them");
    }

    #[test]
    fn a_trigger_pulses_once_per_crossing_and_not_for_a_state_it_started_in() {
        let (mut n, hub) = setup(serde_json::json!([
            { "id": 1, "kind": "trigger", "on_above": 0.5, "off_below": 0.3, "sources": [src(0, 1)] },
        ]));
        let at = |n: &mut DmxInputProcessNode, v: u8| {
            hub.apply_dmx(0, &[v], DESK);
            n.process();
            n.read_output(1)
        };
        assert_eq!(at(&mut n, 255), 0.0, "already high when the node came alive");
        assert_eq!(at(&mut n, 255), 0.0);
        at(&mut n, 0);
        assert_eq!(at(&mut n, 255), 1.0, "went above");
        assert_eq!(at(&mut n, 255), 0.0, "one tick");
        at(&mut n, 100);
        assert_eq!(at(&mut n, 255), 0.0, "it never came below the off level, so no new crossing");
        at(&mut n, 10);
        assert_eq!(at(&mut n, 255), 1.0, "and now it did");
    }

    #[test]
    fn a_selector_numbers_the_range_a_value_is_in() {
        let (mut n, hub) = setup(serde_json::json!([
            { "id": 1, "kind": "index", "bins": 5, "sources": [src(0, 1)] },
        ]));
        let at = |n: &mut DmxInputProcessNode, v: u8| {
            hub.apply_dmx(0, &[v], DESK);
            n.process();
            n.read_output(1)
        };
        assert_eq!([at(&mut n, 0), at(&mut n, 50), at(&mut n, 51), at(&mut n, 130), at(&mut n, 255)], [0.0, 0.0, 1.0, 2.0, 4.0]);
    }

    #[test]
    fn a_position_reads_pan_and_tilt() {
        let (mut n, hub) = setup(serde_json::json!([
            { "id": 1, "kind": "position", "sources": [{ "address": 1, "fine": 2 }, src(0, 3)] },
        ]));
        hub.apply_dmx(0, &[0xFF, 0xFF, 0], DESK);
        n.process();
        let o = out(&n);
        assert!(close(o[1], 1.0) && close(o[2], 0.0), "{o:?}");
    }

    #[test]
    fn changing_the_kind_keeps_the_sources_it_had_and_places_new_ones_after_them() {
        let mut o = DmxOutput::new(1, DmxKind::Value, 3, 10);
        assert_eq!(o.sources, vec![Source::new(3, 10)]);
        o.kind = DmxKind::Color;
        o.fit_sources();
        assert_eq!(o.sources.iter().map(|s| (s.universe, s.address)).collect::<Vec<_>>(), vec![(3, 10), (3, 11), (3, 12)]);
        o.dimmer = true;
        o.fit_sources();
        assert_eq!(o.sources.len(), 4);
        assert_eq!(o.source_labels(), vec!["Red", "Green", "Blue", "Dimmer"]);
        o.kind = DmxKind::Gate;
        o.fit_sources();
        assert_eq!(o.sources.len(), 1);
        let fresh = DmxOutput::new(2, DmxKind::Position, 0, 511);
        assert_eq!(fresh.sources.iter().map(|s| s.address).collect::<Vec<_>>(), vec![511, 512]);
    }

    #[test]
    fn no_input_means_no_signal_values_hold_and_the_config_round_trips() {
        let (mut n, hub) = setup(serde_json::json!([{ "id": 1, "kind": "value", "sources": [src(0, 1)] }]));
        hub.apply_dmx(0, &[255], DESK);
        n.process();
        assert_eq!(n.read_output(1), 1.0);
        n.load_data(&serde_json::json!({ "input": 0, "outputs": [{ "id": 1 }] }));
        n.process();
        assert_eq!(n.read_output(0), 0.0, "not connected");
        let saved = n.save_data().unwrap();
        assert_eq!(saved["outputs"][0]["sources"].as_array().map(Vec::len), Some(1), "an output always has its sources");
        assert_eq!(saved["outputs"][0]["kind"], "value");
    }
}
