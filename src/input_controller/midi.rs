//! MIDI backend for input controllers using `midir`.

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

use super::{InputSource, SharedControllers};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MidiSource {
    /// Continuous controller. Channel 1..=16, controller 0..=127.
    Cc { channel: u8, controller: u8 },
    /// Note (binary). Channel 1..=16, note 0..=127. Velocity is ignored for
    /// matching; we only care about on/off transitions.
    Note { channel: u8, note: u8 },
    /// 14-bit pitch bend, normalized to 0..1 with 0.5 = center.
    PitchBend { channel: u8 },
}

impl MidiSource {
    pub fn is_binary(&self) -> bool {
        matches!(self, MidiSource::Note { .. })
    }

    pub fn label(&self) -> String {
        match self {
            MidiSource::Cc { channel, controller } => format!("CC ch{} #{}", channel, controller),
            MidiSource::Note { channel, note } => format!("Note ch{} #{}", channel, note),
            MidiSource::PitchBend { channel } => format!("Pitch ch{}", channel),
        }
    }
}

/// List currently-available MIDI input port names on the system.
pub fn available_ports() -> Vec<String> {
    let input = match MidiInput::new("LightBeat-probe") {
        Ok(i) => i,
        Err(_) => return Vec::new(),
    };
    input.ports().iter()
        .filter_map(|p| input.port_name(p).ok())
        .collect()
}

