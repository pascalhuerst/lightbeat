//! DMX Input: channels of a DMX input from the setup (a lighting desk sending
//! Art-Net) as outputs. Each entry reads one or more channels from a universe
//! and turns them into a value, a gate, a trigger, a colour or a position.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::dmx_input::DmxInputHub;
use crate::engine::nodes::io::status::{level, with_status_port};
use crate::engine::types::*;

/// What the channels of an entry mean.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DmxKind {
    /// One channel as 0..1.
    #[default]
    Value,
    /// Two channels (coarse and fine) as 0..1.
    Value16,
    /// One channel, on above the threshold.
    Gate,
    /// One channel; a pulse each time it goes above the threshold.
    Trigger,
    /// Red, green and blue.
    Color,
    /// Red, green and blue, each with a fine channel.
    Color16,
    /// Pan and tilt, each with a fine channel.
    Position16,
}

impl DmxKind {
    pub const ALL: [Self; 7] =
        [Self::Value, Self::Value16, Self::Gate, Self::Trigger, Self::Color, Self::Color16, Self::Position16];

    pub fn label(self) -> &'static str {
        match self {
            DmxKind::Value => "Value (8 bit)",
            DmxKind::Value16 => "Value (16 bit)",
            DmxKind::Gate => "Gate",
            DmxKind::Trigger => "Trigger",
            DmxKind::Color => "Color (RGB, 8 bit)",
            DmxKind::Color16 => "Color (RGB, 16 bit)",
            DmxKind::Position16 => "Position (pan/tilt, 16 bit)",
        }
    }

    /// Components: what the channels are made of (coarse and fine count as one).
    fn components(self) -> usize {
        match self {
            DmxKind::Color | DmxKind::Color16 => 3,
            DmxKind::Position16 => 2,
            _ => 1,
        }
    }

    pub fn has_fine(self) -> bool {
        matches!(self, DmxKind::Value16 | DmxKind::Color16 | DmxKind::Position16)
    }

    pub fn has_threshold(self) -> bool {
        matches!(self, DmxKind::Gate | DmxKind::Trigger)
    }

    /// How many DMX channels it uses.
    pub fn footprint(self) -> usize {
        self.components() * if self.has_fine() { 2 } else { 1 }
    }

    pub fn port_type(self) -> PortType {
        match self {
            DmxKind::Gate | DmxKind::Trigger => PortType::Logic,
            DmxKind::Color | DmxKind::Color16 => PortType::Color,
            DmxKind::Position16 => PortType::Position,
            DmxKind::Value | DmxKind::Value16 => PortType::Untyped,
        }
    }
}

/// Where the fine channel of a 16-bit value sits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FineLayout {
    /// Each coarse channel is followed by its fine one: R, R fine, G, G fine, ...
    #[default]
    Pairs,
    /// All coarse channels, then all fine ones: R, G, B, R fine, G fine, ...
    Block,
}

impl FineLayout {
    pub fn label(self) -> &'static str {
        match self {
            FineLayout::Pairs => "coarse, fine, coarse, fine",
            FineLayout::Block => "all coarse, then all fine",
        }
    }
}

fn half() -> f32 {
    0.5
}

fn first_address() -> u16 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DmxEntry {
    /// Stays with the entry when others are added or removed.
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: DmxKind,
    /// The universe (15-bit port address) of the input.
    #[serde(default)]
    pub universe: u16,
    /// First DMX channel, 1 to 512.
    #[serde(default = "first_address")]
    pub address: u16,
    /// For a gate or trigger: the level above which it is on, 0..1.
    #[serde(default = "half")]
    pub threshold: f32,
    #[serde(default)]
    pub layout: FineLayout,
}

impl DmxEntry {
    pub fn new(id: u32, universe: u16) -> Self {
        Self {
            id,
            name: String::new(),
            kind: DmxKind::Value,
            universe,
            address: 1,
            threshold: 0.5,
            layout: FineLayout::Pairs,
        }
    }

    pub fn port_name(&self, index: usize) -> String {
        if self.name.is_empty() { format!("ch {}", index + 1) } else { self.name.clone() }
    }

    /// The channels it uses, zero based. Past the end of the universe it is cut short.
    pub fn channels(&self) -> Range<usize> {
        let first = usize::from(self.address.clamp(1, 512)) - 1;
        first..(first + self.kind.footprint()).min(512)
    }

    /// Whether all its channels fit in the universe.
    pub fn fits(&self) -> bool {
        self.channels().len() == self.kind.footprint()
    }

