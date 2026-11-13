//! DMX coming in from another lighting desk.
//!
//! A *DMX input* in the setup is a source of universes, for example "the
//! Chamsys desk sends universes 4 and 5 over Art-Net". The hub receives the
//! packets, keeps the latest frame of every universe an input listens to, and
//! announces LightBeat on the network as an Art-Net node so the desk can find
//! it and send to it. DMX Input nodes in the graph read channels from there.

pub mod artnet;

use std::collections::{HashMap, VecDeque};
use std::net::Ipv4Addr;
use std::sync::Arc;
use parking_lot::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// The Art-Net UDP port.
pub const ARTNET_PORT: u16 = 6454;

/// How long after its last packet a universe still counts as live. Desks send
/// a universe at least about once a second, even when nothing changes.
pub const LIVE_WINDOW: Duration = Duration::from_secs(5);

/// Changes kept for learning.
const MAX_CHANGES: usize = 512;

/// A universe's number split the way Art-Net and the objects write it.
pub fn split_address(port_address: u16) -> (u8, u8, u8) {
    (((port_address >> 8) & 0x7F) as u8, ((port_address >> 4) & 0x0F) as u8, (port_address & 0x0F) as u8)
}

pub fn join_address(net: u8, subnet: u8, universe: u8) -> u16 {
    (u16::from(net & 0x7F) << 8) | (u16::from(subnet & 0x0F) << 4) | u16::from(universe & 0x0F)
}

/// "net.sub.universe", for showing.
pub fn format_address(port_address: u16) -> String {
    let (n, s, u) = split_address(port_address);
    format!("{n}.{s}.{u}")
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DmxInputConfig {
    pub id: u32,
    pub name: String,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    pub kind: DmxInputKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DmxInputKind {
    /// Art-Net. LightBeat announces itself as a node with these universes
    /// (15-bit port addresses) and listens for them.
    ArtNet {
        #[serde(default)]
        universes: Vec<u16>,
        /// Announce on this local address only; empty for every interface.
        #[serde(default)]
        interface: String,
    },
}

impl DmxInputKind {
    pub fn label(&self) -> &'static str {
        match self {
            DmxInputKind::ArtNet { .. } => "Art-Net",
        }
    }
}

impl DmxInputConfig {
    pub fn new_art_net(id: u32, name: impl Into<String>) -> Self {
        Self { id, name: name.into(), enabled: true, kind: DmxInputKind::ArtNet { universes: vec![0], interface: String::new() } }
    }

    pub fn universes(&self) -> &[u16] {
        match &self.kind {
            DmxInputKind::ArtNet { universes, .. } => universes,
        }
    }

    pub fn interface(&self) -> &str {
        match &self.kind {
            DmxInputKind::ArtNet { interface, .. } => interface,
        }
    }
}

/// The latest frame of a universe.
#[derive(Clone)]
pub struct Universe {
    pub data: [u8; 512],
    /// How many channels the last packet carried.
    pub len: usize,
    pub last_rx: Option<Instant>,
    pub from: Option<Ipv4Addr>,
    pub packets: u64,
    /// Packets per second, over the last second.
    pub rate: f32,
    window_packets: u64,
}

impl Default for Universe {
    fn default() -> Self {
        Self { data: [0; 512], len: 0, last_rx: None, from: None, packets: 0, rate: 0.0, window_packets: 0 }
    }
}

impl Universe {
    pub fn live(&self) -> bool {
        self.last_rx.is_some_and(|t| t.elapsed() < LIVE_WINDOW)
    }
}

/// A universe that arrived but no enabled input listens to.
#[derive(Debug, Clone)]
pub struct Heard {
    pub last: Instant,
    pub from: Ipv4Addr,
}

/// A channel that changed value, for learning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Change {
    pub seq: u64,
    pub universe: u16,
    /// Zero based.
    pub channel: usize,
    pub value: u8,
}

