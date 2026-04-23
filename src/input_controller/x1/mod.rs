//! Native Instruments Kontrol X1 (Mk1) support.
//!
//! The X1 is a USB HID controller (not MIDI), so it bypasses midir entirely
//! and talks to rusb directly. An `X1Session` owns one worker thread per
//! device: the thread connects to the X1 over USB, polls input reports,
//! translates them into writes on `ControllerRuntime::values`, and pushes
//! `out_values` back as LED brightness.
//!
//! Layout is fixed and fully mapped by default — no learn mode. Every
//! button / encoder / pot on the device corresponds to one `LearnedInput`
//! whose source is an `X1Source` variant.

mod controller;
mod session;
mod state;

use serde::{Deserialize, Serialize};

pub use session::X1Session;

use super::{InputBindingMode, InputSource, LearnedInput};

/// Source variant for X1 events, parallel to `MidiSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum X1Source {
    Button(X1ButtonId),
    Encoder(X1EncoderId),
    Pot(X1PotId),
}

impl X1Source {
    pub fn is_binary(&self) -> bool { matches!(self, X1Source::Button(_)) }

    pub fn label(&self) -> String {
        match self {
            X1Source::Button(id) => format!("Btn {}", button_name(*id)),
            X1Source::Encoder(id) => format!("Enc {}", encoder_name(*id)),
            X1Source::Pot(id) => format!("Pot {}", pot_name(*id)),
        }
    }
}

/// Mirror of `controller::ButtonId` decoupled from the upstream enum for
/// `Serialize` / `Deserialize` (controller.rs is a copy of an external
/// library and we don't want to touch its derives).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum X1ButtonId {
    Deck1On, Deck2On,
    Deck1_1, Deck2_1, Deck1_2, Deck2_2, Deck1_3, Deck2_3,
    Deck1EncLoad, Deck2EncLoad,
    Shift,
    Deck1Fx1, Deck1Fx2, Deck2Fx1, Deck2Fx2,
    Deck1EncLoop, Deck2EncLoop,
    Hotcue,
    Deck1In, Deck1Out, Deck2In, Deck2Out,
    Deck1BeatLeft, Deck1BeatRight, Deck2BeatLeft, Deck2BeatRight,
    Deck1CueRel, Deck1CupAbs, Deck2CueRel, Deck2CupAbs,
    Deck1Play, Deck1Sync, Deck2Play, Deck2Sync,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum X1EncoderId {
    Deck1Browse, Deck2Browse, Deck1Loop, Deck2Loop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum X1PotId {
    Deck1DryWet, Deck1_1, Deck1_2, Deck1_3,
    Deck2DryWet, Deck2_1, Deck2_2, Deck2_3,
}

pub fn button_name(id: X1ButtonId) -> &'static str {
    match id {
        X1ButtonId::Deck1On => "D1 On",
        X1ButtonId::Deck2On => "D2 On",
        X1ButtonId::Deck1_1 => "D1 1", X1ButtonId::Deck1_2 => "D1 2", X1ButtonId::Deck1_3 => "D1 3",
        X1ButtonId::Deck2_1 => "D2 1", X1ButtonId::Deck2_2 => "D2 2", X1ButtonId::Deck2_3 => "D2 3",
        X1ButtonId::Deck1EncLoad => "D1 Enc Load", X1ButtonId::Deck2EncLoad => "D2 Enc Load",
        X1ButtonId::Shift => "Shift",
        X1ButtonId::Deck1Fx1 => "D1 Fx1", X1ButtonId::Deck1Fx2 => "D1 Fx2",
        X1ButtonId::Deck2Fx1 => "D2 Fx1", X1ButtonId::Deck2Fx2 => "D2 Fx2",
        X1ButtonId::Deck1EncLoop => "D1 Enc Loop", X1ButtonId::Deck2EncLoop => "D2 Enc Loop",
        X1ButtonId::Hotcue => "Hotcue",
        X1ButtonId::Deck1In => "D1 In", X1ButtonId::Deck1Out => "D1 Out",
        X1ButtonId::Deck2In => "D2 In", X1ButtonId::Deck2Out => "D2 Out",
        X1ButtonId::Deck1BeatLeft => "D1 Beat ◀", X1ButtonId::Deck1BeatRight => "D1 Beat ▶",
        X1ButtonId::Deck2BeatLeft => "D2 Beat ◀", X1ButtonId::Deck2BeatRight => "D2 Beat ▶",
        X1ButtonId::Deck1CueRel => "D1 Cue", X1ButtonId::Deck1CupAbs => "D1 Cup",
        X1ButtonId::Deck2CueRel => "D2 Cue", X1ButtonId::Deck2CupAbs => "D2 Cup",
        X1ButtonId::Deck1Play => "D1 Play", X1ButtonId::Deck1Sync => "D1 Sync",
        X1ButtonId::Deck2Play => "D2 Play", X1ButtonId::Deck2Sync => "D2 Sync",
    }
}

