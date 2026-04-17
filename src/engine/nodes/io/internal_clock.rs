use std::time::Instant;

use crate::engine::types::*;

/// Display state sent to the UI for rendering the internal clock widget.
pub struct InternalClockDisplay {
    pub bpm: f32,
    pub playing: bool,
    pub last_beat_time: Option<Instant>,
    pub last_beat_is_downbeat: bool,
}

/// A standalone clock that generates beat/phase signals from a configurable BPM.
/// Independent of Ableton Link.
pub struct InternalClockProcessNode {
    id: NodeId,
    bpm: f32,
    playing: bool,
    /// Continuous beat counter (e.g. 0.0..N) — phase = beat_pos % 1.0.
    beat_pos: f64,
    /// Beat number of the last fired beat (used to count downbeats).
    last_beat_idx: i64,
    last_tick: Option<Instant>,
    /// Input 0: rising edge toggles play/stop.
    play_stop_in: f32,
    prev_play_stop: f32,
    /// Input 1: BPM value (continuous, applied on `set_bpm` rising edge).
    bpm_in: f32,
    /// Input 2: rising edge captures `bpm_in` into `self.bpm`.
    set_bpm_in: f32,
    prev_set_bpm: f32,
    /// Input 3: rising edge resets phase / beat counter to 0.
    reset_in: f32,
    prev_reset: f32,
    beat_output: f32,
    /// For display (set when a beat fires).
    last_beat_time: Option<Instant>,
    last_beat_is_downbeat: bool,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl InternalClockProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            bpm: 120.0,
            playing: false,
            beat_pos: 0.0,
            last_beat_idx: -1,
            last_tick: None,
            play_stop_in: 0.0,
            prev_play_stop: 0.0,
            bpm_in: 0.0,
            set_bpm_in: 0.0,
            prev_set_bpm: 0.0,
            reset_in: 0.0,
            prev_reset: 0.0,
            beat_output: 0.0,
            last_beat_time: None,
            last_beat_is_downbeat: false,
            inputs: vec![
                PortDef::new("play/stop", PortType::Logic),
                PortDef::new("bpm", PortType::Untyped),
                PortDef::new("set bpm", PortType::Logic),
                PortDef::new("reset", PortType::Logic),
            ],
            outputs: vec![
                PortDef::new("beat", PortType::Logic),
                PortDef::new("play", PortType::Logic),
                PortDef::new("phase", PortType::Phase),
            ],
        }
    }
}

impl ProcessNode for InternalClockProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Internal Clock" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        match pi {
            0 => self.play_stop_in = v,
            1 => self.bpm_in = v,
            2 => self.set_bpm_in = v,
            3 => self.reset_in = v,
            _ => {}
        }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi {
            0 => self.play_stop_in,
            1 => self.bpm_in,
            2 => self.set_bpm_in,
            3 => self.reset_in,
            _ => 0.0,
        }
    }

    fn process(&mut self) {
        // Rising edge on `set bpm`: capture bpm input into the internal BPM.
        if self.set_bpm_in >= 0.5 && self.prev_set_bpm < 0.5 && self.bpm_in > 0.0 {
            self.bpm = self.bpm_in.clamp(20.0, 300.0);
        }
        self.prev_set_bpm = self.set_bpm_in;

        // Rising edge on `reset`: zero the phase + beat counter.
        if self.reset_in >= 0.5 && self.prev_reset < 0.5 {
            self.beat_pos = 0.0;
            self.last_beat_idx = -1;
            self.last_tick = if self.playing { Some(Instant::now()) } else { None };
        }
        self.prev_reset = self.reset_in;

        // Rising edge on play/stop: toggle play state.
        if self.play_stop_in >= 0.5 && self.prev_play_stop < 0.5 {
            self.playing = !self.playing;
            if self.playing {
                // Fresh play session — reset phase so beat 1 lands now.
                self.beat_pos = 0.0;
                self.last_beat_idx = -1;
                self.last_tick = Some(Instant::now());
            }
        }
        self.prev_play_stop = self.play_stop_in;

        self.beat_output = 0.0;

        if !self.playing {
            self.last_tick = None;
            return;
        }

        let now = Instant::now();
        let dt = match self.last_tick {
            Some(prev) => now.duration_since(prev).as_secs_f64(),
            None => 0.0,
        };
        self.last_tick = Some(now);

        // Advance beat position.
        let beats_per_sec = self.bpm as f64 / 60.0;
        self.beat_pos += dt * beats_per_sec;

        // Fire beat output when crossing into a new whole beat.
        let cur_beat_idx = self.beat_pos.floor() as i64;
        if cur_beat_idx > self.last_beat_idx {
            self.beat_output = 1.0;
            self.last_beat_idx = cur_beat_idx;
            self.last_beat_time = Some(now);
            self.last_beat_is_downbeat = cur_beat_idx % 4 == 0;
        }
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.beat_output,
            1 => if self.playing { 1.0 } else { 0.0 },
            2 => self.beat_pos.fract() as f32,
            _ => 0.0,
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Float {
            name: "BPM".into(),
            value: self.bpm,
            min: 20.0,
            max: 300.0,
            step: 0.1,
            unit: "bpm",
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 { self.bpm = value.as_f32().clamp(20.0, 300.0); }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(InternalClockDisplay {
            bpm: self.bpm,
            playing: self.playing,
            last_beat_time: self.last_beat_time,
            last_beat_is_downbeat: self.last_beat_is_downbeat,
        }));
    }
}
