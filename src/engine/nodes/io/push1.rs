//! Dedicated Push 1 controller node. Binds to an `InputControllerKind::Push1`
//! runtime and exposes grouped, address-based ports instead of one port per
//! physical control.
//!
//! Pads (8x8):
//!   - Outputs: `pad trig` (1-tick pulse on any pad event), `pad x` (0..7
//!     column), `pad y` (0..7 row), `pad vel` (0..1 velocity; 0 means release).
//!   - Inputs: `set pad trig` (rising-edge commit), `set pad x`, `set pad y`,
//!     `set pad color` (0..1 → MIDI velocity 0..127, used as Push 1 palette
//!     index for the pad LED).
//!
//! Top-row buttons (CC 102..109) and bottom-row buttons (CC 20..27) each get
//! the same (trig, col, value) + (set trig, set col, set color) shape.
//!
//! Encoders (11 relatives — tempo / swing / track 1..8 / master): grouped as
//! `enc trig`, `enc idx` (0..10) and `enc value` (accumulated 0..1). No LED
//! feedback — Push 1 encoders have no rings.
//!
//! Touch strip: single 0..1 output.
//!
//! Named transport / nav buttons (Play, Record, Stop, Shift, Select, Up,
//! Down, Left, Right, Oct Up, Oct Down): one output each. Feedback is handled
//! via separate inputs using the same names.

use std::collections::VecDeque;

use crate::engine::types::*;
use crate::input_controller::{InputControllerKind, InputSource, LearnedInput, SharedControllers};
use crate::input_controller::midi::MidiSource;

/// Display state for the widget.
pub struct Push1Display {
    pub controller_id: u32,
    pub controller_name: String,
    pub connected: bool,
}

/// Index buckets pre-computed from the bound controller's input list.
/// Refreshed whenever the bound id changes.
#[derive(Default)]
struct IndexMap {
    /// 64 entries, [row * 8 + col] → index into `values[]`. Row 0 = bottom row.
    pads: Vec<usize>,
    /// 8 entries, [col] → index.
    btn_top: Vec<usize>,
    btn_bot: Vec<usize>,
    /// 11 entries, in the order Tempo, Swing, Track 1..8, Master.
    encoders: Vec<usize>,
    /// Touch strip (pitch bend) index, if present.
    slider: Option<usize>,
    /// Named transport / nav buttons — index or None if not in the runtime.
    play: Option<usize>,
    record: Option<usize>,
    stop: Option<usize>,
    shift: Option<usize>,
    select: Option<usize>,
    up: Option<usize>,
    down: Option<usize>,
    left: Option<usize>,
    right: Option<usize>,
    oct_up: Option<usize>,
    oct_down: Option<usize>,
}

#[derive(Clone, Copy)]
struct PadEvent { col: u8, row: u8, vel: f32 }
#[derive(Clone, Copy)]
struct BtnEvent { col: u8, value: f32 }
#[derive(Clone, Copy)]
struct EncEvent { idx: u8, value: f32 }

const NAMED_BUTTONS: &[&str] = &[
    "Play", "Record", "Stop", "Shift", "Select",
    "Up", "Down", "Left", "Right", "Octave Up", "Octave Down",
];

pub struct Push1ProcessNode {
    id: NodeId,
    controller_id: u32,
    controllers: SharedControllers,

    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,

    idx_map: IndexMap,
    /// Previous snapshot of the bound controller's `values[]`, used for
    /// diff-based event generation.
    prev_values: Vec<f32>,
    /// Event queues. Drained one-per-tick so bursts don't lose events.
    pad_events: VecDeque<PadEvent>,
    btn_top_events: VecDeque<BtnEvent>,
    btn_bot_events: VecDeque<BtnEvent>,
    enc_events: VecDeque<EncEvent>,

    /// Latched output state (what `read_output` returns).
    pad_trig: f32, pad_x: f32, pad_y: f32, pad_vel: f32,
    btn_top_trig: f32, btn_top_col: f32, btn_top_val: f32,
    btn_bot_trig: f32, btn_bot_col: f32, btn_bot_val: f32,
    enc_trig: f32, enc_idx: f32, enc_value: f32,
    slider_value: f32,
    named_values: [f32; 11],

    /// Input-side latches (engine's write_input buffers).
    in_values: Vec<f32>,
    prev_set_pad_trig: f32,
    prev_set_btn_top_trig: f32,
    prev_set_btn_bot_trig: f32,
    prev_named_in: [f32; 11],

