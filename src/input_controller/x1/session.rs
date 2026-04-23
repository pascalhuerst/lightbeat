//! Worker thread + USB connection lifecycle for one X1 controller.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::controller::{
    ButtonEventKind, LedHandle, X1Controller, LED_BRIGHT, LED_DIM,
};
use super::ids;
use super::{InputSource, X1Source};
use crate::input_controller::{
    ActivityLogEntry, ConnectionStatus, SharedControllers, ACTIVITY_LOG_CAPACITY,
};

use super::button_led_index;

pub struct X1Session {
    pub controller_id: u32,
    stop: Arc<AtomicBool>,
    _join: Option<JoinHandle<()>>,
}

impl Drop for X1Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self._join.take() {
            let _ = h.join();
        }
    }
}

impl X1Session {
    /// Open the USB connection and spawn the worker. Returns an error if the
    /// device isn't present — the manager will retry on the next reconcile.
    pub fn open(controller_id: u32, shared: SharedControllers) -> Result<Self, String> {
        let mut controller = X1Controller::connect()
            .map_err(|e| format!("X1 USB connect: {:?}", e))?;

        install_callbacks(&mut controller, controller_id, shared.clone());

        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_thread = stop.clone();
        let shared_for_thread = shared;
        let join = thread::Builder::new()
            .name(format!("lightbeat-x1-{}", controller_id))
            .spawn(move || run(controller, controller_id, shared_for_thread, stop_for_thread))
            .map_err(|e| format!("spawn X1 worker: {}", e))?;

        Ok(Self {
            controller_id,
            stop,
            _join: Some(join),
        })
    }
}

/// Install button / encoder / pot callbacks that write into the shared state.
fn install_callbacks(
    controller: &mut X1Controller,
    controller_id: u32,
    shared: SharedControllers,
) {
    let s = shared.clone();
    controller.set_button_callback(move |_state, ev, _ts, _led| {
        let src = InputSource::X1(X1Source::Button(ids::button_from_controller(ev.id)));
        let v = match ev.kind {
            ButtonEventKind::Pressed => 1.0,
            ButtonEventKind::Released => 0.0,
        };
        write_input_value(&s, controller_id, &src, v);
    });

    // X1 encoders are 4-bit wrap counters (0..15). Turning one click in
    // either direction moves the counter by ±1 (mod 16) — useless as an
    // absolute value, great as a delta. We accumulate each click into a
    // per-encoder phase (0..1, wrapping at both ends) so forward rotation
    // eventually loops back to 0 and reverse rotation goes through 1.
    // `ENC_STEP` picks the feel: ~24 clicks per full 0→1 trip, same as
    // Push 1 / BCF relative encoders.
    const ENC_STEP: f32 = 1.0 / 24.0;
    let enc_state: Arc<Mutex<HashMap<super::X1EncoderId, f32>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let s = shared.clone();
    controller.set_encoder_callback(move |_state, ev, _ts, _led| {
        // Shortest-path signed delta mod 16 (-8..=7).
        let prev = ev.previous as i32 & 0x0F;
        let cur = ev.value as i32 & 0x0F;
        let raw = cur - prev;
        let delta = ((raw + 8).rem_euclid(16) - 8) as f32;

        let enc_id = ids::encoder_from_controller(ev.id);
        let v = {
            let mut state = enc_state.lock().unwrap();
            let cur = state.entry(enc_id).or_insert(0.0);
            *cur = (*cur + delta * ENC_STEP).rem_euclid(1.0);
            *cur
        };
        let src = InputSource::X1(X1Source::Encoder(enc_id));
        write_input_value(&s, controller_id, &src, v);
    });

    // Pots on the X1 Mk1 are 12-bit (raw 0..4095); the low couple of bits
    // are analog noise, so emitting every sub-bit change floods the log.
    // Drop the bottom 2 bits (keep 10 — 1024 levels, smoother than 7-bit
    // MIDI) and only forward when the quantized value actually changes
    // per pot.
    const POT_RAW_BITS: u32 = 12;
    const POT_SHIFT: u32 = 3;
    const POT_MASK: u16 = (1 << POT_RAW_BITS) - 1; // 0x0FFF
    const POT_MAX: u16 = (1 << (POT_RAW_BITS - POT_SHIFT)) - 1; // 511
    let pot_last: Arc<Mutex<HashMap<super::X1PotId, u16>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let s = shared.clone();
    controller.set_pot_callback(move |_state, ev, _ts, _led| {
        let pot_id = ids::pot_from_controller(ev.id);
        let raw = ev.value & POT_MASK;
        let quant = raw >> POT_SHIFT;
        {
            let mut last = pot_last.lock().unwrap();
            match last.get(&pot_id) {
                Some(&prev) if prev == quant => return,
                _ => { last.insert(pot_id, quant); }
            }
        }
        let src = InputSource::X1(X1Source::Pot(pot_id));
        let v = quant as f32 / POT_MAX as f32;
        write_input_value(&s, controller_id, &src, v);
    });
}

