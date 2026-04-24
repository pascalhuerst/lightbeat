//! Port Hub — N same-typed (in, out) pairs that just pass the value
//! through. Pure wire-organization node: no processing, no state.
//! Useful when a big bundle of wires needs to cross the graph tidily
//! or to act as a single "grab handle" for copy/paste.
//!
//! All slots share one port type (chosen in the inspector) so the node
//! stays simple and predictable. Slot count 1..=16.

use crate::engine::nodes::math::multiplex::{
    MUX_MAX_SLOTS, MUX_MIN_SLOTS, MUX_TYPE_NAMES, MUX_TYPES, clamp_slots, type_from_index,
    type_to_index,
};
use crate::engine::types::*;

pub const HUB_DEFAULT_SLOTS: usize = 4;

/// Max channels per slot across the supported port types (mirrors the
/// Mux's `MAX_SLOT_CHANNELS` — kept in sync because we reuse the same
/// type list).
const MAX_SLOT_CHANNELS: usize = GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS;

pub struct PortHubDisplay {
    pub port_type: PortType,
    pub slots: usize,
}

pub struct PortHubProcessNode {
    id: NodeId,
    port_type: PortType,
    slots: usize,
    /// Flat channel buffer: slot `i`'s channels start at `i * n_ch`.
    values: Vec<f32>,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl PortHubProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mut node = Self {
            id,
            port_type: PortType::Untyped,
            slots: HUB_DEFAULT_SLOTS,
            values: vec![0.0; MUX_MAX_SLOTS * MAX_SLOT_CHANNELS],
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        node.rebuild();
        node
    }

    fn rebuild(&mut self) {
        let pt = self.port_type;
        self.inputs = (0..self.slots)
            .map(|i| PortDef::new(format!("in{}", i + 1), pt))
            .collect();
        self.outputs = (0..self.slots)
            .map(|i| PortDef::new(format!("out{}", i + 1), pt))
            .collect();
        for v in self.values.iter_mut() { *v = 0.0; }
    }
}

impl ProcessNode for PortHubProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Port Hub" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, channel: usize, v: f32) {
        if channel < self.values.len() { self.values[channel] = v; }
    }
    fn read_input(&self, channel: usize) -> f32 {
        self.values.get(channel).copied().unwrap_or(0.0)
    }

    fn process(&mut self) {
        // No-op: `read_output` reads the same flat buffer that
        // `write_input` writes, so values pass straight through.
    }

    fn read_output(&self, channel: usize) -> f32 {
        self.values.get(channel).copied().unwrap_or(0.0)
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
        shared.display = Some(Box::new(PortHubDisplay {
            port_type: self.port_type,
            slots: self.slots,
        }));
    }
}

// Keep `MUX_TYPES` referenced here so the unused-import warning doesn't
// flip if we later strip the import — the hub and mux share the same
// type universe intentionally.
#[allow(dead_code)]
const _TYPES_REFERENCE: &[PortType] = MUX_TYPES;
