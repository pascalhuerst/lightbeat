#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct X1State {
    // Buttons
    pub button_deck1_on: bool,
    pub button_deck2_on: bool,
    pub button_deck1_1: bool,
    pub button_deck2_1: bool,
    pub button_deck1_2: bool,
    pub button_deck2_2: bool,
    pub button_deck1_3: bool,
    pub button_deck2_3: bool,
    pub button_deck1_enc_load: bool,
    pub button_shift: bool,
    pub button_deck2_enc_load: bool,
    pub button_deck1_fx1: bool,
    pub button_deck1_fx2: bool,
    pub button_deck2_fx1: bool,
    pub button_deck2_fx2: bool,
    pub button_deck1_enc_loop: bool,
    pub button_hotcue: bool,
    pub button_deck2_enc_loop: bool,
    pub button_deck1_in: bool,
    pub button_deck1_out: bool,
    pub button_deck2_in: bool,
    pub button_deck2_out: bool,
    pub button_deck1_beat_left: bool,
    pub button_deck1_beat_right: bool,
    pub button_deck2_beat_left: bool,
    pub button_deck2_beat_right: bool,
    pub button_deck1_cue_rel: bool,
    pub button_deck1_cup_abs: bool,
    pub button_deck2_cue_rel: bool,
    pub button_deck2_cup_abs: bool,
    pub button_deck1_play: bool,
    pub button_deck1_sync: bool,
    pub button_deck2_play: bool,
    pub button_deck2_sync: bool,
    // Encoders (each nibble is 0..15)
    pub encoder_deck1_browse: u8,
    pub encoder_deck2_browse: u8,
    pub encoder_deck1_loop: u8,
    pub encoder_deck2_loop: u8,
    pub encoders: [u8; 4],
    // Named pots
    pub pot_deck1_dry_wet: u16,
    pub pot_deck1_1: u16,
    pub pot_deck1_2: u16,
    pub pot_deck1_3: u16,
    pub pot_deck2_dry_wet: u16,
    pub pot_deck2_1: u16,
    pub pot_deck2_2: u16,
    pub pot_deck2_3: u16,
    pub pots: [u16; 8], // keep for raw access if needed
}

impl X1State {
    /// Parse controller state directly from the raw USB input buffer.
    pub fn from_buf(buf: &[u8; 24]) -> Self {
        let mut pots_raw = [0u16; 8];
        for i in 0..8 {
            let idx = 8 + i * 2;
            if idx + 1 < buf.len() {
                pots_raw[i] = u16::from_be_bytes([buf[idx], buf[idx + 1]]);
            }
        }

        let bit = |group: usize, offset: u8| -> bool {
            if let Some(byte) = buf.get(1 + group) {
                if let Some(mask) = (1u8).checked_shl(offset as u32) {
                    return (*byte & mask) != 0;
                }
            }
            false
        };

        let nibble = |index: usize, high: bool| -> u8 {
            buf.get(index)
                .map(|byte| if high { byte >> 4 } else { byte & 0x0F })
                .unwrap_or(0)
        };

        let enc = [
            nibble(6, false),
            nibble(6, true),
            nibble(7, false),
            nibble(7, true),
        ];

        Self {
            button_deck1_on: bit(3, 4),
            button_deck2_on: bit(4, 0),
            button_deck1_1: bit(3, 5),
            button_deck2_1: bit(4, 1),
            button_deck1_2: bit(3, 6),
            button_deck2_2: bit(4, 2),
            button_deck1_3: bit(3, 7),
            button_deck2_3: bit(4, 3),
            button_deck1_enc_load: bit(3, 0),
            button_shift: bit(4, 4),
            button_deck2_enc_load: bit(3, 1),
            button_deck1_fx1: bit(1, 1),
            button_deck1_fx2: bit(1, 0),
            button_deck2_fx1: bit(4, 6),
            button_deck2_fx2: bit(4, 5),
            button_deck1_enc_loop: bit(3, 2),
            button_hotcue: bit(4, 7),
            button_deck2_enc_loop: bit(3, 3),
            button_deck1_in: bit(2, 4),
            button_deck1_out: bit(0, 3),
            button_deck2_in: bit(1, 4),
            button_deck2_out: bit(2, 3),
            button_deck1_beat_left: bit(0, 2),
            button_deck1_beat_right: bit(2, 5),
            button_deck2_beat_left: bit(2, 2),
            button_deck2_beat_right: bit(1, 5),
            button_deck1_cue_rel: bit(0, 1),
            button_deck1_cup_abs: bit(2, 6),
            button_deck2_cue_rel: bit(2, 1),
            button_deck2_cup_abs: bit(1, 6),
            button_deck1_play: bit(0, 0),
            button_deck1_sync: bit(2, 7),
            button_deck2_play: bit(2, 0),
            button_deck2_sync: bit(1, 7),
            encoder_deck1_browse: enc[0],
            encoder_deck2_browse: enc[1],
            encoder_deck1_loop: enc[2],
            encoder_deck2_loop: enc[3],
            encoders: enc,
            pot_deck1_dry_wet: pots_raw[4],
            pot_deck1_1: pots_raw[6],
            pot_deck1_2: pots_raw[7],
            pot_deck1_3: pots_raw[5],
            pot_deck2_dry_wet: pots_raw[2],
            pot_deck2_1: pots_raw[1],
            pot_deck2_2: pots_raw[0],
            pot_deck2_3: pots_raw[3],
            pots: pots_raw,
        }
    }
}