#[derive(Default)]
pub struct DmxState {
    inputs: Vec<DmxInputConfig>,
    universes: HashMap<u16, Universe>,
    heard: HashMap<u16, Heard>,
    changes: VecDeque<Change>,
    seq: u64,
    /// Why the socket could not be opened, if it could not.
    bind_error: Option<String>,
    /// The port the socket is listening on, once it is.
    listening: Option<u16>,
    /// Tell the network about us again.
    announce: bool,
    poll_counter: u32,
}

/// What the setup window shows of one input.
pub struct InputStatus {
    pub live: bool,
    pub listening: bool,
    pub bind_error: Option<String>,
    pub universes: Vec<(u16, Universe)>,
}

#[derive(Clone)]
pub struct DmxInputHub(Arc<Mutex<DmxState>>);

impl DmxInputHub {
    /// A hub with its worker running on the Art-Net port.
    pub fn start() -> Self {
        Self::start_on(ARTNET_PORT)
    }

    /// A hub whose worker listens on `port` (0 for any free port, for tests).
    pub fn start_on(port: u16) -> Self {
        let hub = Self::idle();
        let worker = hub.clone();
        std::thread::Builder::new()
            .name("lightbeat-dmx-in".into())
            .spawn(move || artnet::run(worker, port))
            .expect("spawn the DMX input worker");
        hub
    }

