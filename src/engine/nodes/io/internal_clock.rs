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
    /// Trigger input: rising edge toggles play/stop.
    trigger_in: f32,
    prev_trigger: f32,
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
            trigger_in: 0.0,
            prev_trigger: 0.0,
            beat_output: 0.0,
            last_beat_time: None,
            last_beat_is_downbeat: false,
            inputs: vec![PortDef::new("trigger", PortType::Logic)],
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
        if pi == 0 { self.trigger_in = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.trigger_in } else { 0.0 }
    }

    fn process(&mut self) {
        // Rising edge on trigger input toggles play/stop.
        if self.trigger_in >= 0.5 && self.prev_trigger < 0.5 {
            self.playing = !self.playing;
            if self.playing {
                // Reset phase on start so each play session begins cleanly.
                self.beat_pos = 0.0;
                self.last_beat_idx = -1;
                self.last_tick = Some(Instant::now());
            }
        }
        self.prev_trigger = self.trigger_in;

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
