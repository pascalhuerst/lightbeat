use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::beat_clock::{BeatInfo, BeatListener, LinkSnapshot};
use crate::engine::types::*;

/// Display state sent to the UI for rendering the clock widget.
pub struct ClockDisplay {
    pub tempo: f64,
    pub playing: bool,
    pub num_peers: usize,
    pub last_beat_time: Option<Instant>,
    pub last_beat_is_downbeat: bool,
}

/// Shared between the beat clock callback thread and the engine thread.
pub struct ClockBeatState {
    pub last_beat_time: Option<Instant>,
    pub last_beat_is_downbeat: bool,
    pub pending_beats: u32,
}

impl ClockBeatState {
    pub fn new() -> Self {
        Self {
            last_beat_time: None,
            last_beat_is_downbeat: false,
            pending_beats: 0,
        }
    }
}

impl BeatListener for ClockBeatState {
    fn on_beat(&mut self, info: &BeatInfo) {
        self.last_beat_time = Some(Instant::now());
        self.last_beat_is_downbeat = info.beat.is_multiple_of(4);
        self.pending_beats += 1;
    }

    fn on_transport_change(&mut self, playing: bool) {
        if !playing {
            self.last_beat_time = None;
        }
    }
}

pub struct ClockProcessNode {
    id: NodeId,
    pub beat_state: Arc<Mutex<ClockBeatState>>,
    pub snapshot: Arc<Mutex<LinkSnapshot>>,
    outputs: Vec<PortDef>,
    beat_output: f32,
    phase_output: f32,
    /// Read from the snapshot `step` already holds, so the two extra outputs
    /// cost no extra lock per tick.
    bpm_output: f32,
    peers_output: f32,
    /// Whole beat the shifted clock was in at the last tick; none while stopped.
    last_beat_idx: Option<i64>,
}

impl ClockProcessNode {
    pub fn new(id: NodeId, snapshot: Arc<Mutex<LinkSnapshot>>) -> Self {
        Self {
            id,
            beat_state: Arc::new(Mutex::new(ClockBeatState::new())),
            snapshot,
            outputs: vec![
                PortDef::new("beat", PortType::Logic),
                PortDef::new("play", PortType::Logic),
                PortDef::new("phase", PortType::Phase),
                PortDef::new("bpm", PortType::Untyped),
                PortDef::new("peers", PortType::Untyped),
            ],
            beat_output: 0.0,
            phase_output: 0.0,
            bpm_output: 0.0,
            peers_output: 0.0,
            last_beat_idx: None,
        }
    }

    /// One tick with the clock run `lead_ms` ahead of Link's position, to
    /// make up for the delay between the engine and the light. The beat is
    /// found by watching that shifted position cross a whole beat, so it
    /// moves with the phase.
    fn step(&mut self, lead_ms: f32) {
        // The subscription still feeds the display; its count is not used.
        self.beat_state.lock().unwrap().pending_beats = 0;
        let snap = self.snapshot.lock().unwrap().clone();

        self.bpm_output = snap.tempo as f32;
        self.peers_output = snap.num_peers as f32;
        self.beat_output = 0.0;
        if !snap.playing {
            self.last_beat_idx = None;
            return;
        }
        let lead = crate::latency::beats(lead_ms, snap.tempo);
        let position = snap.beat + lead;
        let idx = position.floor() as i64;
        if self.last_beat_idx.is_some_and(|last| idx > last) {
            self.beat_output = 1.0;
        }
        self.last_beat_idx = Some(idx);
        // Link's phase is within the bar of `quantum` beats, normalized.
        const QUANTUM: f64 = 4.0;
        self.phase_output = (snap.phase + lead / QUANTUM).rem_euclid(1.0) as f32;
    }
}

impl ProcessNode for ClockProcessNode {
    fn node_id(&self) -> NodeId {
        self.id
    }

    fn type_name(&self) -> &'static str {
        "Link Clock"
    }

    fn inputs(&self) -> &[PortDef] {
        &[]
    }

    fn outputs(&self) -> &[PortDef] {
        &self.outputs
    }

    fn process(&mut self) {
        self.step(crate::latency::lead_ms());
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.beat_output,
            1 => {
                let snap = self.snapshot.lock().unwrap();
                if snap.playing { 1.0 } else { 0.0 }
            }
            2 => self.phase_output,
            3 => self.bpm_output,
            4 => self.peers_output,
            _ => 0.0,
        }
    }


    fn update_display(&self, shared: &mut NodeSharedState) {
        let snap = self.snapshot.lock().unwrap();
        let bs = self.beat_state.lock().unwrap();
        shared.display = Some(Box::new(ClockDisplay {
            tempo: snap.tempo,
            playing: snap.playing,
            num_peers: snap.num_peers,
            last_beat_time: bs.last_beat_time,
            last_beat_is_downbeat: bs.last_beat_is_downbeat,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock_at(beat: f64, tempo: f64) -> (ClockProcessNode, Arc<Mutex<LinkSnapshot>>) {
        let snap = Arc::new(Mutex::new(LinkSnapshot {
            tempo,
            beat,
            phase: (beat / 4.0).fract(),
            playing: true,
            num_peers: 0,
        }));
        (ClockProcessNode::new(NodeId(1), snap.clone()), snap)
    }

    fn move_to(snap: &Arc<Mutex<LinkSnapshot>>, beat: f64) {
        let mut s = snap.lock().unwrap();
        s.beat = beat;
        s.phase = (beat / 4.0).fract();
    }

    #[test]
    fn a_lead_fires_the_beat_and_moves_the_phase_earlier() {
        // 120 BPM, 250 ms is half a beat.
        let (mut c, snap) = clock_at(0.3, 120.0);
        c.step(250.0);
        assert_eq!(c.read_output(0), 0.0);
        move_to(&snap, 0.6);
        c.step(250.0);
        assert_eq!(c.read_output(0), 1.0, "Link is at 0.6, the shifted clock at 1.1");
        assert!((c.read_output(2) - (0.6 + 0.5) / 4.0).abs() < 1e-5);
        move_to(&snap, 0.7);
        c.step(250.0);
        assert_eq!(c.read_output(0), 0.0, "once per beat");
    }

    #[test]
    fn the_session_tempo_and_peer_count_are_outputs() {
        let (mut c, snap) = clock_at(0.3, 126.0);
        snap.lock().unwrap().num_peers = 2;
        c.step(0.0);
        assert_eq!(c.outputs().len(), 5);
        assert_eq!((c.outputs()[3].name.as_str(), c.outputs()[4].name.as_str()), ("bpm", "peers"));
        assert_eq!((c.read_output(3), c.read_output(4)), (126.0, 2.0));
        snap.lock().unwrap().playing = false;
        c.step(0.0);
        assert_eq!(c.read_output(3), 126.0, "the tempo is there while stopped too");
    }

    #[test]
    fn without_a_lead_it_follows_link_and_never_fires_on_start_or_while_stopped() {
        let (mut c, snap) = clock_at(3.9, 120.0);
        c.step(0.0);
        assert_eq!(c.read_output(0), 0.0, "joining mid-beat is not a beat");
        move_to(&snap, 4.1);
        c.step(0.0);
        assert_eq!(c.read_output(0), 1.0);
        snap.lock().unwrap().playing = false;
        move_to(&snap, 5.2);
        c.step(0.0);
        assert_eq!(c.read_output(0), 0.0);
    }
}