pub fn encoder_name(id: X1EncoderId) -> &'static str {
    match id {
        X1EncoderId::Deck1Browse => "D1 Browse", X1EncoderId::Deck2Browse => "D2 Browse",
        X1EncoderId::Deck1Loop => "D1 Loop", X1EncoderId::Deck2Loop => "D2 Loop",
    }
}

pub fn pot_name(id: X1PotId) -> &'static str {
    match id {
        X1PotId::Deck1DryWet => "D1 Dry/Wet",
        X1PotId::Deck1_1 => "D1 Pot 1", X1PotId::Deck1_2 => "D1 Pot 2", X1PotId::Deck1_3 => "D1 Pot 3",
        X1PotId::Deck2DryWet => "D2 Dry/Wet",
        X1PotId::Deck2_1 => "D2 Pot 1", X1PotId::Deck2_2 => "D2 Pot 2", X1PotId::Deck2_3 => "D2 Pot 3",
    }
}

/// Per-button LED slot on the X1 hardware. The LED USB packet is 32 bytes:
/// byte 0 is a report-type header (0x0C) and must not be written as an LED,
/// so valid LED slots are 1..=31. The slot-to-button layout on the Mk1 is
/// undocumented; this table was reverse-engineered by probing every slot
/// against the physical LEDs. `None` means the button has no dedicated
/// LED on the Mk1 (FX1/2, Enc Load/Loop, Shift, Hotcue) — button events
/// still reach the graph, there's just no feedback to drive.
pub fn button_led_index(id: X1ButtonId) -> Option<usize> {
    use X1ButtonId::*;
    let idx = match id {
        // Top row, slots 1..8 — physical right-to-left, Deck 2 side first.
        Deck2_3 => 1,  Deck2_2 => 2,  Deck2_1 => 3,  Deck2On => 4,
        Deck1_3 => 5,  Deck1_2 => 6,  Deck1_1 => 7,  Deck1On => 8,
        // Deck 2 transport, slots 9..16 — right-to-left from the far edge
        // of the controller toward the center.
        Deck2Sync => 9,       Deck2Play => 10,
        Deck2CupAbs => 11,    Deck2CueRel => 12,
        Deck2BeatRight => 13, Deck2BeatLeft => 14,
        Deck2Out => 15,       Deck2In => 16,
        // Deck 1 transport, slots 17..24 — mirrors Deck 2 across the
        // device centerline.
        Deck1Out => 17,       Deck1In => 18,
        Deck1BeatRight => 19, Deck1BeatLeft => 20,
        Deck1CupAbs => 21,    Deck1CueRel => 22,
        Deck1Sync => 23,      Deck1Play => 24,
        // FX buttons + Shift + Hotcue — 6 LEDs in slots 25..30 (tentative;
        // the exact slot→button pairing still needs hardware verification).
        Deck1Fx1 => 25,  Deck1Fx2 => 26,
        Deck2Fx1 => 27,  Deck2Fx2 => 28,
        Shift    => 29,  Hotcue   => 30,
        // Encoders don't have LEDs on the Mk1.
        Deck1EncLoad | Deck2EncLoad
            | Deck1EncLoop | Deck2EncLoop => return None,
    };
    Some(idx)
}

