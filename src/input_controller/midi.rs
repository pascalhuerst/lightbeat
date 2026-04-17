//! MIDI backend for input controllers using `midir`.

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

use super::{InputSource, MidiLogEntry, SharedControllers, MIDI_LOG_CAPACITY};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MidiSource {
    /// Continuous controller. Channel 1..=16, controller 0..=127.
    Cc { channel: u8, controller: u8 },
    /// Relative-1 continuous controller (Push 1 / BCF "Relative 1"). Incoming
    /// values decode to a signed delta: 1..=63 = +delta, 65..=127 = -(128−v),
    /// 0 ignored. The MIDI handler accumulates into `values[i]` in 0..1.
    CcRelative { channel: u8, controller: u8 },
    /// Note (binary). Channel 1..=16, note 0..=127. Velocity is ignored for
    /// matching; we only care about on/off transitions.
    Note { channel: u8, note: u8 },
    /// Pad-style note with velocity preserved as a 0..1 continuous value on
    /// press (release goes to 0). Matches Push 1 / Push 2 pad behaviour where
    /// the velocity encodes impact strength rather than being a binary gate.
    NoteVelocity { channel: u8, note: u8 },
    /// 14-bit pitch bend, normalized to 0..1 with 0.5 = center.
    PitchBend { channel: u8 },
}

impl MidiSource {
    pub fn is_binary(&self) -> bool {
        matches!(self, MidiSource::Note { .. })
    }

    pub fn channel(&self) -> u8 {
        match self {
            MidiSource::Cc { channel, .. }
            | MidiSource::CcRelative { channel, .. }
            | MidiSource::Note { channel, .. }
            | MidiSource::NoteVelocity { channel, .. }
            | MidiSource::PitchBend { channel } => *channel,
        }
    }