    connected: bool,
    controller_name: String,
}

// Input/Output indices. Keep stable even when we grow the schema so
// connections survive.
mod out_idx {
    pub const PAD_TRIG: usize = 0;
    pub const PAD_X: usize = 1;
    pub const PAD_Y: usize = 2;
    pub const PAD_VEL: usize = 3;

    pub const BTN_TOP_TRIG: usize = 4;
    pub const BTN_TOP_COL: usize = 5;
    pub const BTN_TOP_VAL: usize = 6;

    pub const BTN_BOT_TRIG: usize = 7;
    pub const BTN_BOT_COL: usize = 8;
    pub const BTN_BOT_VAL: usize = 9;

    pub const ENC_TRIG: usize = 10;
    pub const ENC_IDX: usize = 11;
    pub const ENC_VAL: usize = 12;

    pub const SLIDER: usize = 13;

    /// 11 named transport/nav buttons start here.
    pub const NAMED_BASE: usize = 14;
    pub const COUNT: usize = NAMED_BASE + 11;
}

mod in_idx {
    pub const SET_PAD_TRIG: usize = 0;
    pub const SET_PAD_X: usize = 1;
    pub const SET_PAD_Y: usize = 2;
    pub const SET_PAD_COLOR: usize = 3;

    pub const SET_BTN_TOP_TRIG: usize = 4;
    pub const SET_BTN_TOP_COL: usize = 5;
    pub const SET_BTN_TOP_COLOR: usize = 6;

    pub const SET_BTN_BOT_TRIG: usize = 7;
    pub const SET_BTN_BOT_COL: usize = 8;
    pub const SET_BTN_BOT_COLOR: usize = 9;

    /// 11 named transport/nav LEDs start here.
    pub const NAMED_BASE: usize = 10;
    pub const COUNT: usize = NAMED_BASE + 11;
}

impl Push1ProcessNode {
    pub fn new(id: NodeId, controllers: SharedControllers) -> Self {
        Self {
            id,
            controller_id: 0,
            controllers,
            inputs: build_inputs(),
            outputs: build_outputs(),
            idx_map: IndexMap::default(),
            prev_values: Vec::new(),
            pad_events: VecDeque::new(),
            btn_top_events: VecDeque::new(),
            btn_bot_events: VecDeque::new(),
            enc_events: VecDeque::new(),
            pad_trig: 0.0, pad_x: 0.0, pad_y: 0.0, pad_vel: 0.0,
            btn_top_trig: 0.0, btn_top_col: 0.0, btn_top_val: 0.0,
            btn_bot_trig: 0.0, btn_bot_col: 0.0, btn_bot_val: 0.0,
            enc_trig: 0.0, enc_idx: 0.0, enc_value: 0.0,
            slider_value: 0.0,
            named_values: [0.0; 11],
            in_values: vec![0.0; in_idx::COUNT],
            prev_set_pad_trig: 0.0,
            prev_set_btn_top_trig: 0.0,
            prev_set_btn_bot_trig: 0.0,
            prev_named_in: [0.0; 11],
            connected: false,
            controller_name: String::new(),
        }
    }

}

