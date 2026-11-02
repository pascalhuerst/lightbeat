//! MIDI devices for the MIDI Input and MIDI Output nodes.
//!
//! The nodes name a device and the messages they care about; this hub does
//! the rest. A worker thread opens the devices the nodes ask for, keeps the
//! latest state of every control that comes in (so a node can read it at any
//! rate without missing a press), and sends what the output nodes queue.
//!
//! Devices are looked up on start-up, when a node's device changes and when
//! the user presses Rescan, never on a timer: enumerating MIDI ports is slow
//! and noisy. A device that is not there is simply not connected until then.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use serde::{Deserialize, Serialize};

use crate::input_controller::midi::resolve_port;

/// What kind of message a control sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MsgKind {
    Cc,
    Note,
    PitchBend,
}

impl MsgKind {
    pub const ALL: [Self; 3] = [Self::Cc, Self::Note, Self::PitchBend];

    pub fn label(self) -> &'static str {
        match self {
            MsgKind::Cc => "CC",
            MsgKind::Note => "Note",
            MsgKind::PitchBend => "Pitch bend",
        }
    }
}

/// One control on a device: a CC number, a note, or the pitch wheel of a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Binding {
    pub kind: MsgKind,
    /// 1..=16.
    pub channel: u8,
    /// CC or note number; 0 for the pitch wheel.
    pub number: u8,
}

impl Binding {
    pub fn label(&self) -> String {
        match self.kind {
            MsgKind::Cc => format!("CC ch{} #{}", self.channel, self.number),
            MsgKind::Note => format!("Note ch{} #{}", self.channel, self.number),
            MsgKind::PitchBend => format!("Pitch ch{}", self.channel),
        }
    }
}

/// A decoded incoming message.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Msg {
    Cc { channel: u8, cc: u8, value: u8 },
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    Bend { channel: u8, value: u16 },
}

impl Msg {
    /// The control the message belongs to.
    pub fn binding(&self) -> Binding {
        match *self {
            Msg::Cc { channel, cc, .. } => Binding { kind: MsgKind::Cc, channel, number: cc },
            Msg::NoteOn { channel, note, .. } | Msg::NoteOff { channel, note } => {
                Binding { kind: MsgKind::Note, channel, number: note }
            }
            Msg::Bend { channel, .. } => Binding { kind: MsgKind::PitchBend, channel, number: 0 },
        }
    }
}

pub fn parse(bytes: &[u8]) -> Option<Msg> {
    let status = *bytes.first()?;
    let channel = (status & 0x0F) + 1;
    let d1 = bytes.get(1).copied().unwrap_or(0);
    let d2 = bytes.get(2).copied().unwrap_or(0);
    match status & 0xF0 {
        0x80 => Some(Msg::NoteOff { channel, note: d1 }),
        0x90 if d2 == 0 => Some(Msg::NoteOff { channel, note: d1 }),
        0x90 => Some(Msg::NoteOn { channel, note: d1, velocity: d2 }),
        0xB0 => Some(Msg::Cc { channel, cc: d1, value: d2 }),
        0xE0 => Some(Msg::Bend { channel, value: u16::from(d1 & 0x7F) | (u16::from(d2 & 0x7F) << 7) }),
        _ => None,
    }
}

/// The bytes that send `msg` to a device.
pub fn encode(binding: Binding, raw: u16) -> [u8; 3] {
    let ch = binding.channel.clamp(1, 16) - 1;
    match binding.kind {
        MsgKind::Cc => [0xB0 | ch, binding.number & 0x7F, (raw as u8).min(127)],
        // A note off is a note on with no velocity: every device takes it, and
        // pad controllers read the velocity as a colour.
        MsgKind::Note => [0x90 | ch, binding.number & 0x7F, (raw as u8).min(127)],
        MsgKind::PitchBend => [0xE0 | ch, (raw & 0x7F) as u8, ((raw >> 7) & 0x7F) as u8],
    }
}

/// A relative encoder's step: 1..=63 turns up, 65..=127 turns down.
pub fn relative_delta(value: u8) -> i64 {
    match value {
        0 => 0,
        v @ 1..=63 => i64::from(v),
        v => i64::from(v) - 128,
    }
}