    pub fn label(&self) -> String {
        match self {
            MidiSource::Cc { channel, controller } => format!("CC ch{} #{}", channel, controller),
            MidiSource::CcRelative { channel, controller } => format!("CC~ ch{} #{}", channel, controller),
            MidiSource::Note { channel, note } => format!("Note ch{} #{}", channel, note),
            MidiSource::NoteVelocity { channel, note } => format!("NoteV ch{} #{}", channel, note),
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

    let mut state = shared.lock().unwrap();
    let c = match state.iter_mut().find(|c| c.id == controller_id) {
        Some(c) => c,
        None => return,
    };

    // Always log — debug panel reads this; cost is negligible.
    let log_entry = MidiLogEntry {
        raw: msg.to_vec(),
        decoded: parsed.as_ref().map(|(s, v)| (InputSource::Midi(s.clone()), *v)),
        matched_input_idx: None, // filled in below once we know the match
        instant: std::time::Instant::now(),
    };
    c.midi_log.push_back(log_entry);
    while c.midi_log.len() > MIDI_LOG_CAPACITY {
        c.midi_log.pop_front();
    }

    let (source, value) = match parsed {
        Some(x) => x,
        None => return,
    };

    if c.learning {
        c.learn_buffer.push_back(InputSource::Midi(source.clone()));
        while c.learn_buffer.len() > 32 {
            c.learn_buffer.pop_front();
        }
        return;
    }

    // Lookup allows a Cc message to match either `Cc` or `CcRelative` with
    // the same channel/controller — the learned source decides semantics.
    let idx = c.inputs.iter().position(|i| match (&source, &i.source) {
        (MidiSource::Cc { channel: sc, controller: scc },
         InputSource::Midi(MidiSource::Cc { channel, controller })) =>
            sc == channel && scc == controller,
        (MidiSource::Cc { channel: sc, controller: scc },
         InputSource::Midi(MidiSource::CcRelative { channel, controller })) =>
            sc == channel && scc == controller,
        (MidiSource::Note { channel: sc, note: sn },
         InputSource::Midi(MidiSource::Note { channel, note })) =>
            sc == channel && sn == note,
        (MidiSource::Note { channel: sc, note: sn },
         InputSource::Midi(MidiSource::NoteVelocity { channel, note })) =>
            sc == channel && sn == note,
        (s, InputSource::Midi(t)) => s == t,
    });
    let Some(idx) = idx else { return };

    let (raw_cc_val, raw_note_vel) = match &source {
        MidiSource::Cc { .. } | MidiSource::CcRelative { .. } => {
            (msg.get(2).copied().unwrap_or(0), 0)
        }
        MidiSource::Note { .. } | MidiSource::NoteVelocity { .. } => {
            (0, msg.get(2).copied().unwrap_or(0))
        }
        _ => (0, 0),
    };

    // Compute stored value per the *learned* source type.
    let stored = match &c.inputs[idx].source {
        InputSource::Midi(MidiSource::CcRelative { .. }) => {
            // Relative-1: 1..63 positive, 65..127 negative, 0 ignored.
            let delta = match raw_cc_val {
                0 => 0i32,
                v @ 1..=63 => v as i32,
                v => v as i32 - 128,
            };
            // Normalize deltas so ~24 clicks ≈ 1.0 — Push/BCF encoders both
            // feel like this at the default detent density.
            let step = 1.0 / 24.0;
            let cur = c.values.get(idx).copied().unwrap_or(0.0);
            (cur + delta as f32 * step).clamp(0.0, 1.0)
        }
        InputSource::Midi(MidiSource::NoteVelocity { .. }) => {
            // Note On with vel 0 = release, else velocity 0..1.
            if raw_note_vel == 0 { 0.0 } else { raw_note_vel as f32 / 127.0 }
        }
        _ => value,
    };

    if let Some(slot) = c.values.get_mut(idx) {
        *slot = stored;
    }
    // Mirror into out_values so hardware stays in sync. Skip for relative
    // encoders — echoing the delta back would spin the value endlessly.
    // Also skip when the debug panel has taken over — the user is driving
    // the hardware from the UI and doesn't want device input to stomp it.
    let mirror = !matches!(c.inputs[idx].source, InputSource::Midi(MidiSource::CcRelative { .. }))
        && !c.debug_feedback_override;
    if mirror {
        if let Some(slot) = c.out_values.get_mut(idx) {
            *slot = stored;
        }
    }

    // Backfill the matched index on the most recent log entry so the debug
    // panel can show "→ Fader 3" next to the raw bytes.
    if let Some(last) = c.midi_log.back_mut() {
        last.matched_input_idx = Some(idx);
    }

    // Record for the "jump to touched row" highlight.
    if c.debug_highlight_on_touch {
        c.last_match_idx = Some(idx);
        c.last_match_instant = Some(std::time::Instant::now());
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
/// Returns None when the source kind doesn't accept feedback (e.g. relative
/// encoders — the host can't push a value back that way).
fn encode_midi(source: Option<&InputSource>, value: f32) -> Option<[u8; 3]> {
    let src = source?;
    let v = value.clamp(0.0, 1.0);
    match src {
        InputSource::Midi(MidiSource::Cc { channel, controller }) => {
            let status = 0xB0 | ((*channel - 1) & 0x0F);
            let data = (v * 127.0).round() as u8;
            Some([status, *controller & 0x7F, data & 0x7F])
        }
        InputSource::Midi(MidiSource::CcRelative { .. }) => None,
        InputSource::Midi(MidiSource::Note { channel, note }) => {
            let status = 0x90 | ((*channel - 1) & 0x0F);
            let vel = if v >= 0.5 { 127 } else { 0 };
            Some([status, *note & 0x7F, vel])
        }
        InputSource::Midi(MidiSource::NoteVelocity { channel, note }) => {
            // Used by Push pads for LED color lighting — velocity is a 0..127
            // palette index / brightness, pass through scaled.
            let status = 0x90 | ((*channel - 1) & 0x0F);
            let vel = (v * 127.0).round() as u8;
            Some([status, *note & 0x7F, vel & 0x7F])
        }
        InputSource::Midi(MidiSource::PitchBend { channel }) => {
            let status = 0xE0 | ((*channel - 1) & 0x0F);
            let combined = (v * 16383.0).round() as u16;
            Some([status, (combined & 0x7F) as u8, ((combined >> 7) & 0x7F) as u8])
        }
    }
}