    /// A hub without a worker.
    pub fn idle() -> Self {
        Self(Arc::new(Mutex::new(DmxState::default())))
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, DmxState> {
        self.0.lock()
    }

    // -- the setup ----------------------------------------------------------

    pub fn set_inputs(&self, inputs: &[DmxInputConfig]) {
        let mut s = self.lock();
        s.inputs = inputs.to_vec();
        s.prune_universes();
        s.announce = true;
    }

    pub fn export(&self) -> Vec<DmxInputConfig> {
        self.lock().inputs.clone()
    }

    pub fn input_name(&self, id: u32) -> Option<String> {
        self.lock().inputs.iter().find(|i| i.id == id).map(|i| i.name.clone())
    }

    pub fn input_universes(&self, id: u32) -> Vec<u16> {
        self.lock().inputs.iter().find(|i| i.id == id).map(|i| i.universes().to_vec()).unwrap_or_default()
    }

    pub fn input_choices(&self) -> Vec<(u32, String)> {
        self.lock().inputs.iter().map(|i| (i.id, i.name.clone())).collect()
    }

    // -- what the setup window shows -----------------------------------------

    pub fn status(&self, id: u32) -> Option<InputStatus> {
        let s = self.lock();
        let input = s.inputs.iter().find(|i| i.id == id)?;
        let universes: Vec<(u16, Universe)> =
            input.universes().iter().map(|u| (*u, s.universes.get(u).cloned().unwrap_or_default())).collect();
        Some(InputStatus {
            live: universes.iter().any(|(_, u)| u.live()),
            listening: s.listening.is_some(),
            bind_error: s.bind_error.clone(),
            universes,
        })
    }

    /// Universes that arrived recently and that no enabled input listens to.
    pub fn heard_unconfigured(&self) -> Vec<(u16, Ipv4Addr)> {
        let s = self.lock();
        let mut out: Vec<(u16, Ipv4Addr)> = s
            .heard
            .iter()
            .filter(|(u, h)| h.last.elapsed() < LIVE_WINDOW && !s.wanted(**u))
            .map(|(u, h)| (*u, h.from))
            .collect();
        out.sort();
        out
    }

    // -- what the nodes read -------------------------------------------------

    /// Fill `out` with the channels asked for, each `(universe, first channel
    /// (zero based), count)`, laid end to end; a missing channel reads 0.
    pub fn read_channels(&self, requests: &[(u16, usize, usize)], out: &mut Vec<u8>) {
        out.clear();
        let s = self.lock();
        for &(universe, first, count) in requests {
            let u = s.universes.get(&universe);
            for c in first..first + count {
                out.push(u.and_then(|u| u.data.get(c).copied()).unwrap_or(0));
            }
        }
    }

    pub(crate) fn universe_live(&self, universe: u16) -> bool {
        self.lock().universes.get(&universe).is_some_and(Universe::live)
    }

    /// Whether any universe of input `id` has had a packet lately.
    pub fn input_live(&self, id: u32) -> bool {
        let s = self.lock();
        s.inputs
            .iter()
            .find(|i| i.id == id)
            .is_some_and(|i| i.universes().iter().any(|u| s.universes.get(u).is_some_and(Universe::live)))
    }

    // -- learning ------------------------------------------------------------

    pub fn latest_seq(&self) -> u64 {
        self.lock().seq
    }

    /// Channel changes after number `since`, restricted to `universes`.
    pub fn changes_since(&self, since: u64, universes: &[u16]) -> Vec<Change> {
        self.lock().changes.iter().filter(|c| c.seq > since && universes.contains(&c.universe)).copied().collect()
    }

    // -- the worker ----------------------------------------------------------

    /// Take a received frame in. Channels past the end of the packet keep their value.
    pub fn apply_dmx(&self, universe: u16, data: &[u8], from: Ipv4Addr) {
        let mut s = self.lock();
        if !s.wanted(universe) {
            s.heard.insert(universe, Heard { last: Instant::now(), from });
            return;
        }
        s.heard.remove(&universe);
        let first = !s.universes.contains_key(&universe) || s.universes[&universe].packets == 0;
        let mut seq = s.seq;
        let mut changed = Vec::new();
        let u = s.universes.entry(universe).or_default();
        let n = data.len().min(512);
        for (c, &v) in data[..n].iter().enumerate() {
            // The first frame is a state, not a series of moves.
            if !first && u.data[c] != v {
                seq += 1;
                changed.push(Change { seq, universe, channel: c, value: v });
            }
            u.data[c] = v;
        }
        u.len = n;
        u.last_rx = Some(Instant::now());
        u.from = Some(from);
        u.packets += 1;
        u.window_packets += 1;
        s.seq = seq;
        for c in changed {
            s.changes.push_back(c);
            if s.changes.len() > MAX_CHANGES {
                s.changes.pop_front();
            }
        }
    }

    pub(crate) fn tick_rates(&self) {
        for u in self.lock().universes.values_mut() {
            u.rate = u.window_packets as f32;
            u.window_packets = 0;
        }
    }

    pub(crate) fn wants_socket(&self) -> bool {
        self.lock().inputs.iter().any(|i| i.enabled && !i.universes().is_empty())
    }

    pub(crate) fn take_announce(&self) -> bool {
        std::mem::take(&mut self.lock().announce)
    }

    pub(crate) fn request_announce(&self) {
        self.lock().announce = true;
    }

    pub(crate) fn set_listening(&self, listening: Option<u16>, error: Option<String>) {
        let mut s = self.lock();
        s.listening = listening;
        s.bind_error = error;
    }

    #[cfg(test)]
    pub fn listening_port(&self) -> Option<u16> {
        self.lock().listening
    }
}

impl DmxState {
    /// Whether an enabled input listens to `universe`.
    fn wanted(&self, universe: u16) -> bool {
        self.inputs.iter().any(|i| i.enabled && i.universes().contains(&universe))
    }

    /// Forget the frames of universes nothing listens to any more.
    fn prune_universes(&mut self) {
        let wanted: Vec<u16> = self
            .inputs
            .iter()
            .filter(|i| i.enabled)
            .flat_map(|i| i.universes().iter().copied())
            .collect();
        self.universes.retain(|u, _| wanted.contains(u));
    }

