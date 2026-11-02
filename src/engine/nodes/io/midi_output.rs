//! MIDI Output: sends its inputs to a MIDI device as CC, note or pitch bend
//! messages, whatever is wired to them: a fader on a motorised controller,
//! the lights on a pad grid. The device and the list are the node's own and
//! are saved with the project.

use serde::{Deserialize, Serialize};

use crate::engine::nodes::io::status::{level, status_port};
use crate::engine::types::*;
use crate::midi_io::{Binding, MidiHub, MsgKind};

/// How an input turns into a message.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutMode {
    /// 0..1 across the whole range of a CC or the pitch wheel.
    #[default]
    Value,
    /// A note held while the input is high.
    Gate,
    /// A note whose velocity is the input; 0 is off. For pads that show it as a colour.
    Velocity,
}

impl OutMode {
    pub fn label(self) -> &'static str {
        match self {
            OutMode::Value => "Value",
            OutMode::Gate => "Gate (on / off)",
            OutMode::Velocity => "Velocity",
        }
    }

    pub fn port_type(self) -> PortType {
        match self {
            OutMode::Gate => PortType::Logic,
            _ => PortType::Untyped,
        }
    }

    pub fn modes_for(kind: MsgKind) -> &'static [OutMode] {
        match kind {
            MsgKind::Note => &[OutMode::Gate, OutMode::Velocity],
            _ => &[OutMode::Value],
        }
    }

    pub fn default_for(kind: MsgKind) -> Self {
        Self::modes_for(kind)[0]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutEntry {
    /// Stays with the entry when others are added or removed.
    pub id: u32,
    #[serde(default)]
    pub name: String,
    pub binding: Binding,
    #[serde(default)]
    pub mode: OutMode,
}

impl OutEntry {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            name: String::new(),
            binding: Binding { kind: MsgKind::Cc, channel: 1, number: 0 },
            mode: OutMode::Value,
        }
    }

    pub fn port_name(&self, index: usize) -> String {
        if self.name.is_empty() { format!("in {}", index + 1) } else { self.name.clone() }
    }

    /// The raw value to send for `value`.
    fn raw(&self, value: f32) -> u16 {
        let v = value.clamp(0.0, 1.0);
        match (self.binding.kind, self.mode) {
            (MsgKind::PitchBend, _) => (v * 16383.0).round() as u16,
            (MsgKind::Note, OutMode::Gate) => if value >= 0.5 { 127 } else { 0 },
            _ => (v * 127.0).round() as u16,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MidiOutputConfig {
    /// The device, by the name it was saved under; empty for none.
    #[serde(default)]
    pub port: String,
    #[serde(default)]
    pub entries: Vec<OutEntry>,
}

/// The node's input ports for `entries`.
pub fn input_ports(entries: &[OutEntry]) -> Vec<PortDef> {
    entries.iter().enumerate().map(|(i, e)| PortDef::new(e.port_name(i), e.mode.port_type())).collect()
}

pub struct MidiOutputProcessNode {
    id: NodeId,
    hub: MidiHub,
    config: MidiOutputConfig,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
    values: Vec<f32>,
    wired: Vec<bool>,
    /// What was last sent for each entry, to send only changes.
    sent: Vec<Option<u16>>,
    generation: u64,
    connected: bool,
}

impl MidiOutputProcessNode {
    pub fn new(id: NodeId, hub: MidiHub) -> Self {
        let mut n = Self {
            id,
            hub,
            config: MidiOutputConfig::default(),
            inputs: Vec::new(),
            outputs: vec![status_port()],
            values: Vec::new(),
            wired: Vec::new(),
            sent: Vec::new(),
            generation: 0,
            connected: false,
        };
        n.rebuild();
        n
    }

    fn rebuild(&mut self) {
        self.inputs = input_ports(&self.config.entries);
        self.values.resize(self.inputs.len(), 0.0);
        self.wired.resize(self.inputs.len(), false);
        self.sent = vec![None; self.inputs.len()];
    }
}

impl Drop for MidiOutputProcessNode {
    fn drop(&mut self) {
        self.hub.set_output(self.id.0, "");
    }
}

impl ProcessNode for MidiOutputProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "MIDI Output" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if let Some(slot) = self.values.get_mut(pi) {
            *slot = v;
        }
    }

    fn read_input(&self, pi: usize) -> f32 {
        self.values.get(pi).copied().unwrap_or(0.0)
    }

    fn set_input_connections(&mut self, connected: &[bool]) {
        for (w, c) in self.wired.iter_mut().zip(connected) {
            *w = *c;
        }
    }

    fn process(&mut self) {
        let (connected, generation) = self.hub.output_state(&self.config.port);
        self.connected = connected;
        if !connected {
            self.sent.fill(None);
            return;
        }
        // A device that has just appeared knows nothing: send it everything.
        if generation != self.generation {
            self.generation = generation;
            self.sent.fill(None);
        }
        for (i, entry) in self.config.entries.iter().enumerate() {
            // Only what is wired; an unconnected input would otherwise send
            // zeros and flatten the device's own controls.
            if !self.wired.get(i).copied().unwrap_or(false) {
                continue;
            }
            let raw = entry.raw(self.values[i]);
            if self.sent[i] != Some(raw) {
                self.sent[i] = Some(raw);
                self.hub.send(&self.config.port, entry.binding, raw);
            }
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { level(self.connected) } else { 0.0 }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.config).ok()
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Ok(config) = serde_json::from_value::<MidiOutputConfig>(data.clone()) {
            self.config = config;
            self.rebuild();
            self.hub.set_output(self.id.0, &self.config.port);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORT: &str = "Controller";

    fn entry(id: u32, kind: &str, number: u8, mode: &str) -> serde_json::Value {
        serde_json::json!({ "id": id, "binding": {"kind": kind, "channel": 1, "number": number}, "mode": mode })
    }

    fn node(entries: serde_json::Value, wired: &[bool]) -> (MidiOutputProcessNode, MidiHub) {
        let hub = MidiHub::idle();
        let mut n = MidiOutputProcessNode::new(NodeId(1), hub.clone());
        n.load_data(&serde_json::json!({ "port": PORT, "entries": entries }));
        n.set_input_connections(wired);
        (n, hub)
    }

    #[test]
    fn inputs_are_typed_by_mode_and_the_only_output_is_the_status() {
        let (n, _) = node(serde_json::json!([entry(1, "cc", 7, "value"), entry(2, "note", 60, "gate")]), &[]);
        assert_eq!(n.inputs().iter().map(|p| p.port_type).collect::<Vec<_>>(), vec![PortType::Untyped, PortType::Logic]);
        assert_eq!(n.outputs().iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["connected"]);
        assert_eq!(n.inputs()[0].name, "in 1");
    }

    #[test]
    fn a_changed_value_is_sent_as_the_right_message_once() {
        let (mut n, hub) = node(
            serde_json::json!([entry(1, "cc", 7, "value"), entry(2, "note", 60, "gate"), entry(3, "pitch_bend", 0, "value"), entry(4, "note", 61, "velocity")]),
            &[true, true, true, true],
        );
        n.process();
        assert_eq!(n.read_output(0), 0.0, "no device yet");
        hub.fake_output(PORT);
        for (i, v) in [0.5, 1.0, 1.0, 0.25].into_iter().enumerate() {
            n.write_input(i, v);
        }
        n.process();
        assert_eq!(n.read_output(0), 1.0);
        assert_eq!(hub.sent(PORT), vec![[0xB0, 7, 64], [0x90, 60, 127], [0xE0, 0x7F, 0x7F], [0x90, 61, 32]]);
        n.process();
        assert_eq!(hub.sent(PORT).len(), 4, "nothing changed, nothing sent");
        n.write_input(1, 0.0);
        n.process();
        assert_eq!(hub.sent(PORT).last(), Some(&[0x90, 60, 0]), "gate low is a note off");
    }

    #[test]
    fn unwired_inputs_send_nothing() {
        let (mut n, hub) = node(serde_json::json!([entry(1, "cc", 7, "value"), entry(2, "cc", 8, "value")]), &[false, true]);
        hub.fake_output(PORT);
        n.process();
        assert_eq!(hub.sent(PORT), vec![[0xB0, 8, 0]]);
    }

    #[test]
    fn a_device_that_appears_again_is_sent_everything_again() {
        let (mut n, hub) = node(serde_json::json!([entry(1, "cc", 7, "value")]), &[true]);
        hub.fake_output(PORT);
        n.write_input(0, 1.0);
        n.process();
        assert_eq!(hub.sent(PORT).len(), 1);
        hub.fake_output(PORT);
        n.process();
        assert_eq!(hub.sent(PORT).len(), 2, "a new connection starts from nothing");
    }

    #[test]
    fn jitter_below_the_midi_resolution_is_not_sent() {
        let (mut n, hub) = node(serde_json::json!([entry(1, "cc", 7, "value")]), &[true]);
        hub.fake_output(PORT);
        n.write_input(0, 0.5000);
        n.process();
        n.write_input(0, 0.5013);
        n.process();
        assert_eq!(hub.sent(PORT).len(), 1);
    }
}
