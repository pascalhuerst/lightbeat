//! Generic typed multiplexer / demultiplexer.
//!
//! Both nodes start in `PortType::Any` ("Auto") and lock to the first
//! connected wire's type. Slot count is adjustable (2..=MUX_MAX_SLOTS).
//! `select` is an untyped integer index (rounded, clamped to 0..slots-1).

use crate::engine::types::*;

/// Maximum slots a mux/demux can have. Buffers are sized for this.
pub const MUX_MAX_SLOTS: usize = 16;
/// Default slot count for newly created nodes.
pub const MUX_DEFAULT_SLOTS: usize = 8;
/// Minimum slot count (two slots is the minimum meaningful multiplexer).
pub const MUX_MIN_SLOTS: usize = 2;
/// Max channels per slot across supported types (Gradient = 40).
const MAX_SLOT_CHANNELS: usize = GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS;

/// Types the mux/demux can operate on; `Any` is the neutral initial state.
pub const MUX_TYPES: &[PortType] = &[
    PortType::Any,
    PortType::Logic,
    PortType::Phase,
    PortType::Untyped,
    PortType::Color,
    PortType::Position,
    PortType::Palette,
    PortType::Gradient,
];

pub const MUX_TYPE_NAMES: &[&str] = &[
    "Auto", "Logic", "Phase", "Untyped", "Color", "Position", "Palette", "Gradient",
];

pub fn type_to_index(pt: PortType) -> usize {
    MUX_TYPES.iter().position(|t| *t == pt).unwrap_or(0)
}

pub fn type_from_index(i: usize) -> PortType {
    MUX_TYPES.get(i).copied().unwrap_or(PortType::Any)
}

pub fn clamp_slots(n: usize) -> usize {
    n.clamp(MUX_MIN_SLOTS, MUX_MAX_SLOTS)
}

pub struct MuxDisplay {
    pub port_type: PortType,
    pub slots: usize,
    pub selected: usize,
}

// ---------------------------------------------------------------------------
// Multiplexer: N typed inputs + select → 1 typed output
// ---------------------------------------------------------------------------

pub struct MultiplexerProcessNode {
    id: NodeId,
    port_type: PortType,
    slots: usize,
    /// Layout: [select][slot0 channels ...][slot1 channels ...] ... [slotN-1]
    input_values: Vec<f32>,
    output_values: Vec<f32>,
    selected: usize,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl MultiplexerProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mut node = Self {
            id,
            port_type: PortType::Any,
            slots: MUX_DEFAULT_SLOTS,
            input_values: vec![0.0; 1 + MUX_MAX_SLOTS * MAX_SLOT_CHANNELS],
            output_values: vec![0.0; MAX_SLOT_CHANNELS],
            selected: 0,
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        node.rebuild();
        node
    }

    fn rebuild(&mut self) {
        let pt = self.port_type;
        let mut ins = vec![PortDef::new("select", PortType::Untyped)];
        for i in 0..self.slots {
            ins.push(PortDef::new(format!("in{}", i), pt));
        }
        self.inputs = ins;
        self.outputs = vec![PortDef::new("out", pt)];
        for v in self.input_values.iter_mut() { *v = 0.0; }
        for v in self.output_values.iter_mut() { *v = 0.0; }
        self.selected = 0;
    }
}