    pub(crate) fn next_report_number(&mut self) -> u32 {
        self.poll_counter = self.poll_counter.wrapping_add(1) % 10_000;
        self.poll_counter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESK: Ipv4Addr = Ipv4Addr::new(2, 0, 0, 7);

    fn hub_with(universes: &[u16]) -> DmxInputHub {
        let hub = DmxInputHub::idle();
        hub.set_inputs(&[hub_input(universes)]);
        hub
    }

    fn hub_input(universes: &[u16]) -> DmxInputConfig {
        let mut input = DmxInputConfig::new_art_net(1, "Desk");
        input.kind = DmxInputKind::ArtNet { universes: universes.to_vec(), interface: String::new() };
        input
    }

    #[test]
    fn a_port_address_splits_into_net_subnet_and_universe_and_back() {
        assert_eq!(split_address(0x1234), (0x12, 3, 4));
        assert_eq!(join_address(0x12, 3, 4), 0x1234);
        assert_eq!(format_address(0x0105), "1.0.5");
        assert_eq!(join_address(0xFF, 0xFF, 0xFF), 0x7FFF, "the top bit of the net is not part of the address");
    }

    #[test]
    fn frames_of_listened_universes_are_kept_and_the_rest_are_only_noticed() {
        let hub = hub_with(&[3]);
        hub.apply_dmx(3, &[10, 20, 30], DESK);
        hub.apply_dmx(9, &[1; 512], DESK);
        let mut out = Vec::new();
        hub.read_channels(&[(3, 0, 3), (3, 510, 2), (9, 0, 1)], &mut out);
        assert_eq!(out, vec![10, 20, 30, 0, 0, 0], "short packets leave the rest at zero, and unlistened universes read zero");
        assert!(hub.input_live(1));
        assert_eq!(hub.heard_unconfigured().iter().map(|h| h.0).collect::<Vec<_>>(), vec![9]);
    }

    #[test]
    fn a_short_packet_keeps_the_channels_it_does_not_carry() {
        let hub = hub_with(&[0]);
        hub.apply_dmx(0, &[1, 2, 3, 4], DESK);
        hub.apply_dmx(0, &[9, 9], DESK);
        let mut out = Vec::new();
        hub.read_channels(&[(0, 0, 4)], &mut out);
        assert_eq!(out, vec![9, 9, 3, 4]);
    }

    #[test]
    fn learning_sees_what_moved_after_a_point_and_not_the_first_frame() {
        let hub = hub_with(&[0, 1]);
        hub.apply_dmx(0, &[5, 5, 5], DESK);
        assert_eq!(hub.latest_seq(), 0, "the first frame is not a move");
        let start = hub.latest_seq();
        hub.apply_dmx(0, &[5, 99, 5], DESK);
        hub.apply_dmx(1, &[1], DESK);
        hub.apply_dmx(1, &[2], DESK);
        let moved = hub.changes_since(start, &[0]);
        assert_eq!(moved.len(), 1);
        assert_eq!((moved[0].universe, moved[0].channel, moved[0].value), (0, 1, 99));
        assert_eq!(hub.changes_since(start, &[0, 1]).len(), 2);
    }

    #[test]
    fn disabling_an_input_or_removing_a_universe_forgets_its_frames() {
        let hub = hub_with(&[0, 1]);
        hub.apply_dmx(1, &[7], DESK);
        assert!(hub.input_live(1));
        hub.set_inputs(&[hub_input(&[0])]);
        let mut out = Vec::new();
        hub.read_channels(&[(1, 0, 1)], &mut out);
        assert_eq!(out, vec![0]);
        assert!(!hub.input_live(1));
        hub.apply_dmx(1, &[7], DESK);
        assert_eq!(hub.heard_unconfigured().len(), 1, "now it is only heard");
    }

    #[test]
    fn the_config_round_trips_and_defaults_fill_in_old_files() {
        let input = DmxInputConfig::new_art_net(4, "Chamsys");
        let json = serde_json::to_string(&input).unwrap();
        assert_eq!(serde_json::from_str::<DmxInputConfig>(&json).unwrap(), input);
        let minimal: DmxInputConfig = serde_json::from_str(r#"{"id":1,"name":"x","kind":{"ArtNet":{}}}"#).unwrap();
        assert!(minimal.enabled && minimal.universes().is_empty() && minimal.interface().is_empty());
    }
}