pub const ALL_BUTTONS: &[X1ButtonId] = &[
    X1ButtonId::Deck1On, X1ButtonId::Deck2On,
    X1ButtonId::Deck1_1, X1ButtonId::Deck1_2, X1ButtonId::Deck1_3,
    X1ButtonId::Deck2_1, X1ButtonId::Deck2_2, X1ButtonId::Deck2_3,
    X1ButtonId::Deck1EncLoad, X1ButtonId::Deck2EncLoad,
    X1ButtonId::Shift,
    X1ButtonId::Deck1Fx1, X1ButtonId::Deck1Fx2,
    X1ButtonId::Deck2Fx1, X1ButtonId::Deck2Fx2,
    X1ButtonId::Deck1EncLoop, X1ButtonId::Deck2EncLoop,
    X1ButtonId::Hotcue,
    X1ButtonId::Deck1In, X1ButtonId::Deck1Out,
    X1ButtonId::Deck2In, X1ButtonId::Deck2Out,
    X1ButtonId::Deck1BeatLeft, X1ButtonId::Deck1BeatRight,
    X1ButtonId::Deck2BeatLeft, X1ButtonId::Deck2BeatRight,
    X1ButtonId::Deck1CueRel, X1ButtonId::Deck1CupAbs,
    X1ButtonId::Deck2CueRel, X1ButtonId::Deck2CupAbs,
    X1ButtonId::Deck1Play, X1ButtonId::Deck1Sync,
    X1ButtonId::Deck2Play, X1ButtonId::Deck2Sync,
];

pub const ALL_ENCODERS: &[X1EncoderId] = &[
    X1EncoderId::Deck1Browse, X1EncoderId::Deck2Browse,
    X1EncoderId::Deck1Loop, X1EncoderId::Deck2Loop,
];

pub const ALL_POTS: &[X1PotId] = &[
    X1PotId::Deck1DryWet, X1PotId::Deck1_1, X1PotId::Deck1_2, X1PotId::Deck1_3,
    X1PotId::Deck2DryWet, X1PotId::Deck2_1, X1PotId::Deck2_2, X1PotId::Deck2_3,
];

/// Build the canonical X1 input list. Every physical control gets one entry.
/// IDs start at `start_id` and are dense/contiguous.
pub fn x1_preset_inputs(start_id: u32) -> Vec<LearnedInput> {
    let mut inputs = Vec::with_capacity(ALL_BUTTONS.len() + ALL_ENCODERS.len() + ALL_POTS.len());
    let mut next = start_id;
    for &b in ALL_BUTTONS {
        inputs.push(LearnedInput {
            id: next,
            name: format!("Btn {}", button_name(b)),
            source: InputSource::X1(X1Source::Button(b)),
            mode: InputBindingMode::Value,
            disable_feedback: false,
        });
        next += 1;
    }
    for &e in ALL_ENCODERS {
        inputs.push(LearnedInput {
            id: next,
            name: format!("Enc {}", encoder_name(e)),
            source: InputSource::X1(X1Source::Encoder(e)),
            mode: InputBindingMode::Value,
            disable_feedback: true, // encoders have no LED feedback
        });
        next += 1;
    }
    for &p in ALL_POTS {
        inputs.push(LearnedInput {
            id: next,
            name: format!("Pot {}", pot_name(p)),
            source: InputSource::X1(X1Source::Pot(p)),
            mode: InputBindingMode::Value,
            disable_feedback: true, // pots have no LED feedback
        });
        next += 1;
    }
    inputs
}

/// Cross-file mapping helpers so the engine node can convert upstream
/// `controller::*Id` values into our persistent `X1Source` variants.
pub mod ids {
    use super::{X1ButtonId, X1EncoderId, X1PotId};
    use super::controller::{ButtonId, EncoderId, PotId};