impl ProcessNode for MultiplexerProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Multiplexer" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, channel: usize, v: f32) {
        if channel < self.input_values.len() { self.input_values[channel] = v; }
    }
    fn read_input(&self, channel: usize) -> f32 {
        self.input_values.get(channel).copied().unwrap_or(0.0)
    }

    fn process(&mut self) {
        if self.port_type == PortType::Any {
            return;
        }
        let n = self.port_type.channel_count();
        let sel = self.input_values[0];
        let idx = (sel.round() as i32).clamp(0, self.slots as i32 - 1) as usize;
        self.selected = idx;
        let base = 1 + idx * n;
        for c in 0..n {
            self.output_values[c] = self.input_values.get(base + c).copied().unwrap_or(0.0);
        }
    }

    fn read_output(&self, channel: usize) -> f32 {
        self.output_values.get(channel).copied().unwrap_or(0.0)
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Choice {
                name: "Type".into(),
                value: type_to_index(self.port_type),
                options: MUX_TYPE_NAMES.iter().map(|s| s.to_string()).collect(),
            },
            ParamDef::Int {
                name: "Slots".into(),
                value: self.slots as i64,
                min: MUX_MIN_SLOTS as i64,
                max: MUX_MAX_SLOTS as i64,
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => {
                let new_pt = type_from_index(value.as_usize());
                if new_pt != self.port_type {
                    self.port_type = new_pt;
                    self.rebuild();
                }
            }
            1 => {
                let new_slots = clamp_slots(value.as_usize());
                if new_slots != self.slots {
                    self.slots = new_slots;
                    self.rebuild();
                }
            }
            _ => {}
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(MuxDisplay {
            port_type: self.port_type,
            slots: self.slots,
            selected: self.selected,
        }));
    }
}

// ---------------------------------------------------------------------------
// Demultiplexer: 1 typed input + select → N typed outputs (unselected = 0)
// ---------------------------------------------------------------------------

pub struct DemultiplexerProcessNode {
    id: NodeId,
    port_type: PortType,
    slots: usize,
    /// Layout: [select][input channels ...]
    input_values: Vec<f32>,
    /// Layout: [slot0 channels ...][slot1 channels ...] ... [slotN-1]
    output_values: Vec<f32>,
    selected: usize,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl DemultiplexerProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mut node = Self {
            id,
            port_type: PortType::Any,
            slots: MUX_DEFAULT_SLOTS,
            input_values: vec![0.0; 1 + MAX_SLOT_CHANNELS],
            output_values: vec![0.0; MUX_MAX_SLOTS * MAX_SLOT_CHANNELS],
            selected: 0,
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        node.rebuild();
        node
    }

    fn rebuild(&mut self) {
        let pt = self.port_type;
        self.inputs = vec![
            PortDef::new("select", PortType::Untyped),
            PortDef::new("in", pt),
        ];
        self.outputs = (0..self.slots)
            .map(|i| PortDef::new(format!("out{}", i), pt))
            .collect();
        for v in self.input_values.iter_mut() { *v = 0.0; }
        for v in self.output_values.iter_mut() { *v = 0.0; }
        self.selected = 0;
    }
}

impl ProcessNode for DemultiplexerProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Demultiplexer" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, channel: usize, v: f32) {
        if channel < self.input_values.len() { self.input_values[channel] = v; }
    }
    fn read_input(&self, channel: usize) -> f32 {
        self.input_values.get(channel).copied().unwrap_or(0.0)
    }

    fn process(&mut self) {
        if self.port_type == PortType::Any {
            return;
        }
        let n = self.port_type.channel_count();
        let sel = self.input_values[0];
        let idx = (sel.round() as i32).clamp(0, self.slots as i32 - 1) as usize;
        self.selected = idx;
        for v in self.output_values.iter_mut() { *v = 0.0; }
        let base = idx * n;
        for c in 0..n {
            self.output_values[base + c] = self.input_values.get(1 + c).copied().unwrap_or(0.0);
        }
    }

    fn read_output(&self, channel: usize) -> f32 {
        self.output_values.get(channel).copied().unwrap_or(0.0)
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Choice {
                name: "Type".into(),
                value: type_to_index(self.port_type),
                options: MUX_TYPE_NAMES.iter().map(|s| s.to_string()).collect(),
            },
            ParamDef::Int {
                name: "Slots".into(),
                value: self.slots as i64,
                min: MUX_MIN_SLOTS as i64,
                max: MUX_MAX_SLOTS as i64,
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => {
                let new_pt = type_from_index(value.as_usize());
                if new_pt != self.port_type {
                    self.port_type = new_pt;
                    self.rebuild();
                }
            }
            1 => {
                let new_slots = clamp_slots(value.as_usize());
                if new_slots != self.slots {
                    self.slots = new_slots;
                    self.rebuild();
                }
            }
            _ => {}
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(MuxDisplay {
            port_type: self.port_type,
            slots: self.slots,
            selected: self.selected,
        }));
    }
}