/// What a control did last, kept so a reader never misses a press between two looks.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Control {
    /// CC value, note velocity (0 when released) or the 14-bit pitch wheel.
    pub raw: u16,
    pub on_count: u64,
    pub off_count: u64,
    /// Everything a CC has turned, as a relative encoder reads it.
    pub rel_sum: i64,
}

#[derive(Default)]
pub struct InPort {
    pub connected: bool,
    controls: HashMap<Binding, Control>,
    /// The latest messages, numbered, for learning.
    recent: VecDeque<(u64, Msg)>,
    seq: u64,
}

impl InPort {
    fn apply(&mut self, msg: Msg) {
        let c = self.controls.entry(msg.binding()).or_default();
        match msg {
            Msg::Cc { value, .. } => {
                c.raw = u16::from(value);
                c.rel_sum += relative_delta(value);
            }
            Msg::NoteOn { velocity, .. } => {
                c.raw = u16::from(velocity);
                c.on_count += 1;
            }
            Msg::NoteOff { .. } => {
                c.raw = 0;
                c.off_count += 1;
            }
            Msg::Bend { value, .. } => c.raw = value,
        }
        self.seq += 1;
        self.recent.push_back((self.seq, msg));
        while self.recent.len() > 64 {
            self.recent.pop_front();
        }
    }
}

#[derive(Default)]
pub struct OutPort {
    pub connected: bool,
    /// Counts connections, so a node knows to send everything again to a
    /// device that has just appeared.
    pub generation: u64,
    queue: VecDeque<[u8; 3]>,
}

#[derive(Default)]
struct State {
    /// Node id to the device it wants, by the name it saved.
    wanted_in: HashMap<u64, String>,
    wanted_out: HashMap<u64, String>,
    in_ports: HashMap<String, InPort>,
    out_ports: HashMap<String, OutPort>,
    available_in: Vec<String>,
    available_out: Vec<String>,
    /// The device lists are out of date: enumerate again.
    rescan: bool,
    scanned: bool,
    first_scan_asked: bool,
    /// The set of wanted devices changed: open and close connections.
    reconcile: bool,
}

struct Inner {
    state: Mutex<State>,
    wake: Condvar,
}

#[derive(Clone)]
pub struct MidiHub(Arc<Inner>);

impl MidiHub {
    /// A hub with its worker running.
    pub fn start() -> Self {
        let hub = Self::idle();
        let inner = hub.0.clone();
        thread::Builder::new()
            .name("lightbeat-midi".into())
            .spawn(move || run(inner))
            .expect("spawn the MIDI worker");
        hub
    }

    /// A hub without a worker, for tests: nothing is ever opened.
    pub fn idle() -> Self {
        Self(Arc::new(Inner { state: Mutex::new(State::default()), wake: Condvar::new() }))
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.0.state.lock().unwrap()
    }

    /// Make sure the device lists have been filled at least once, for a
    /// dropdown that is about to show them.
    pub fn ensure_scanned(&self) {
        let mut s = self.lock();
        if !s.scanned && !s.first_scan_asked {
            s.first_scan_asked = true;
            s.rescan = true;
            drop(s);
            self.0.wake.notify_all();
        }
    }

    /// Look for devices again, as the Rescan button does.
    pub fn rescan(&self) {
        let mut s = self.lock();
        s.rescan = true;
        s.reconcile = true;
        drop(s);
        self.0.wake.notify_all();
    }

    pub fn available_inputs(&self) -> Vec<String> {
        self.lock().available_in.clone()
    }

    pub fn available_outputs(&self) -> Vec<String> {
        self.lock().available_out.clone()
    }

    /// `node` listens to `port` ("" = none).
    pub fn set_input(&self, node: u64, port: &str) {
        let mut s = self.lock();
        if port.is_empty() {
            s.wanted_in.remove(&node);
        } else if s.wanted_in.get(&node).map(String::as_str) != Some(port) {
            s.wanted_in.insert(node, port.to_string());
        } else {
            return;
        }
        s.reconcile = true;
        drop(s);
        self.0.wake.notify_all();
    }

    /// `node` sends to `port` ("" = none).
    pub fn set_output(&self, node: u64, port: &str) {
        let mut s = self.lock();
        if port.is_empty() {
            s.wanted_out.remove(&node);
        } else if s.wanted_out.get(&node).map(String::as_str) != Some(port) {
            s.wanted_out.insert(node, port.to_string());
        } else {
            return;
        }
        s.reconcile = true;
        drop(s);
        self.0.wake.notify_all();
    }