    /// Turn the raw channel values (`footprint` of them) into the output
    /// channels of its port. Gate and trigger are decided by the node.
    fn decode(&self, raw: &[u8], out: &mut [f32]) {
        let byte = |i: usize| f32::from(raw.get(i).copied().unwrap_or(0));
        let components = self.kind.components();
        for (k, slot) in out.iter_mut().enumerate().take(components) {
            *slot = if self.kind.has_fine() {
                let (coarse, fine) = match self.layout {
                    FineLayout::Pairs => (2 * k, 2 * k + 1),
                    FineLayout::Block => (k, components + k),
                };
                (byte(coarse) * 256.0 + byte(fine)) / 65535.0
            } else {
                byte(k) / 255.0
            };
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DmxInputNodeConfig {
    /// The DMX input of the setup; 0 for none.
    #[serde(default)]
    pub input: u32,
    #[serde(default)]
    pub entries: Vec<DmxEntry>,
}

/// The node's output ports: the status port, then one per entry.
pub fn output_ports(entries: &[DmxEntry]) -> Vec<PortDef> {
    with_status_port(entries.iter().enumerate().map(|(i, e)| PortDef::new(e.port_name(i), e.kind.port_type())).collect())
}

pub struct DmxInputProcessNode {
    id: NodeId,
    hub: DmxInputHub,
    config: DmxInputNodeConfig,
    outputs: Vec<PortDef>,
    values: Vec<f32>,
    /// What to read from the hub: each entry's universe, first channel and count.
    requests: Vec<(u16, usize, usize)>,
    raw: Vec<u8>,
    /// Per entry: was it above its threshold last tick.
    was_on: Vec<bool>,
    /// The first live tick sets `was_on`, so a trigger already held is not a press.
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
            was_on: Vec::new(),
            baselined: false,
        };
        n.rebuild();
        n
    }

    fn rebuild(&mut self) {
        self.outputs = output_ports(&self.config.entries);
        self.values = vec![0.0; total_channels(&self.outputs)];
        self.requests = self
            .config
            .entries
            .iter()
            .map(|e| {
                let c = e.channels();
                (e.universe, c.start, c.len())
            })
            .collect();
        self.was_on = vec![false; self.config.entries.len()];
        self.baselined = false;
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

        let mut at = 0;
        let mut base = 1;
        for (i, entry) in self.config.entries.iter().enumerate() {
            // An entry at the end of the universe may have fewer channels than its kind needs.
            let have = self.requests[i].2;
            let raw = &self.raw[at..at + have];
            at += have;
            let width = entry.kind.port_type().channel_count();
            match entry.kind {
                DmxKind::Gate | DmxKind::Trigger => {
                    let on = raw.first().is_some_and(|v| f32::from(*v) / 255.0 >= entry.threshold.max(1e-6));
                    let pulse = on && !self.was_on[i] && self.baselined;
                    self.values[base] = match entry.kind {
                        DmxKind::Gate => f32::from(u8::from(on)),
                        _ => f32::from(u8::from(pulse)),
                    };
                    self.was_on[i] = on;
                }
                _ => entry.decode(raw, &mut self.values[base..base + width]),
            }
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

    fn setup(entries: serde_json::Value) -> (DmxInputProcessNode, DmxInputHub) {
        let hub = DmxInputHub::idle();
        let mut input = DmxInputConfig::new_art_net(1, "Desk");
        input.kind = DmxInputKind::ArtNet { universes: vec![0, 1], interface: String::new() };
        hub.set_inputs(&[input]);
        let mut n = DmxInputProcessNode::new(NodeId(1), hub.clone());
        n.load_data(&serde_json::json!({ "input": 1, "entries": entries }));
        (n, hub)
    }

    fn out(n: &DmxInputProcessNode) -> Vec<f32> {
        (0..total_channels(n.outputs())).map(|i| n.read_output(i)).collect()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn ports_are_the_status_port_and_one_per_entry_typed_by_kind() {
        let (n, _) = setup(serde_json::json!([
            { "id": 1, "name": "Dim", "kind": "value" },
            { "id": 2, "kind": "color16", "address": 10 },
            { "id": 3, "kind": "gate", "address": 20 },
            { "id": 4, "kind": "position16", "address": 30 },
        ]));
        let ports: Vec<(String, PortType)> = n.outputs().iter().map(|p| (p.name.clone(), p.port_type)).collect();
        assert_eq!(
            ports,
            vec![
                ("connected".into(), PortType::Logic),
                ("Dim".into(), PortType::Untyped),
                ("ch 2".into(), PortType::Color),
                ("ch 3".into(), PortType::Logic),
                ("ch 4".into(), PortType::Position),
            ]
        );
        assert_eq!(total_channels(n.outputs()), 1 + 1 + 3 + 1 + 2);
    }

    #[test]
    fn eight_bit_values_colors_and_gates_follow_the_channels() {
        let (mut n, hub) = setup(serde_json::json!([
            { "id": 1, "kind": "value", "address": 1 },
            { "id": 2, "kind": "color", "address": 2 },
            { "id": 3, "kind": "gate", "address": 5 },
            { "id": 4, "kind": "gate", "address": 6, "threshold": 0.9 },
        ]));
        hub.apply_dmx(0, &[255, 255, 128, 0, 130, 200], DESK);
        n.process();
        let o = out(&n);
        assert_eq!(o[0], 1.0, "live");
        assert!(close(o[1], 1.0));
        assert!(close(o[2], 1.0) && close(o[3], 128.0 / 255.0) && close(o[4], 0.0));
        assert_eq!(o[5], 1.0, "130 is above half");
        assert_eq!(o[6], 0.0, "200 is below 0.9");
    }

    #[test]
    fn sixteen_bit_values_combine_coarse_and_fine_in_either_layout() {
        let (mut n, hub) = setup(serde_json::json!([
            { "id": 1, "kind": "value16", "address": 1 },
            { "id": 2, "kind": "color16", "address": 3, "layout": "pairs" },
            { "id": 3, "kind": "color16", "address": 9, "layout": "block" },
            { "id": 4, "kind": "position16", "address": 15 },
        ]));
        let mut frame = vec![0u8; 20];
        frame[0..2].copy_from_slice(&[0x80, 0x00]);
        // Pairs: R, Rf, G, Gf, B, Bf.
        frame[2..8].copy_from_slice(&[0xFF, 0xFF, 0x00, 0x00, 0x80, 0x00]);
        // Block: R, G, B, Rf, Gf, Bf.
        frame[8..14].copy_from_slice(&[0xFF, 0x00, 0x80, 0xFF, 0x00, 0x00]);
        // Position pairs: pan, pan fine, tilt, tilt fine.
        frame[14..18].copy_from_slice(&[0x00, 0x00, 0xFF, 0xFF]);
        hub.apply_dmx(0, &frame, DESK);
        n.process();
        let o = out(&n);
        assert!(close(o[1], 32768.0 / 65535.0), "value16: {}", o[1]);
        assert!(close(o[2], 1.0) && close(o[3], 0.0) && close(o[4], 32768.0 / 65535.0), "pairs: {:?}", &o[2..5]);
        assert!(close(o[5], 1.0) && close(o[6], 0.0) && close(o[7], 32768.0 / 65535.0), "block: {:?}", &o[5..8]);
        assert!(close(o[8], 0.0) && close(o[9], 1.0), "position: {:?}", &o[8..10]);
    }

    #[test]
    fn a_trigger_pulses_once_when_it_goes_on_and_not_for_a_state_it_started_in() {
        let (mut n, hub) = setup(serde_json::json!([{ "id": 1, "kind": "trigger", "address": 1 }]));
        hub.apply_dmx(0, &[255], DESK);
        n.process();
        assert_eq!(n.read_output(1), 0.0, "already on when the node came alive");
        hub.apply_dmx(0, &[0], DESK);
        n.process();
        hub.apply_dmx(0, &[255], DESK);
        n.process();
        assert_eq!(n.read_output(1), 1.0, "pressed");
        n.process();
        assert_eq!(n.read_output(1), 0.0, "one tick");
    }

    #[test]
    fn entries_read_their_own_universe_and_hold_when_the_desk_goes_quiet() {
        let (mut n, hub) = setup(serde_json::json!([
            { "id": 1, "kind": "value", "universe": 1, "address": 3 },
            { "id": 2, "kind": "value", "universe": 0, "address": 3 },
        ]));
        hub.apply_dmx(1, &[0, 0, 51], DESK);
        hub.apply_dmx(0, &[0, 0, 255], DESK);
        n.process();
        assert!(close(n.read_output(1), 0.2) && close(n.read_output(2), 1.0));
        // Nothing listens to universe 7: it reads as zero.
        n.load_data(&serde_json::json!({ "input": 1, "entries": [{ "id": 1, "kind": "value", "universe": 7 }] }));
        n.process();
        assert_eq!(n.read_output(1), 0.0);
    }

    #[test]
    fn an_entry_that_runs_off_the_end_of_the_universe_is_cut_and_flagged() {
        let mut e = DmxEntry::new(1, 0);
        e.kind = DmxKind::Color16;
        e.address = 510;
        assert_eq!(e.channels(), 509..512);
        assert!(!e.fits());
        e.address = 507;
        assert!(e.fits());
        let (mut n, hub) = setup(serde_json::json!([{ "id": 1, "kind": "color16", "address": 510 }]));
        hub.apply_dmx(0, &[7u8; 512], DESK);
        n.process();
        assert_eq!(total_channels(n.outputs()), 4, "it does not panic and still has its three colour channels");
    }

    #[test]
    fn no_input_means_no_signal_and_the_config_round_trips() {
        let (mut n, _) = setup(serde_json::json!([{ "id": 1, "name": "A", "kind": "gate", "address": 4, "threshold": 0.25 }]));
        n.load_data(&serde_json::json!({ "input": 0, "entries": [{ "id": 1 }] }));
        n.process();
        assert_eq!(n.read_output(0), 0.0);
        n.load_data(&serde_json::json!({ "input": 1, "entries": [{ "id": 1, "name": "A", "kind": "gate", "address": 4, "threshold": 0.25 }] }));
        let saved = n.save_data().unwrap();
        assert_eq!((saved["input"].as_u64(), saved["entries"][0]["address"].as_u64()), (Some(1), Some(4)));
        assert_eq!(saved["entries"][0]["kind"], "gate");
    }
}
