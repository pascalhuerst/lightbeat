use std::time::{Duration, Instant};

use rusb::{Context, DeviceHandle, Error, UsbContext};

use super::state::X1State;

/// Timestamp used for controller events.
pub type Timestamp = Instant;

/// Classification for button state transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonEventKind {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
}

impl Modifiers {
    fn from_state(state: &X1State) -> Self {
        Self {
            shift: state.button_shift,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonId {
    Deck1On,
    Deck2On,
    Deck1_1,
    Deck2_1,
    Deck1_2,
    Deck2_2,
    Deck1_3,
    Deck2_3,
    Deck1EncLoad,
    Shift,
    Deck2EncLoad,
    Deck1Fx1,
    Deck1Fx2,
    Deck2Fx1,
    Deck2Fx2,
    Deck1EncLoop,
    Hotcue,
    Deck2EncLoop,
    Deck1In,
    Deck1Out,
    Deck2In,
    Deck2Out,
    Deck1BeatLeft,
    Deck1BeatRight,
    Deck2BeatLeft,
    Deck2BeatRight,
    Deck1CueRel,
    Deck1CupAbs,
    Deck2CueRel,
    Deck2CupAbs,
    Deck1Play,
    Deck1Sync,
    Deck2Play,
    Deck2Sync,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderId {
    Deck1Browse,
    Deck2Browse,
    Deck1Loop,
    Deck2Loop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PotId {
    Deck1DryWet,
    Deck1_1,
    Deck1_2,
    Deck1_3,
    Deck2DryWet,
    Deck2_1,
    Deck2_2,
    Deck2_3,
}

/// Information about a button transition.
#[derive(Debug, Clone, Copy)]
pub struct ButtonEvent {
    pub id: ButtonId,
    pub kind: ButtonEventKind,
    pub modifiers: Modifiers,
}

/// Information about an encoder value change.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct EncoderEvent {
    pub id: EncoderId,
    pub value: u8,
    pub previous: u8,
    pub modifiers: Modifiers,
}

/// Information about a potentiometer value change.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct PotEvent {
    pub id: PotId,
    pub value: u16,
    pub previous: u16,
    pub modifiers: Modifiers,
}

/// High-level interface for working with the X1 controller.
///
/// The controller polls the USB endpoint, updates LED feedback, and
/// notifies registered callbacks when buttons, encoders, or pots change.
pub struct X1Controller {
    _context: Context,
    handle: DeviceHandle<Context>,
    input_buf: [u8; 24],
    timeout: Duration,
    leds: [u8; 32],
    leds_dirty: bool,
    last_state: X1State,
    initialized: bool,
    button_callback:
        Option<Box<dyn FnMut(&X1State, ButtonEvent, Timestamp, &mut LedHandle) + Send + 'static>>,
    encoder_callback:
        Option<Box<dyn FnMut(&X1State, EncoderEvent, Timestamp, &mut LedHandle) + Send + 'static>>,
    pot_callback:
        Option<Box<dyn FnMut(&X1State, PotEvent, Timestamp, &mut LedHandle) + Send + 'static>>,
}

#[allow(dead_code)]
pub struct LedHandle<'a> {
    leds: &'a mut [u8; 32],
    dirty: &'a mut bool,
}

impl<'a> LedHandle<'a> {
    fn new(leds: &'a mut [u8; 32], dirty: &'a mut bool) -> Self {
        Self { leds, dirty }
    }

    #[allow(dead_code)]
    pub fn set_raw(&mut self, idx: usize, value: u8) {
        if let Some(slot) = self.leds.get_mut(idx) {
            *slot = value;
            *self.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub fn set_pressed(&mut self, idx: usize, pressed: bool) {
        let value = if pressed { LED_BRIGHT } else { LED_DIM };
        self.set_raw(idx, value);
    }
}

impl X1Controller {
    /// Connect to the first Kontrol X1 Mk1 discovered on the USB bus.
    pub fn connect() -> rusb::Result<Self> {
        let context = Context::new()?;
        let mut handle = None;

        for device in context.devices()?.iter() {
            let desc = device.device_descriptor()?;
            if desc.vendor_id() == 0x17cc && desc.product_id() == 0x2305 {
                handle = Some(device.open()?);
                break;
            }
        }

        let handle = match handle {
            Some(h) => h,
            None => {
                eprintln!("No X1 controller found.");
                return Err(Error::NoDevice);
            }
        };

        handle.set_active_configuration(1)?;
        handle.claim_interface(0)?;
        handle.set_alternate_setting(0, 0)?;

        let mut leds = [LED_DIM; 32];
        leds[0] = 0x0C;
        leds[31] = LED_DIM;

        Ok(Self {
            _context: context,
            handle,
            input_buf: [0; 24],
            timeout: Duration::from_millis(50),
            leds,
            leds_dirty: true,
            last_state: X1State::default(),
            initialized: false,
            button_callback: None,
            encoder_callback: None,
            pot_callback: None,
        })
    }

    /// Enter the controller's polling loop. This blocks until an error occurs.
    #[allow(dead_code)]
    pub fn run(mut self) -> rusb::Result<()> {
        loop {
            self.poll_once()?;
        }
    }

    /// Perform a single USB poll, firing callbacks as needed.
    pub fn poll_once(&mut self) -> rusb::Result<()> {
        match self
            .handle
            .read_bulk(USB_READ_ENDPOINT, &mut self.input_buf, self.timeout)
        {
            Ok(len) if len == self.input_buf.len() => {
                let state = X1State::from_buf(&self.input_buf);
                let now = Instant::now();

                if !self.initialized {
                    self.last_state = state;
                    self.initialized = true;
                    self.leds_dirty = true;
                    self.flush_leds()?;
                    return Ok(());
                }

                self.handle_button_changes(&state, now);
                self.handle_encoder_changes(&state, now);
                self.handle_pot_changes(&state, now);

                self.flush_leds()?;
                self.last_state = state;
            }
            Ok(_) => {}
            Err(Error::Timeout) => {}
            Err(err) => return Err(err),
        }
        Ok(())
    }

    /// Install a callback to be notified about button state transitions.
    pub fn set_button_callback<F>(&mut self, callback: F)
    where
        F: FnMut(&X1State, ButtonEvent, Timestamp, &mut LedHandle) + Send + 'static,
    {
        self.button_callback = Some(Box::new(callback));
    }

    /// Install a callback to be notified about encoder value changes.
    #[allow(dead_code)]
    pub fn set_encoder_callback<F>(&mut self, callback: F)
    where
        F: FnMut(&X1State, EncoderEvent, Timestamp, &mut LedHandle) + Send + 'static,
    {
        self.encoder_callback = Some(Box::new(callback));
    }

    #[allow(dead_code)]
    pub fn set_pot_callback<F>(&mut self, callback: F)
    where
        F: FnMut(&X1State, PotEvent, Timestamp, &mut LedHandle) + Send + 'static,
    {
        self.pot_callback = Some(Box::new(callback));
    }

    /// Remove all registered callbacks.
    #[allow(dead_code)]
    pub fn clear_callbacks(&mut self) {
        self.button_callback = None;
        self.encoder_callback = None;
        self.pot_callback = None;
    }

    pub fn set_led_raw(&mut self, idx: usize, value: u8) {
        if let Some(slot) = self.leds.get_mut(idx) {
            *slot = value;
            self.leds_dirty = true;
        }
    }

    #[allow(dead_code)]
    pub fn set_led_pressed(&mut self, idx: usize, pressed: bool) {
        let value = if pressed { LED_BRIGHT } else { LED_DIM };
        self.set_led_raw(idx, value);
    }

    #[allow(dead_code)]
    pub fn last_state(&self) -> &X1State {
        &self.last_state
    }

    fn handle_button_changes(&mut self, state: &X1State, now: Instant) {
        self.emit_button(
            state,
            now,
            ButtonId::Deck1On,
            state.button_deck1_on,
            self.last_state.button_deck1_on,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2On,
            state.button_deck2_on,
            self.last_state.button_deck2_on,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck1_1,
            state.button_deck1_1,
            self.last_state.button_deck1_1,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2_1,
            state.button_deck2_1,
            self.last_state.button_deck2_1,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck1_2,
            state.button_deck1_2,
            self.last_state.button_deck1_2,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2_2,
            state.button_deck2_2,
            self.last_state.button_deck2_2,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck1_3,
            state.button_deck1_3,
            self.last_state.button_deck1_3,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2_3,
            state.button_deck2_3,
            self.last_state.button_deck2_3,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck1EncLoad,
            state.button_deck1_enc_load,
            self.last_state.button_deck1_enc_load,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Shift,
            state.button_shift,
            self.last_state.button_shift,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2EncLoad,
            state.button_deck2_enc_load,
            self.last_state.button_deck2_enc_load,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck1Fx1,
            state.button_deck1_fx1,
            self.last_state.button_deck1_fx1,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck1Fx2,
            state.button_deck1_fx2,
            self.last_state.button_deck1_fx2,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2Fx1,
            state.button_deck2_fx1,
            self.last_state.button_deck2_fx1,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2Fx2,
            state.button_deck2_fx2,
            self.last_state.button_deck2_fx2,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck1EncLoop,
            state.button_deck1_enc_loop,
            self.last_state.button_deck1_enc_loop,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Hotcue,
            state.button_hotcue,
            self.last_state.button_hotcue,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2EncLoop,
            state.button_deck2_enc_loop,
            self.last_state.button_deck2_enc_loop,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck1In,
            state.button_deck1_in,
            self.last_state.button_deck1_in,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck1Out,
            state.button_deck1_out,
            self.last_state.button_deck1_out,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2In,
            state.button_deck2_in,
            self.last_state.button_deck2_in,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2Out,
            state.button_deck2_out,
            self.last_state.button_deck2_out,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck1BeatLeft,
            state.button_deck1_beat_left,
            self.last_state.button_deck1_beat_left,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck1BeatRight,
            state.button_deck1_beat_right,
            self.last_state.button_deck1_beat_right,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2BeatLeft,
            state.button_deck2_beat_left,
            self.last_state.button_deck2_beat_left,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2BeatRight,
            state.button_deck2_beat_right,
            self.last_state.button_deck2_beat_right,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck1CueRel,
            state.button_deck1_cue_rel,
            self.last_state.button_deck1_cue_rel,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck1CupAbs,
            state.button_deck1_cup_abs,
            self.last_state.button_deck1_cup_abs,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2CueRel,
            state.button_deck2_cue_rel,
            self.last_state.button_deck2_cue_rel,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2CupAbs,
            state.button_deck2_cup_abs,
            self.last_state.button_deck2_cup_abs,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck1Play,
            state.button_deck1_play,
            self.last_state.button_deck1_play,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck1Sync,
            state.button_deck1_sync,
            self.last_state.button_deck1_sync,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2Play,
            state.button_deck2_play,
            self.last_state.button_deck2_play,
        );
        self.emit_button(
            state,
            now,
            ButtonId::Deck2Sync,
            state.button_deck2_sync,
            self.last_state.button_deck2_sync,
        );
    }

    fn handle_encoder_changes(&mut self, state: &X1State, now: Instant) {
        self.emit_encoder(
            state,
            now,
            EncoderId::Deck1Browse,
            state.encoder_deck1_browse,
            self.last_state.encoder_deck1_browse,
        );
        self.emit_encoder(
            state,
            now,
            EncoderId::Deck2Browse,
            state.encoder_deck2_browse,
            self.last_state.encoder_deck2_browse,
        );
        self.emit_encoder(
            state,
            now,
            EncoderId::Deck1Loop,
            state.encoder_deck1_loop,
            self.last_state.encoder_deck1_loop,
        );
        self.emit_encoder(
            state,
            now,
            EncoderId::Deck2Loop,
            state.encoder_deck2_loop,
            self.last_state.encoder_deck2_loop,
        );
    }

    fn handle_pot_changes(&mut self, state: &X1State, now: Instant) {
        self.emit_pot(
            state,
            now,
            PotId::Deck1DryWet,
            state.pot_deck1_dry_wet,
            self.last_state.pot_deck1_dry_wet,
        );
        self.emit_pot(
            state,
            now,
            PotId::Deck1_1,
            state.pot_deck1_1,
            self.last_state.pot_deck1_1,
        );
        self.emit_pot(
            state,
            now,
            PotId::Deck1_2,
            state.pot_deck1_2,
            self.last_state.pot_deck1_2,
        );
        self.emit_pot(
            state,
            now,
            PotId::Deck1_3,
            state.pot_deck1_3,
            self.last_state.pot_deck1_3,
        );
        self.emit_pot(
            state,
            now,
            PotId::Deck2DryWet,
            state.pot_deck2_dry_wet,
            self.last_state.pot_deck2_dry_wet,
        );
        self.emit_pot(
            state,
            now,
            PotId::Deck2_1,
            state.pot_deck2_1,
            self.last_state.pot_deck2_1,
        );
        self.emit_pot(
            state,
            now,
            PotId::Deck2_2,
            state.pot_deck2_2,
            self.last_state.pot_deck2_2,
        );
        self.emit_pot(
            state,
            now,
            PotId::Deck2_3,
            state.pot_deck2_3,
            self.last_state.pot_deck2_3,
        );
    }

    fn emit_button(&mut self, state: &X1State, now: Instant, id: ButtonId, new: bool, old: bool) {
        if new == old {
            return;
        }
        if let Some(mut cb) = self.button_callback.take() {
            let kind = if new {
                ButtonEventKind::Pressed
            } else {
                ButtonEventKind::Released
            };
            let modifiers = Modifiers::from_state(state);
            let mut handle = LedHandle::new(&mut self.leds, &mut self.leds_dirty);
            cb(
                state,
                ButtonEvent {
                    id,
                    kind,
                    modifiers,
                },
                now,
                &mut handle,
            );
            self.button_callback = Some(cb);
        }
    }

    fn emit_encoder(&mut self, state: &X1State, now: Instant, id: EncoderId, new: u8, old: u8) {
        if new == old {
            return;
        }
        if let Some(mut cb) = self.encoder_callback.take() {
            let modifiers = Modifiers::from_state(state);
            let mut handle = LedHandle::new(&mut self.leds, &mut self.leds_dirty);
            cb(
                state,
                EncoderEvent {
                    id,
                    value: new,
                    previous: old,
                    modifiers,
                },
                now,
                &mut handle,
            );
            self.encoder_callback = Some(cb);
        }
    }

    fn emit_pot(&mut self, state: &X1State, now: Instant, id: PotId, new: u16, old: u16) {
        if new == old {
            return;
        }
        if let Some(mut cb) = self.pot_callback.take() {
            let modifiers = Modifiers::from_state(state);
            let mut handle = LedHandle::new(&mut self.leds, &mut self.leds_dirty);
            cb(
                state,
                PotEvent {
                    id,
                    value: new,
                    previous: old,
                    modifiers,
                },
                now,
                &mut handle,
            );
            self.pot_callback = Some(cb);
        }
    }

    pub fn flush_leds(&mut self) -> rusb::Result<()> {
        if !self.leds_dirty {
            return Ok(());
        }

        // Byte 0 of the LED packet is a report-type header (0x0C), not an
        // LED value. Force it back before every write so that a stray
        // `set_led_raw(0, _)` can't brick the whole packet.
        self.leds[0] = 0x0C;

        if let Err(err) = self
            .handle
            .write_bulk(USB_WRITE_ENDPOINT, &self.leds, self.timeout)
        {
            eprintln!("USB write error: {:?}", err);
        } else {
            let mut ack = [0u8; 1];
            match self
                .handle
                .read_bulk(USB_UNLOCK_ENDPOINT, &mut ack, self.timeout)
            {
                Ok(_) | Err(Error::Timeout) => {}
                Err(err) => eprintln!("USB unlock read error: {:?}", err),
            }
            self.leds_dirty = false;
        }

        Ok(())
    }
}

const USB_READ_ENDPOINT: u8 = 0x84;
const USB_WRITE_ENDPOINT: u8 = 0x01;
const USB_UNLOCK_ENDPOINT: u8 = 0x81;
pub const LED_DIM: u8 = 0x05;
pub const LED_BRIGHT: u8 = 0x7F;