/// Pure indexing helper — takes the controller's input list and produces an
/// IndexMap. Called from `process()` while holding the shared lock, so it
/// must not lock anything itself.
fn compute_index(inputs: &[LearnedInput]) -> IndexMap {
    let mut pads = vec![usize::MAX; 64];
    let mut top = vec![usize::MAX; 8];
    let mut bot = vec![usize::MAX; 8];

    let enc_ccs: [u8; 11] = [14, 15, 71, 72, 73, 74, 75, 76, 77, 78, 79];
    let mut enc_lookup: [Option<usize>; 11] = [None; 11];
    let named_ccs: [u8; 11] = [85, 86, 29, 49, 48, 46, 47, 44, 45, 55, 54];
    let mut named_lookup: [Option<usize>; 11] = [None; 11];
    let mut slider = None;

    for (i, input) in inputs.iter().enumerate() {
        match &input.source {
            InputSource::Midi(MidiSource::NoteVelocity { note, .. }) => {
                // Pad layout from push1_preset_inputs:
                //   note = 36 + (row-1)*8 + (col-1), row 1 = bottom.
                if *note >= 36 && *note <= 99 {
                    let n = *note - 36;
                    let row = n / 8;
                    let col = n % 8;
                    pads[(row * 8 + col) as usize] = i;
                }
            }
            InputSource::Midi(MidiSource::Cc { controller, .. }) => {
                let cc = *controller;
                if (102..=109).contains(&cc) {
                    top[(cc - 102) as usize] = i;
                } else if (20..=27).contains(&cc) {
                    bot[(cc - 20) as usize] = i;
                } else if let Some(pos) = named_ccs.iter().position(|&x| x == cc) {
                    named_lookup[pos] = Some(i);
                }
            }
            InputSource::Midi(MidiSource::CcRelative { controller, .. }) => {
                if let Some(pos) = enc_ccs.iter().position(|&x| x == *controller) {
                    enc_lookup[pos] = Some(i);
                }
            }
            InputSource::Midi(MidiSource::PitchBend { .. }) => slider = Some(i),
            _ => {}
        }
    }

    IndexMap {
        pads,
        btn_top: top,
        btn_bot: bot,
        encoders: enc_lookup.iter().map(|o| o.unwrap_or(usize::MAX)).collect(),
        slider,
        play: named_lookup[0],
        record: named_lookup[1],
        stop: named_lookup[2],
        shift: named_lookup[3],
        select: named_lookup[4],
        up: named_lookup[5],
        down: named_lookup[6],
        left: named_lookup[7],
        right: named_lookup[8],
        oct_up: named_lookup[9],
        oct_down: named_lookup[10],
    }
}

fn build_outputs() -> Vec<PortDef> {
    let mut v = Vec::with_capacity(out_idx::COUNT);
    v.push(PortDef::new("pad trig", PortType::Logic));
    v.push(PortDef::new("pad x", PortType::Untyped));
    v.push(PortDef::new("pad y", PortType::Untyped));
    v.push(PortDef::new("pad vel", PortType::Untyped));

    v.push(PortDef::new("btn top trig", PortType::Logic));
    v.push(PortDef::new("btn top col", PortType::Untyped));
    v.push(PortDef::new("btn top val", PortType::Logic));

    v.push(PortDef::new("btn bot trig", PortType::Logic));
    v.push(PortDef::new("btn bot col", PortType::Untyped));
    v.push(PortDef::new("btn bot val", PortType::Logic));

    v.push(PortDef::new("enc trig", PortType::Logic));
    v.push(PortDef::new("enc idx", PortType::Untyped));
    v.push(PortDef::new("enc val", PortType::Untyped));

    v.push(PortDef::new("slider", PortType::Untyped));

    for name in NAMED_BUTTONS {
        v.push(PortDef::new(*name, PortType::Logic));
    }
    v
}

fn build_inputs() -> Vec<PortDef> {
    let mut v = Vec::with_capacity(in_idx::COUNT);
    v.push(PortDef::new("set pad trig", PortType::Logic));
    v.push(PortDef::new("set pad x", PortType::Untyped));
    v.push(PortDef::new("set pad y", PortType::Untyped));
    v.push(PortDef::new("set pad color", PortType::Untyped));

    v.push(PortDef::new("set btn top trig", PortType::Logic));
    v.push(PortDef::new("set btn top col", PortType::Untyped));
    v.push(PortDef::new("set btn top color", PortType::Untyped));

    v.push(PortDef::new("set btn bot trig", PortType::Logic));
    v.push(PortDef::new("set btn bot col", PortType::Untyped));
    v.push(PortDef::new("set btn bot color", PortType::Untyped));

    for name in NAMED_BUTTONS {
        v.push(PortDef::new(*name, PortType::Logic));
    }
    v
}