/// List currently-available MIDI output port names.
pub fn available_output_ports() -> Vec<String> {
    let output = match MidiOutput::new("LightBeat-out-probe") {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    output.ports().iter()
        .filter_map(|p| output.port_name(p).ok())
        .collect()
}

pub struct MidiSession {
    pub controller_id: u32,
    pub port_name: String,
    pub output_port_name: Option<String>,
    /// Held to keep the connection alive; explicit drop on session drop.
    _conn: MidiInputConnection<()>,
    /// Signals the feedback worker to stop. Dropped with the session.
    _feedback_stop: Option<Arc<AtomicBool>>,
    _feedback_join: Option<thread::JoinHandle<()>>,
}

impl Drop for MidiSession {
    fn drop(&mut self) {
        if let Some(flag) = &self._feedback_stop {
            flag.store(true, Ordering::Relaxed);
        }
        if let Some(h) = self._feedback_join.take() {
            let _ = h.join();
        }
    }
}

impl MidiSession {
    pub fn open(
        controller_id: u32,
        port_name: String,
        output_port_name: Option<String>,
        shared: SharedControllers,
    ) -> Result<Self, String> {
        let mut input = MidiInput::new(&format!("LightBeat-{}", controller_id))
            .map_err(|e| e.to_string())?;
        input.ignore(midir::Ignore::None);

        let port = input.ports().into_iter()
            .find(|p| input.port_name(p).map(|n| n == port_name).unwrap_or(false))
            .ok_or_else(|| format!("port '{}' not found", port_name))?;

        let shared_for_cb = shared.clone();
        let conn = input.connect(
            &port,
            "lightbeat-input",
            move |_ts, msg, _| handle_midi_message(controller_id, msg, &shared_for_cb),
            (),
        ).map_err(|e| e.to_string())?;

        // Optional feedback: if an output port is configured, open a MIDI OUT
        // connection and start a worker thread that watches `out_values` on
        // the controller runtime and emits CC on change.
        let (feedback_stop, feedback_join) = if let Some(out_name) = &output_port_name {
            let out = MidiOutput::new(&format!("LightBeat-out-{}", controller_id))
                .map_err(|e| e.to_string())?;
            let out_port = out.ports().into_iter()
                .find(|p| out.port_name(p).map(|n| n == *out_name).unwrap_or(false))
                .ok_or_else(|| format!("output port '{}' not found", out_name))?;
            let out_conn = out.connect(&out_port, "lightbeat-output")
                .map_err(|e| e.to_string())?;

            let stop = Arc::new(AtomicBool::new(false));
            let stop_c = stop.clone();
            let shared_fb = shared.clone();
            let join = thread::Builder::new()
                .name(format!("lightbeat-midi-fb-{}", controller_id))
                .spawn(move || run_feedback_worker(controller_id, out_conn, shared_fb, stop_c))
                .map_err(|e| e.to_string())?;
            (Some(stop), Some(join))
        } else {
            (None, None)
        };

        Ok(Self {
            controller_id,
            port_name,
            output_port_name,
            _conn: conn,
            _feedback_stop: feedback_stop,
            _feedback_join: feedback_join,
        })
    }
}

/// Parse a raw MIDI message and route it to the matching learned input
/// (or push onto the learn buffer if the controller is in learn mode).
fn handle_midi_message(controller_id: u32, msg: &[u8], shared: &SharedControllers) {
    let parsed = parse_midi(msg);
    let (source, value) = match parsed {
        Some(x) => x,
        None => return,
    };

    let mut state = shared.lock().unwrap();
    let c = match state.iter_mut().find(|c| c.id == controller_id) {
        Some(c) => c,
        None => return,
    };

    if c.learning {
        c.learn_buffer.push_back(InputSource::Midi(source.clone()));
        while c.learn_buffer.len() > 32 {
            c.learn_buffer.pop_front();
        }
        return;
    }

    if let Some(idx) = c.inputs.iter().position(|i| match &i.source {
        InputSource::Midi(s) => s == &source,
    }) {
        if let Some(slot) = c.values.get_mut(idx) {
            *slot = value;
        }
        // Mirror into out_values so parameter-feedback-aware hardware stays
        // in sync even without the graph pushing a value — avoids fighting
        // the user when they first touch a control.
        if let Some(slot) = c.out_values.get_mut(idx) {
            *slot = value;
        }
    }
}

/// Decode a 1-3 byte status message into `(MidiSource, value 0..1)`.
/// Returns None for messages we don't care about.
fn parse_midi(msg: &[u8]) -> Option<(MidiSource, f32)> {
    if msg.is_empty() { return None; }
    let status = msg[0] & 0xF0;
    let channel = (msg[0] & 0x0F) + 1;

    match status {
        0x90 => {
            let note = *msg.get(1)?;
            let vel = *msg.get(2)?;
            let value = if vel > 0 { 1.0 } else { 0.0 };
            Some((MidiSource::Note { channel, note }, value))
        }
        0x80 => {
            let note = *msg.get(1)?;
            Some((MidiSource::Note { channel, note }, 0.0))
        }
        0xB0 => {
            let cc = *msg.get(1)?;
            let val = *msg.get(2)?;
            Some((MidiSource::Cc { channel, controller: cc }, val as f32 / 127.0))
        }
        0xE0 => {
            let lsb = *msg.get(1)? as u16;
            let msb = *msg.get(2)? as u16;
            let combined = (msb << 7) | lsb;
            Some((MidiSource::PitchBend { channel }, combined as f32 / 16383.0))
        }
        _ => None,
    }
}

/// Polls `out_values` at ~60 Hz and emits a MIDI message for every entry that
/// changed since the last tick. Encoding follows the learned input's source
/// type, so CC / Note / PitchBend all round-trip naturally.
fn run_feedback_worker(
    controller_id: u32,
    mut out_conn: MidiOutputConnection,
    shared: SharedControllers,
    stop: Arc<AtomicBool>,
) {
    const POLL_INTERVAL: Duration = Duration::from_millis(16);
    // Last-sent values per input index. Grows as needed.
    let mut last_sent: Vec<Option<f32>> = Vec::new();
    // Source-by-index snapshot so we don't touch the shared state while
    // serializing MIDI bytes.
    let mut sources: Vec<InputSource> = Vec::new();

    while !stop.load(Ordering::Relaxed) {
        thread::sleep(POLL_INTERVAL);

        // Snapshot the current out_values + source list under a short lock.
        let snapshot: Option<(Vec<f32>, Vec<InputSource>)> = {
            let state = shared.lock().unwrap();
            state.iter().find(|c| c.id == controller_id).map(|c| {
                (
                    c.out_values.clone(),
                    c.inputs.iter().map(|i| i.source.clone()).collect(),
                )
            })
        };
        let Some((out_values, src)) = snapshot else {
            continue;
        };

        // Sync tracking vecs if the input set grew/shrank.
        if sources.len() != src.len() {
            sources = src;
            last_sent.resize(sources.len(), None);
        }
        if last_sent.len() != out_values.len() {
            last_sent.resize(out_values.len(), None);
        }

        for (i, v) in out_values.iter().enumerate() {
            let prev = last_sent[i];
            if prev == Some(*v) { continue; }
            let bytes = encode_midi(sources.get(i), *v);
            if let Some(b) = bytes {
                let _ = out_conn.send(&b);
                last_sent[i] = Some(*v);
            }
        }
    }
}

/// Encode a 0..1 value into a MIDI message matching the source kind.
fn encode_midi(source: Option<&InputSource>, value: f32) -> Option<[u8; 3]> {
    let src = source?;
    let v = value.clamp(0.0, 1.0);
    match src {
        InputSource::Midi(MidiSource::Cc { channel, controller }) => {
            let status = 0xB0 | ((*channel - 1) & 0x0F);
            let data = (v * 127.0).round() as u8;
            Some([status, *controller & 0x7F, data & 0x7F])
        }
        InputSource::Midi(MidiSource::Note { channel, note }) => {
            let status = 0x90 | ((*channel - 1) & 0x0F);
            // For button LEDs: velocity 127 = on, 0 = off.
            let vel = if v >= 0.5 { 127 } else { 0 };
            Some([status, *note & 0x7F, vel])
        }
        InputSource::Midi(MidiSource::PitchBend { channel }) => {
            let status = 0xE0 | ((*channel - 1) & 0x0F);
            let combined = (v * 16383.0).round() as u16;
            Some([status, (combined & 0x7F) as u8, ((combined >> 7) & 0x7F) as u8])
        }
    }
}