fn write_input_value(
    shared: &SharedControllers,
    controller_id: u32,
    src: &InputSource,
    value: f32,
) {
    let mut state = shared.lock().unwrap();
    let Some(c) = state.iter_mut().find(|c| c.id == controller_id) else { return };
    let idx = c.inputs.iter().position(|i| &i.source == src);

    // Activity log — no raw bytes, just the decoded event. The debug panel
    // uses this to show what the hardware is sending.
    let entry = ActivityLogEntry {
        raw: None,
        decoded: Some((src.clone(), value)),
        matched_input_idx: idx,
        instant: std::time::Instant::now(),
    };
    c.activity_log.push_back(entry);
    while c.activity_log.len() > ACTIVITY_LOG_CAPACITY {
        c.activity_log.pop_front();
    }

    // Store the value on the matching input row.
    if let Some(idx) = idx {
        if let Some(slot) = c.values.get_mut(idx) {
            *slot = value;
        }
        // "Highlight on touch" — jump the inputs table to this row.
        if c.debug_highlight_on_touch {
            c.last_match_idx = Some(idx);
            c.last_match_instant = Some(std::time::Instant::now());
        }
    }
}

fn run(
    mut controller: X1Controller,
    controller_id: u32,
    shared: SharedControllers,
    stop: Arc<AtomicBool>,
) {
    // Snapshot of last-pushed LED brightness per input index so we only call
    // `set_led_raw` when the value actually changed — avoids hammering the
    // USB endpoint on every iteration.
    let mut last_out: Vec<f32> = Vec::new();

    while !stop.load(Ordering::Relaxed) {
        if let Err(e) = controller.poll_once() {
            // Device removed or USB error — bail so the manager can reopen.
            eprintln!("[x1 {}] poll error: {:?}", controller_id, e);
            // Mark disconnected and exit the loop.
            let mut state = shared.lock().unwrap();
            if let Some(c) = state.iter_mut().find(|c| c.id == controller_id) {
                c.status = ConnectionStatus::Waiting;
            }
            return;
        }

        // Push graph → device LED feedback.
        let (sources, values): (Vec<InputSource>, Vec<f32>) = {
            let state = shared.lock().unwrap();
            match state.iter().find(|c| c.id == controller_id) {
                Some(c) => (
                    c.inputs.iter().map(|i| i.source.clone()).collect(),
                    c.out_values.clone(),
                ),
                None => {
                    stop.store(true, Ordering::Relaxed);
                    return;
                }
            }
        };
        if last_out.len() != values.len() {
            last_out = vec![f32::NAN; values.len()];
        }
        for (i, (src, v)) in sources.iter().zip(values.iter()).enumerate() {
            if (last_out[i] - *v).abs() < 1e-4 { continue; }
            last_out[i] = *v;
            let InputSource::X1(X1Source::Button(b)) = src else { continue };
            let Some(led_idx) = button_led_index(*b) else { continue };
            // Map 0..1 continuously to LED brightness. Anything below 0.02
            // snaps to dim (off-ish) so "0" fully extinguishes.
            let byte = if *v <= 0.02 {
                LED_DIM
            } else if *v >= 0.99 {
                LED_BRIGHT
            } else {
                let span = (LED_BRIGHT - LED_DIM) as f32;
                LED_DIM + (v.clamp(0.0, 1.0) * span) as u8
            };
            controller.set_led_raw(led_idx, byte);
        }

        // Flush pending LED writes to the hardware. `poll_once` also flushes,
        // but only when a full input packet arrives — without this explicit
        // flush, LED changes made while the user isn't touching the device
        // would be staged but never sent.
        if let Err(err) = controller.flush_leds() {
            eprintln!("[x1 {}] LED flush error: {:?}", controller_id, err);
        }

        // The underlying poll already sleeps via USB timeout; no explicit sleep.
        // A very brief yield to let other threads in (e.g. the UI) pick up locks.
        thread::sleep(Duration::from_millis(1));
    }
}

/// Compile-time use to keep `LedHandle` in-scope (the controller exposes it
/// but our callbacks don't touch it — we flush LEDs from the main loop
/// instead so feedback stays driven by `out_values` rather than by input
/// events).
#[allow(dead_code)]
fn _keep_led_handle_referenced(_h: LedHandle) {}