    pub fn button_from_controller(b: ButtonId) -> X1ButtonId {
        match b {
            ButtonId::Deck1On => X1ButtonId::Deck1On,
            ButtonId::Deck2On => X1ButtonId::Deck2On,
            ButtonId::Deck1_1 => X1ButtonId::Deck1_1,
            ButtonId::Deck2_1 => X1ButtonId::Deck2_1,
            ButtonId::Deck1_2 => X1ButtonId::Deck1_2,
            ButtonId::Deck2_2 => X1ButtonId::Deck2_2,
            ButtonId::Deck1_3 => X1ButtonId::Deck1_3,
            ButtonId::Deck2_3 => X1ButtonId::Deck2_3,
            ButtonId::Deck1EncLoad => X1ButtonId::Deck1EncLoad,
            ButtonId::Shift => X1ButtonId::Shift,
            ButtonId::Deck2EncLoad => X1ButtonId::Deck2EncLoad,
            ButtonId::Deck1Fx1 => X1ButtonId::Deck1Fx1,
            ButtonId::Deck1Fx2 => X1ButtonId::Deck1Fx2,
            ButtonId::Deck2Fx1 => X1ButtonId::Deck2Fx1,
            ButtonId::Deck2Fx2 => X1ButtonId::Deck2Fx2,
            ButtonId::Deck1EncLoop => X1ButtonId::Deck1EncLoop,
            ButtonId::Hotcue => X1ButtonId::Hotcue,
            ButtonId::Deck2EncLoop => X1ButtonId::Deck2EncLoop,
            ButtonId::Deck1In => X1ButtonId::Deck1In,
            ButtonId::Deck1Out => X1ButtonId::Deck1Out,
            ButtonId::Deck2In => X1ButtonId::Deck2In,
            ButtonId::Deck2Out => X1ButtonId::Deck2Out,
            ButtonId::Deck1BeatLeft => X1ButtonId::Deck1BeatLeft,
            ButtonId::Deck1BeatRight => X1ButtonId::Deck1BeatRight,
            ButtonId::Deck2BeatLeft => X1ButtonId::Deck2BeatLeft,
            ButtonId::Deck2BeatRight => X1ButtonId::Deck2BeatRight,
            ButtonId::Deck1CueRel => X1ButtonId::Deck1CueRel,
            ButtonId::Deck1CupAbs => X1ButtonId::Deck1CupAbs,
            ButtonId::Deck2CueRel => X1ButtonId::Deck2CueRel,
            ButtonId::Deck2CupAbs => X1ButtonId::Deck2CupAbs,
            ButtonId::Deck1Play => X1ButtonId::Deck1Play,
            ButtonId::Deck1Sync => X1ButtonId::Deck1Sync,
            ButtonId::Deck2Play => X1ButtonId::Deck2Play,
            ButtonId::Deck2Sync => X1ButtonId::Deck2Sync,
        }
    }

    pub fn encoder_from_controller(e: EncoderId) -> X1EncoderId {
        match e {
            EncoderId::Deck1Browse => X1EncoderId::Deck1Browse,
            EncoderId::Deck2Browse => X1EncoderId::Deck2Browse,
            EncoderId::Deck1Loop => X1EncoderId::Deck1Loop,
            EncoderId::Deck2Loop => X1EncoderId::Deck2Loop,
        }
    }

    pub fn pot_from_controller(p: PotId) -> X1PotId {
        match p {
            PotId::Deck1DryWet => X1PotId::Deck1DryWet,
            PotId::Deck1_1 => X1PotId::Deck1_1,
            PotId::Deck1_2 => X1PotId::Deck1_2,
            PotId::Deck1_3 => X1PotId::Deck1_3,
            PotId::Deck2DryWet => X1PotId::Deck2DryWet,
            PotId::Deck2_1 => X1PotId::Deck2_1,
            PotId::Deck2_2 => X1PotId::Deck2_2,
            PotId::Deck2_3 => X1PotId::Deck2_3,
        }
    }
}