    /// Devices some node wants that are not connected, by name.
    pub fn missing_devices(&self) -> Vec<String> {
        let s = self.lock();
        let mut missing: Vec<String> = s
            .wanted_in
            .values()
            .filter(|n| !s.in_ports.get(*n).is_some_and(|p| p.connected))
            .chain(s.wanted_out.values().filter(|n| !s.out_ports.get(*n).is_some_and(|p| p.connected)))
            .cloned()
            .collect();
        missing.sort();
        missing.dedup();
        missing
    }

    /// Whether the device `port` is open for listening.
    pub fn input_connected(&self, port: &str) -> bool {
        self.lock().in_ports.get(port).is_some_and(|p| p.connected)
    }

    /// Fill `out` with the state of each binding on `port` and say whether the
    /// device is connected.
    pub fn read_controls(&self, port: &str, bindings: &[Option<Binding>], out: &mut Vec<Option<Control>>) -> bool {
        out.clear();
        let s = self.lock();
        let Some(p) = s.in_ports.get(port) else {
            out.resize(bindings.len(), None);
            return false;
        };
        out.extend(bindings.iter().map(|b| b.and_then(|b| p.controls.get(&b).copied())));
        p.connected
    }

    /// The messages that arrived on `port` after number `since`, and the
    /// number of the latest one (to ask from next time).
    pub fn messages_since(&self, port: &str, since: u64) -> (Vec<Msg>, u64) {
        let s = self.lock();
        let Some(p) = s.in_ports.get(port) else { return (Vec::new(), since) };
        (p.recent.iter().filter(|(n, _)| *n > since).map(|(_, m)| *m).collect(), p.seq)
    }

    /// The number of the latest message on `port`, to start learning from.
    pub fn latest_seq(&self, port: &str) -> u64 {
        self.lock().in_ports.get(port).map_or(0, |p| p.seq)
    }

    pub fn output_connected(&self, port: &str) -> bool {
        self.lock().out_ports.get(port).is_some_and(|p| p.connected)
    }

    /// Whether the output device is open, and how many times it has been
    /// opened (a node sends everything again when that changes).
    pub fn output_state(&self, port: &str) -> (bool, u64) {
        self.lock().out_ports.get(port).map_or((false, 0), |p| (p.connected, p.generation))
    }

    /// Queue a message for the device. Dropped while it is not connected.
    pub fn send(&self, port: &str, binding: Binding, raw: u16) {
        let mut s = self.lock();
        let Some(p) = s.out_ports.get_mut(port) else { return };
        if !p.connected {
            return;
        }
        p.queue.push_back(encode(binding, raw));
        while p.queue.len() > 4096 {
            p.queue.pop_front();
        }
        drop(s);
        self.0.wake.notify_all();
    }

    #[cfg(test)]
    pub fn inject(&self, port: &str, msg: Msg) {
        let mut s = self.lock();
        let p = s.in_ports.entry(port.to_string()).or_default();
        p.connected = true;
        p.apply(msg);
    }

    #[cfg(test)]
    pub fn fake_output(&self, port: &str) {
        let mut s = self.lock();
        let p = s.out_ports.entry(port.to_string()).or_default();
        p.connected = true;
        p.generation += 1;
    }

    #[cfg(test)]
    pub fn sent(&self, port: &str) -> Vec<[u8; 3]> {
        self.lock().out_ports.get(port).map(|p| p.queue.iter().copied().collect()).unwrap_or_default()
    }
}

