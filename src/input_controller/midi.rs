//! MIDI backend for input controllers using `midir`.

use serde::{Deserialize, Serialize};

use midir::{MidiInput, MidiInputConnection};

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

pub struct MidiSession {
    pub controller_id: u32,
    pub port_name: String,
    /// Held to keep the connection alive; explicit drop on session drop.
    _conn: MidiInputConnection<()>,
}

impl MidiSession {
    pub fn open(
        controller_id: u32,
        port_name: String,
        shared: SharedControllers,
    ) -> Result<Self, String> {
        let mut input = MidiInput::new(&format!("LightBeat-{}", controller_id))
            .map_err(|e| e.to_string())?;
        input.ignore(midir::Ignore::None);

        let port = input.ports().into_iter()
            .find(|p| input.port_name(p).map(|n| n == port_name).unwrap_or(false))
            .ok_or_else(|| format!("port '{}' not found", port_name))?;

        let port_name_for_cb = port_name.clone();
        let shared_for_cb = shared.clone();
        let conn = input.connect(
            &port,
            "lightbeat-input",
            move |_ts, msg, _| {
                handle_midi_message(controller_id, msg, &shared_for_cb);
                let _ = port_name_for_cb;
            },
            (),
        ).map_err(|e| e.to_string())?;

        Ok(Self { controller_id, port_name, _conn: conn })
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
        // Cap buffer size to avoid unbounded growth if UI doesn't poll.
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
    }
}

/// Decode a 1-3 byte status message into `(MidiSource, value 0..1)`.
/// Returns None for messages we don't care about.
fn parse_midi(msg: &[u8]) -> Option<(MidiSource, f32)> {
    if msg.is_empty() { return None; }
    let status = msg[0] & 0xF0;
    let channel = (msg[0] & 0x0F) + 1; // 1..=16

    match status {
        0x90 => {
            // Note On (0 velocity = note off).
            let note = *msg.get(1)?;
            let vel = *msg.get(2)?;
            let value = if vel > 0 { 1.0 } else { 0.0 };
            Some((MidiSource::Note { channel, note }, value))
        }
        0x80 => {
            // Note Off.
            let note = *msg.get(1)?;
            Some((MidiSource::Note { channel, note }, 0.0))
        }
        0xB0 => {
            // CC.
            let cc = *msg.get(1)?;
            let val = *msg.get(2)?;
            Some((MidiSource::Cc { channel, controller: cc }, val as f32 / 127.0))
        }
        0xE0 => {
            // Pitch bend (14-bit, lsb-msb).
            let lsb = *msg.get(1)? as u16;
            let msb = *msg.get(2)? as u16;
            let combined = (msb << 7) | lsb; // 0..=16383
            Some((MidiSource::PitchBend { channel }, combined as f32 / 16383.0))
        }
        _ => None,
    }
}
