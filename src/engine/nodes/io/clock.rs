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
            ],
            beat_output: 0.0,
            phase_output: 0.0,
        }
    }
}

impl ProcessNode for ClockProcessNode {
    fn node_id(&self) -> NodeId {
        self.id
    }

    fn type_name(&self) -> &'static str {
        "Clock"
    }

    fn inputs(&self) -> &[PortDef] {
        &[]
    }

    fn outputs(&self) -> &[PortDef] {
        &self.outputs
    }

    fn process(&mut self) {
        let snap = self.snapshot.lock().unwrap();
        let mut bs = self.beat_state.lock().unwrap();

        if bs.pending_beats > 0 {
            bs.pending_beats = 0;
            self.beat_output = 1.0;
        } else {
            self.beat_output = 0.0;
        }

        if snap.playing {
            self.phase_output = snap.phase as f32;
        }
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.beat_output,
            1 => {
                let snap = self.snapshot.lock().unwrap();
                if snap.playing { 1.0 } else { 0.0 }
            }
            2 => self.phase_output,
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