impl ProcessNode for Push1ProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Push 1" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if let Some(slot) = self.in_values.get_mut(port_index) {
            *slot = value;
        }
    }

    fn process(&mut self) {
        if self.controller_id == 0 {
            self.reset_outputs();
            return;
        }

        // Phase 1: hold the lock only long enough to read diffs and write
        // feedback. Everything else happens after the lock is released so we
        // can call &mut self helpers freely.
        let mut found = true;
        {
            let mut state = self.controllers.lock().unwrap();
            let c = match state.iter_mut().find(|c| c.id == self.controller_id) {
                Some(c) => c,
                None => {
                    found = false;
                    // Fall through; post-lock cleanup will reset outputs.
                    // Can't early-return from inside this scope and still
                    // use the lock data afterwards.
                    self.connected = false;
                    // Drop the guard implicitly and continue below.
                    drop(state);
                    self.reset_outputs();
                    return;
                }
            };
            if !matches!(c.kind, InputControllerKind::Push1 { .. }) {
                self.connected = false;
                drop(state);
                self.reset_outputs();
                return;
            }
            self.controller_name = c.name.clone();
            self.connected = true;

            // Lazy index rebuild.
            if self.idx_map.pads.is_empty() || self.idx_map.pads.len() != 64 {
                self.idx_map = compute_index(&c.inputs);
            }
            if self.prev_values.len() != c.values.len() {
                self.prev_values = c.values.clone();
            }

            // Classify diffs → enqueue events.
            for (i, &v) in c.values.iter().enumerate() {
                let prev = self.prev_values[i];
                if (v - prev).abs() < 1e-6 { continue; }

                if let Some(pad_flat) = self.idx_map.pads.iter().position(|&idx| idx == i) {
                    let col = (pad_flat % 8) as u8;
                    let row = (pad_flat / 8) as u8;
                    self.pad_events.push_back(PadEvent { col, row, vel: v });
                } else if let Some(col) = self.idx_map.btn_top.iter().position(|&idx| idx == i) {
                    self.btn_top_events.push_back(BtnEvent { col: col as u8, value: v });
                } else if let Some(col) = self.idx_map.btn_bot.iter().position(|&idx| idx == i) {
                    self.btn_bot_events.push_back(BtnEvent { col: col as u8, value: v });
                } else if let Some(eidx) = self.idx_map.encoders.iter().position(|&idx| idx == i) {
                    self.enc_events.push_back(EncEvent { idx: eidx as u8, value: v });
                } else if Some(i) == self.idx_map.slider {
                    self.slider_value = v;
                } else {
                    let named = [
                        self.idx_map.play, self.idx_map.record, self.idx_map.stop,
                        self.idx_map.shift, self.idx_map.select,
                        self.idx_map.up, self.idx_map.down, self.idx_map.left, self.idx_map.right,
                        self.idx_map.oct_up, self.idx_map.oct_down,
                    ];
                    if let Some(pos) = named.iter().position(|&x| x == Some(i)) {
                        self.named_values[pos] = v;
                    }
                }
            }
            self.prev_values.copy_from_slice(&c.values);

            // Feedback writes — rising-edge commits for the (trig, addr, color)
            // groups plus continuous mirroring for named LEDs.
            let trig = self.in_values[in_idx::SET_PAD_TRIG];
            if trig >= 0.5 && self.prev_set_pad_trig < 0.5 {
                let col = self.in_values[in_idx::SET_PAD_X].clamp(0.0, 7.0) as usize;
                let row = self.in_values[in_idx::SET_PAD_Y].clamp(0.0, 7.0) as usize;
                let color = self.in_values[in_idx::SET_PAD_COLOR].clamp(0.0, 1.0);
                let flat = row * 8 + col;
                if let Some(&idx) = self.idx_map.pads.get(flat) {
                    if idx != usize::MAX {
                        if let Some(slot) = c.out_values.get_mut(idx) { *slot = color; }
                    }
                }
            }
            self.prev_set_pad_trig = trig;

            let trig = self.in_values[in_idx::SET_BTN_TOP_TRIG];
            if trig >= 0.5 && self.prev_set_btn_top_trig < 0.5 {
                let col = self.in_values[in_idx::SET_BTN_TOP_COL].clamp(0.0, 7.0) as usize;
                let color = self.in_values[in_idx::SET_BTN_TOP_COLOR].clamp(0.0, 1.0);
                if let Some(&idx) = self.idx_map.btn_top.get(col) {
                    if idx != usize::MAX {
                        if let Some(slot) = c.out_values.get_mut(idx) { *slot = color; }
                    }
                }
            }
            self.prev_set_btn_top_trig = trig;

            let trig = self.in_values[in_idx::SET_BTN_BOT_TRIG];
            if trig >= 0.5 && self.prev_set_btn_bot_trig < 0.5 {
                let col = self.in_values[in_idx::SET_BTN_BOT_COL].clamp(0.0, 7.0) as usize;
                let color = self.in_values[in_idx::SET_BTN_BOT_COLOR].clamp(0.0, 1.0);
                if let Some(&idx) = self.idx_map.btn_bot.get(col) {
                    if idx != usize::MAX {
                        if let Some(slot) = c.out_values.get_mut(idx) { *slot = color; }
                    }
                }
            }
            self.prev_set_btn_bot_trig = trig;

            let named_targets = [
                self.idx_map.play, self.idx_map.record, self.idx_map.stop,
                self.idx_map.shift, self.idx_map.select,
                self.idx_map.up, self.idx_map.down, self.idx_map.left, self.idx_map.right,
                self.idx_map.oct_up, self.idx_map.oct_down,
            ];
            for (i, target) in named_targets.iter().enumerate() {
                let v = self.in_values[in_idx::NAMED_BASE + i];
                if (v - self.prev_named_in[i]).abs() > 1e-6 {
                    if let Some(idx) = target {
                        if let Some(slot) = c.out_values.get_mut(*idx) { *slot = v.clamp(0.0, 1.0); }
                    }
                    self.prev_named_in[i] = v;
                }
            }
        }
        if !found { return; }

        // Phase 2: emit one event per group per tick. `&mut self` methods
        // can run now that the MutexGuard is out of scope.
        self.emit_pad_event();
        self.emit_btn_top_event();
        self.emit_btn_bot_event();
        self.emit_enc_event();
    }

    fn read_output(&self, pi: usize) -> f32 {
        match pi {
            i if i == out_idx::PAD_TRIG => self.pad_trig,
            i if i == out_idx::PAD_X => self.pad_x,
            i if i == out_idx::PAD_Y => self.pad_y,
            i if i == out_idx::PAD_VEL => self.pad_vel,
            i if i == out_idx::BTN_TOP_TRIG => self.btn_top_trig,
            i if i == out_idx::BTN_TOP_COL => self.btn_top_col,
            i if i == out_idx::BTN_TOP_VAL => self.btn_top_val,
            i if i == out_idx::BTN_BOT_TRIG => self.btn_bot_trig,
            i if i == out_idx::BTN_BOT_COL => self.btn_bot_col,
            i if i == out_idx::BTN_BOT_VAL => self.btn_bot_val,
            i if i == out_idx::ENC_TRIG => self.enc_trig,
            i if i == out_idx::ENC_IDX => self.enc_idx,
            i if i == out_idx::ENC_VAL => self.enc_value,
            i if i == out_idx::SLIDER => self.slider_value,
            i if i >= out_idx::NAMED_BASE && i < out_idx::COUNT => {
                self.named_values[i - out_idx::NAMED_BASE]
            }
            _ => 0.0,
        }
    }

    fn read_input(&self, pi: usize) -> f32 {
        self.in_values.get(pi).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "controller_id": self.controller_id }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(id) = data.get("controller_id").and_then(|v| v.as_u64()) {
            let new_id = id as u32;
            if new_id != self.controller_id {
                self.controller_id = new_id;
                // Force index rebuild on next process() tick.
                self.idx_map = IndexMap::default();
                self.prev_values.clear();
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(Push1Display {
            controller_id: self.controller_id,
            controller_name: self.controller_name.clone(),
            connected: self.connected,
        }));
    }
}

