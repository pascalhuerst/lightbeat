//! MIDI Input: the controls of one MIDI device as outputs, each learned from
//! the device itself. The device and the list of controls are the node's own
//! and are saved with the project.

use serde::{Deserialize, Serialize};

use crate::engine::nodes::io::status::{level, with_status_port};
use crate::engine::types::*;
use crate::midi_io::{Binding, Control, MidiHub, MsgKind};

/// How an entry turns a control into a signal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InMode {
    /// A fader, knob or pitch wheel: 0..1.
    #[default]
    Value,
    /// An endless encoder: each click moves the value a little.
    Relative,
    /// A button held down.
    Gate,
    /// A pad: how hard it was hit, 0 when released.
    Velocity,
    /// A pulse each time a button goes down.
    TriggerPress,
    /// A pulse each time a button comes up.
    TriggerRelease,
}

impl InMode {
    pub fn label(self) -> &'static str {
        match self {
            InMode::Value => "Value",
            InMode::Relative => "Relative encoder",
            InMode::Gate => "Gate (held)",
            InMode::Velocity => "Velocity",
            InMode::TriggerPress => "Trigger on press",
            InMode::TriggerRelease => "Trigger on release",
        }
    }

    pub fn port_type(self) -> PortType {
        match self {
            InMode::Gate | InMode::TriggerPress | InMode::TriggerRelease => PortType::Logic,
            _ => PortType::Untyped,
        }
    }

    /// The modes that make sense for a kind of message.
    pub fn modes_for(kind: MsgKind) -> &'static [InMode] {
        match kind {
            MsgKind::Cc => &[InMode::Value, InMode::Relative],
            MsgKind::Note => &[InMode::Gate, InMode::Velocity, InMode::TriggerPress, InMode::TriggerRelease],
            MsgKind::PitchBend => &[InMode::Value],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InEntry {
    /// Stays with the entry when others are added or removed.
    pub id: u32,
    #[serde(default)]
    pub name: String,
    /// What it listens to; None until it has been learned.
    #[serde(default)]
    pub binding: Option<Binding>,
    #[serde(default)]
    pub mode: InMode,
}

impl InEntry {
    pub fn port_name(&self, index: usize) -> String {
        if self.name.is_empty() { format!("out {}", index + 1) } else { self.name.clone() }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MidiInputConfig {
    /// The device, by the name it was saved under; empty for none.
    #[serde(default)]
    pub port: String,
    #[serde(default)]
    pub entries: Vec<InEntry>,
}

/// The node's output ports for `entries`: the status port, then one each.
pub fn output_ports(entries: &[InEntry]) -> Vec<PortDef> {
    with_status_port(
        entries.iter().enumerate().map(|(i, e)| PortDef::new(e.port_name(i), e.mode.port_type())).collect(),
    )
}

/// How far one click of a relative encoder moves the value: about 24 turn it end to end.
const RELATIVE_STEP: f32 = 1.0 / 24.0;

#[derive(Clone, Copy, Default)]
struct EntryState {
    accumulated: f32,
    last_rel: i64,
    last_on: u64,
    last_off: u64,
}

pub struct MidiInputProcessNode {
    id: NodeId,
    hub: MidiHub,
    config: MidiInputConfig,
    outputs: Vec<PortDef>,
    values: Vec<f32>,
    states: Vec<EntryState>,
    bindings: Vec<Option<Binding>>,
    reads: Vec<Option<Control>>,
    /// The counters have been read once since the device appeared, so a
    /// press is a change from there and not from nothing.
    baselined: bool,
}

impl MidiInputProcessNode {
    pub fn new(id: NodeId, hub: MidiHub) -> Self {
        let mut n = Self {
            id,
            hub,
            config: MidiInputConfig::default(),
            outputs: Vec::new(),
            values: Vec::new(),
            states: Vec::new(),
            bindings: Vec::new(),
            reads: Vec::new(),
            baselined: false,
        };
        n.rebuild();
        n
    }

    fn rebuild(&mut self) {
        self.outputs = output_ports(&self.config.entries);
        self.values = vec![0.0; self.outputs.len()];
        self.states = vec![EntryState::default(); self.config.entries.len()];
        self.bindings = self.config.entries.iter().map(|e| e.binding).collect();
        self.baselined = false;
    }
}

impl Drop for MidiInputProcessNode {
    fn drop(&mut self) {
        self.hub.set_input(self.id.0, "");
    }
}

impl ProcessNode for MidiInputProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "MIDI Input" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn process(&mut self) {
        let connected = self.hub.read_controls(&self.config.port, &self.bindings, &mut self.reads);
        self.values[0] = level(connected);
        if !connected {
            self.baselined = false;
            self.values[1..].fill(0.0);
            return;
        }
        if !self.baselined {
            for (state, read) in self.states.iter_mut().zip(&self.reads) {
                let c = read.unwrap_or_default();
                state.last_rel = c.rel_sum;
                state.last_on = c.on_count;
                state.last_off = c.off_count;
            }
            self.baselined = true;
        }
        for (i, entry) in self.config.entries.iter().enumerate() {
            let c = self.reads[i].unwrap_or_default();
            let state = &mut self.states[i];
            let top = if entry.binding.is_some_and(|b| b.kind == MsgKind::PitchBend) { 16383.0 } else { 127.0 };
            self.values[i + 1] = match entry.mode {
                InMode::Value => f32::from(c.raw) / top,
                InMode::Velocity => f32::from(c.raw) / 127.0,
                InMode::Gate => f32::from(u8::from(c.raw > 0)),
                InMode::Relative => {
                    let delta = c.rel_sum - state.last_rel;
                    state.last_rel = c.rel_sum;
                    state.accumulated = (state.accumulated + delta as f32 * RELATIVE_STEP).clamp(0.0, 1.0);
                    state.accumulated
                }
                InMode::TriggerPress => {
                    let pulse = c.on_count != state.last_on;
                    state.last_on = c.on_count;
                    f32::from(u8::from(pulse))
                }
                InMode::TriggerRelease => {
                    let pulse = c.off_count != state.last_off;
                    state.last_off = c.off_count;
                    f32::from(u8::from(pulse))
                }
            };
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        self.values.get(pi).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.config).ok()
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Ok(config) = serde_json::from_value::<MidiInputConfig>(data.clone()) {
            // An encoder keeps what it counted while the list is edited.
            let before: Vec<(u32, EntryState)> =
                self.config.entries.iter().map(|e| e.id).zip(self.states.iter().copied()).collect();
            self.config = config;
            self.rebuild();
            for (state, entry) in self.states.iter_mut().zip(&self.config.entries) {
                if let Some((_, old)) = before.iter().find(|(id, _)| *id == entry.id) {
                    state.accumulated = old.accumulated;
                }
            }
            self.hub.set_input(self.id.0, &self.config.port);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi_io::Msg;

    const PORT: &str = "Controller";

    fn node(entries: serde_json::Value) -> (MidiInputProcessNode, MidiHub) {
        let hub = MidiHub::idle();
        let mut n = MidiInputProcessNode::new(NodeId(1), hub.clone());
        n.load_data(&serde_json::json!({ "port": PORT, "entries": entries }));
        (n, hub)
    }

    fn entry(id: u32, kind: &str, number: u8, mode: &str) -> serde_json::Value {
        serde_json::json!({ "id": id, "name": format!("e{id}"), "binding": {"kind": kind, "channel": 1, "number": number}, "mode": mode })
    }

    fn outputs(n: &MidiInputProcessNode) -> Vec<f32> {
        (0..n.outputs().len()).map(|i| n.read_output(i)).collect()
    }

    #[test]
    fn ports_are_the_status_port_then_one_per_entry_typed_by_mode() {
        let (n, _) = node(serde_json::json!([entry(1, "cc", 7, "value"), entry(2, "note", 60, "gate"), { "id": 3 }]));
        let ports: Vec<(String, PortType)> = n.outputs().iter().map(|p| (p.name.clone(), p.port_type)).collect();
        assert_eq!(
            ports,
            vec![
                ("connected".to_string(), PortType::Logic),
                ("e1".to_string(), PortType::Untyped),
                ("e2".to_string(), PortType::Logic),
                ("out 3".to_string(), PortType::Untyped),
            ]
        );
    }

    #[test]
    fn nothing_is_connected_until_the_device_is_there_and_then_values_follow_the_controls() {
        let (mut n, hub) = node(serde_json::json!([entry(1, "cc", 7, "value"), entry(2, "cc", 8, "value")]));
        n.process();
        assert_eq!(outputs(&n), vec![0.0, 0.0, 0.0], "no device");
        hub.inject(PORT, Msg::Cc { channel: 1, cc: 7, value: 127 });
        hub.inject(PORT, Msg::Cc { channel: 1, cc: 8, value: 64 });
        n.process();
        let out = outputs(&n);
        assert_eq!(out[0], 1.0);
        assert_eq!(out[1], 1.0);
        assert!((out[2] - 64.0 / 127.0).abs() < 1e-6);
    }

    #[test]
    fn a_pitch_wheel_reads_over_its_fourteen_bits() {
        let (mut n, hub) = node(serde_json::json!([entry(1, "pitch_bend", 0, "value")]));
        hub.inject(PORT, Msg::Bend { channel: 1, value: 16383 });
        n.process();
        assert_eq!(n.read_output(1), 1.0);
        hub.inject(PORT, Msg::Bend { channel: 1, value: 8192 });
        n.process();
        assert!((n.read_output(1) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn buttons_hold_pulse_or_follow_velocity() {
        let (mut n, hub) = node(serde_json::json!([
            entry(1, "note", 60, "gate"),
            entry(2, "note", 60, "velocity"),
            entry(3, "note", 60, "trigger_press"),
            entry(4, "note", 60, "trigger_release"),
        ]));
        hub.inject(PORT, Msg::Cc { channel: 1, cc: 1, value: 0 });
        n.process();
        assert_eq!(&outputs(&n)[1..], &[0.0, 0.0, 0.0, 0.0]);

        hub.inject(PORT, Msg::NoteOn { channel: 1, note: 60, velocity: 127 });
        n.process();
        assert_eq!(&outputs(&n)[1..], &[1.0, 1.0, 1.0, 0.0], "pressed: held, full velocity, press pulse");
        n.process();
        assert_eq!(&outputs(&n)[1..], &[1.0, 1.0, 0.0, 0.0], "the pulse lasts one tick");

        hub.inject(PORT, Msg::NoteOff { channel: 1, note: 60 });
        n.process();
        assert_eq!(&outputs(&n)[1..], &[0.0, 0.0, 0.0, 1.0], "released: release pulse");
    }

    #[test]
    fn the_very_first_press_is_not_mistaken_for_the_state_the_node_started_in() {
        let (mut n, hub) = node(serde_json::json!([entry(1, "note", 36, "trigger_press")]));
        hub.inject(PORT, Msg::Cc { channel: 1, cc: 1, value: 0 });
        n.process();
        hub.inject(PORT, Msg::NoteOn { channel: 1, note: 36, velocity: 100 });
        n.process();
        assert_eq!(n.read_output(1), 1.0);
    }

    #[test]
    fn a_relative_encoder_adds_up_its_clicks_between_ticks() {
        let (mut n, hub) = node(serde_json::json!([entry(1, "cc", 16, "relative")]));
        hub.inject(PORT, Msg::Cc { channel: 1, cc: 1, value: 0 });
        n.process();
        // Twelve clicks up, in several messages, between two ticks.
        for _ in 0..3 {
            hub.inject(PORT, Msg::Cc { channel: 1, cc: 16, value: 4 });
        }
        n.process();
        assert!((n.read_output(1) - 0.5).abs() < 1e-6);
        hub.inject(PORT, Msg::Cc { channel: 1, cc: 16, value: 127 });
        n.process();
        assert!((n.read_output(1) - (0.5 - 1.0 / 24.0)).abs() < 1e-6, "one click down");
        for _ in 0..40 {
            hub.inject(PORT, Msg::Cc { channel: 1, cc: 16, value: 5 });
        }
        n.process();
        assert_eq!(n.read_output(1), 1.0, "stops at the end");
    }

    #[test]
    fn editing_the_list_keeps_an_encoders_position_and_the_config_round_trips() {
        let (mut n, hub) = node(serde_json::json!([entry(1, "cc", 16, "relative")]));
        hub.inject(PORT, Msg::Cc { channel: 1, cc: 1, value: 0 });
        n.process();
        hub.inject(PORT, Msg::Cc { channel: 1, cc: 16, value: 12 });
        n.process();
        let saved = n.save_data().unwrap();
        n.load_data(&serde_json::json!({ "port": PORT, "entries": [entry(2, "cc", 9, "value"), entry(1, "cc", 16, "relative")] }));
        n.process();
        assert_eq!(n.outputs().len(), 3);
        assert!((n.read_output(2) - 0.5).abs() < 1e-6, "entry 1 moved to the second place and kept its value");
        assert_eq!(saved["port"], PORT);
        assert_eq!(saved["entries"][0]["binding"]["kind"], "cc");
    }
}