/// The worker: opens what is wanted and sends what is queued.
fn run(inner: Arc<Inner>) {
    let mut in_conns: HashMap<String, MidiInputConnection<()>> = HashMap::new();
    let mut out_conns: HashMap<String, MidiOutputConnection> = HashMap::new();
    loop {
        let (scan, reconcile) = {
            let mut s = inner.state.lock().unwrap();
            // Nothing is looked up before something wants a device or the
            // user asks, so a project without MIDI never touches the ports.
            let wants = !s.wanted_in.is_empty() || !s.wanted_out.is_empty();
            let scan = s.rescan || (!s.scanned && wants);
            s.rescan = false;
            (scan, std::mem::take(&mut s.reconcile))
        };
        if scan {
            let (ins, outs) = enumerate();
            let mut s = inner.state.lock().unwrap();
            s.available_in = ins;
            s.available_out = outs;
            s.scanned = true;
        }
        if scan || reconcile {
            reconcile_connections(&inner, &mut in_conns, &mut out_conns);
        }

        // Send what the output nodes queued.
        let pending: Vec<(String, Vec<[u8; 3]>)> = {
            let mut s = inner.state.lock().unwrap();
            s.out_ports
                .iter_mut()
                .filter(|(_, p)| p.connected && !p.queue.is_empty())
                .map(|(name, p)| (name.clone(), p.queue.drain(..).collect()))
                .collect()
        };
        for (name, messages) in pending {
            if let Some(conn) = out_conns.get_mut(&name) {
                for m in messages {
                    if conn.send(&m).is_err() {
                        break;
                    }
                }
            }
        }

        let s = inner.state.lock().unwrap();
        if s.rescan || s.reconcile || s.out_ports.values().any(|p| !p.queue.is_empty()) {
            continue;
        }
        let _ = inner.wake.wait_timeout(s, Duration::from_millis(250));
    }
}

fn enumerate() -> (Vec<String>, Vec<String>) {
    let ins = MidiInput::new("LightBeat-midi-scan")
        .map(|i| i.ports().iter().filter_map(|p| i.port_name(p).ok()).collect())
        .unwrap_or_default();
    let outs = MidiOutput::new("LightBeat-midi-scan")
        .map(|o| o.ports().iter().filter_map(|p| o.port_name(p).ok()).collect())
        .unwrap_or_default();
    (ins, outs)
}

/// Open the devices that are wanted and available, close the ones that are not wanted.
fn reconcile_connections(
    inner: &Arc<Inner>,
    in_conns: &mut HashMap<String, MidiInputConnection<()>>,
    out_conns: &mut HashMap<String, MidiOutputConnection>,
) {
    let (want_in, want_out, avail_in, avail_out) = {
        let s = inner.state.lock().unwrap();
        let mut wi: Vec<String> = s.wanted_in.values().cloned().collect();
        let mut wo: Vec<String> = s.wanted_out.values().cloned().collect();
        wi.sort();
        wi.dedup();
        wo.sort();
        wo.dedup();
        (wi, wo, s.available_in.clone(), s.available_out.clone())
    };

    in_conns.retain(|name, _| want_in.contains(name));
    out_conns.retain(|name, _| want_out.contains(name));

    for name in &want_in {
        if !in_conns.contains_key(name)
            && let Some(resolved) = resolve_port(name, &avail_in)
            && let Some(conn) = open_input(inner, name, &resolved)
        {
            in_conns.insert(name.clone(), conn);
        }
    }
    for name in &want_out {
        if !out_conns.contains_key(name)
            && let Some(resolved) = resolve_port(name, &avail_out)
            && let Some(conn) = open_output(&resolved)
        {
            out_conns.insert(name.clone(), conn);
            let mut s = inner.state.lock().unwrap();
            let p = s.out_ports.entry(name.clone()).or_default();
            p.generation += 1;
        }
    }

    let mut s = inner.state.lock().unwrap();
    s.in_ports.retain(|name, _| want_in.contains(name));
    for name in &want_in {
        s.in_ports.entry(name.clone()).or_default().connected = in_conns.contains_key(name);
    }
    s.out_ports.retain(|name, _| want_out.contains(name));
    for name in &want_out {
        let p = s.out_ports.entry(name.clone()).or_default();
        p.connected = out_conns.contains_key(name);
        if !p.connected {
            p.queue.clear();
        }
    }
}

fn open_input(inner: &Arc<Inner>, key: &str, resolved: &str) -> Option<MidiInputConnection<()>> {
    let input = MidiInput::new("LightBeat-midi-in").ok()?;
    let port = input.ports().into_iter().find(|p| input.port_name(p).is_ok_and(|n| n == resolved))?;
    let inner = inner.clone();
    let key = key.to_string();
    input
        .connect(
            &port,
            "lightbeat-midi-in",
            move |_ts, bytes, _| {
                if let Some(msg) = parse(bytes) {
                    let mut s = inner.state.lock().unwrap();
                    if let Some(p) = s.in_ports.get_mut(&key) {
                        p.apply(msg);
                    }
                }
            },
            (),
        )
        .ok()
}