impl Push1ProcessNode {
    fn reset_outputs(&mut self) {
        self.pad_trig = 0.0;
        self.btn_top_trig = 0.0;
        self.btn_bot_trig = 0.0;
        self.enc_trig = 0.0;
        self.pad_events.clear();
        self.btn_top_events.clear();
        self.btn_bot_events.clear();
        self.enc_events.clear();
    }

    fn emit_pad_event(&mut self) {
        if let Some(ev) = self.pad_events.pop_front() {
            self.pad_trig = 1.0;
            self.pad_x = ev.col as f32;
            self.pad_y = ev.row as f32;
            self.pad_vel = ev.vel;
        } else {
            self.pad_trig = 0.0;
        }
    }
    fn emit_btn_top_event(&mut self) {
        if let Some(ev) = self.btn_top_events.pop_front() {
            self.btn_top_trig = 1.0;
            self.btn_top_col = ev.col as f32;
            self.btn_top_val = ev.value;
        } else {
            self.btn_top_trig = 0.0;
        }
    }
    fn emit_btn_bot_event(&mut self) {
        if let Some(ev) = self.btn_bot_events.pop_front() {
            self.btn_bot_trig = 1.0;
            self.btn_bot_col = ev.col as f32;
            self.btn_bot_val = ev.value;
        } else {
            self.btn_bot_trig = 0.0;
        }
    }
    fn emit_enc_event(&mut self) {
        if let Some(ev) = self.enc_events.pop_front() {
            self.enc_trig = 1.0;
            self.enc_idx = ev.idx as f32;
            self.enc_value = ev.value;
        } else {
            self.enc_trig = 0.0;
        }
    }

}