fn open_output(resolved: &str) -> Option<MidiOutputConnection> {
    let output = MidiOutput::new("LightBeat-midi-out").ok()?;
    let port = output.ports().into_iter().find(|p| output.port_name(p).is_ok_and(|n| n == resolved))?;
    output.connect(&port, "lightbeat-midi-out").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CC7: Binding = Binding { kind: MsgKind::Cc, channel: 1, number: 7 };
    const NOTE60: Binding = Binding { kind: MsgKind::Note, channel: 1, number: 60 };

    #[test]
    fn messages_decode_and_encode_back() {
        assert_eq!(parse(&[0xB0, 7, 100]), Some(Msg::Cc { channel: 1, cc: 7, value: 100 }));
        assert_eq!(parse(&[0x91, 60, 90]), Some(Msg::NoteOn { channel: 2, note: 60, velocity: 90 }));
        assert_eq!(parse(&[0x91, 60, 0]), Some(Msg::NoteOff { channel: 2, note: 60 }), "note on with no velocity is a release");
        assert_eq!(parse(&[0x80, 60, 40]), Some(Msg::NoteOff { channel: 1, note: 60 }));
        assert_eq!(parse(&[0xE0, 0x00, 0x40]), Some(Msg::Bend { channel: 1, value: 8192 }));
        assert_eq!(parse(&[0xF8]), None, "clock ticks are not controls");
        assert_eq!(encode(CC7, 100), [0xB0, 7, 100]);
        assert_eq!(encode(NOTE60, 0), [0x90, 60, 0]);
        assert_eq!(encode(Binding { kind: MsgKind::PitchBend, channel: 3, number: 0 }, 8192), [0xE2, 0x00, 0x40]);
    }

    #[test]
    fn relative_encoders_decode_signed_steps() {
        assert_eq!([relative_delta(0), relative_delta(1), relative_delta(63), relative_delta(65), relative_delta(127)], [0, 1, 63, -63, -1]);
    }

    #[test]
    fn controls_remember_their_state_and_count_presses_and_turns() {
        let hub = MidiHub::idle();
        hub.inject("dev", Msg::Cc { channel: 1, cc: 7, value: 10 });
        hub.inject("dev", Msg::Cc { channel: 1, cc: 7, value: 127 });
        hub.inject("dev", Msg::NoteOn { channel: 1, note: 60, velocity: 100 });
        hub.inject("dev", Msg::NoteOff { channel: 1, note: 60 });
        hub.inject("dev", Msg::NoteOn { channel: 1, note: 60, velocity: 50 });
        let mut out = Vec::new();
        assert!(hub.read_controls("dev", &[Some(CC7), Some(NOTE60), None], &mut out));
        assert_eq!(out[0].map(|c| (c.raw, c.rel_sum)), Some((127, 10 - 1)));
        assert_eq!(out[1].map(|c| (c.raw, c.on_count, c.off_count)), Some((50, 2, 1)));
        assert_eq!(out[2], None);
        assert!(!hub.read_controls("elsewhere", &[Some(CC7)], &mut out));
        assert_eq!(out, vec![None]);
    }

    #[test]
    fn learning_sees_the_messages_after_a_point_and_old_ones_stay_out() {
        let hub = MidiHub::idle();
        hub.inject("dev", Msg::Cc { channel: 1, cc: 1, value: 1 });
        let start = hub.latest_seq("dev");
        assert_eq!(hub.messages_since("dev", start).0, vec![]);
        hub.inject("dev", Msg::Cc { channel: 1, cc: 7, value: 5 });
        hub.inject("dev", Msg::NoteOn { channel: 1, note: 60, velocity: 9 });
        let (messages, latest) = hub.messages_since("dev", start);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].binding(), CC7);
        assert_eq!(latest, start + 2);
    }

    #[test]
    fn output_messages_queue_only_while_the_device_is_connected() {
        let hub = MidiHub::idle();
        hub.send("out", CC7, 5);
        assert!(hub.sent("out").is_empty(), "no such device");
        hub.fake_output("out");
        hub.send("out", CC7, 64);
        assert_eq!(hub.sent("out"), vec![[0xB0, 7, 64]]);
    }
}
