//! The engine node every effect shares.
//!
//! An effect only says what it needs (`EffectKind`): its ports, its
//! parameters, how it reacts to triggers, and how it draws. The node around it
//! deals with the rest: counting trigger edges at the engine's full rate,
//! rendering at 60 Hz, finding the stack's geometry, publishing the layer.

use std::any::Any;
use std::time::Instant;

use crate::color::{Gradient, Rgb};
use crate::engine::types::*;

use super::{EffectHub, Geometry, LayerBuffer, RenderGate, layer_level};

pub struct UpdateCtx<'a> {
    /// Seconds since the previous engine tick.
    pub dt: f32,
    /// Rising edges seen on each trigger input since the previous tick, in
    /// the order of `EffectKind::trigger_ports`.
    pub triggers: &'a [u32],
}

pub struct RenderCtx<'a> {
    pub geometry: &'a Geometry,
    /// The colour input; white while nothing is wired to it.
    pub gradient: &'a Gradient,
    /// Every input channel, flat.
    pub inputs: &'a [f32],
}

pub trait EffectKind: Send + 'static {
    fn type_name(&self) -> &'static str;
    /// Control inputs. One of them should be a `Gradient` port for colour.
    fn inputs(&self) -> Vec<PortDef>;
    /// Logical indices of single-channel trigger inputs.
    fn trigger_ports(&self) -> Vec<usize> {
        Vec::new()
    }
    fn params(&self) -> Vec<ParamDef>;
    fn set_param(&mut self, index: usize, value: ParamValue);
    fn update(&mut self, _ctx: &UpdateCtx) {}
    /// Draw into `buf`, which is cleared and shaped like the geometry.
    fn render(&self, ctx: &RenderCtx, buf: &mut LayerBuffer);
}

/// What the widget shows.
pub struct EffectDisplay {
    pub stack: String,
    /// True once the stack named by `stack` exists and the layer is drawing.
    pub linked: bool,
    pub level: f32,
}

pub struct EffectNode<E: EffectKind> {
    id: NodeId,
    kind: E,
    hub: EffectHub,
    inputs: Vec<PortDef>,
    values: Vec<f32>,
    /// Channel index of each trigger input, with its previous state and
    /// the edges not yet handed to the effect.
    triggers: Vec<(usize, bool)>,
    pending: Vec<u32>,
    gradient_channel: Option<usize>,
    gradient_wired: bool,
    white: Gradient,
    stack: String,
    gate: RenderGate,
    last_tick: Option<Instant>,
    buf: LayerBuffer,
    linked: bool,
    level: f32,
}

impl<E: EffectKind> EffectNode<E> {
    pub fn new(id: NodeId, kind: E, hub: EffectHub) -> Self {
        let inputs = kind.inputs();
        let channels = total_channels(&inputs);
        let triggers: Vec<(usize, bool)> =
            kind.trigger_ports().into_iter().map(|p| (port_base_index(&inputs, p), false)).collect();
        let gradient_channel = inputs
            .iter()
            .position(|p| p.port_type == PortType::Gradient)
            .map(|p| port_base_index(&inputs, p));
        Self {
            id,
            kind,
            hub,
            pending: vec![0; triggers.len()],
            triggers,
            values: vec![0.0; channels],
            inputs,
            gradient_channel,
            gradient_wired: false,
            white: Gradient::two(Rgb::WHITE, Rgb::WHITE),
            stack: String::new(),
            gate: RenderGate::new(),
            last_tick: None,
            buf: Vec::new(),
            linked: false,
            level: 0.0,
        }
    }

    fn unpublish(&self) {
        self.hub.lock().layers.remove(&self.id.0);
    }
}

impl<E: EffectKind> Drop for EffectNode<E> {
    fn drop(&mut self) {
        self.unpublish();
    }
}

impl<E: EffectKind> ProcessNode for EffectNode<E> {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.kind.type_name() }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, channel: usize, value: f32) {
        let Some(slot) = self.values.get_mut(channel) else { return };
        *slot = value;
        // Edges are counted here, at the full tick rate, so a 1 ms pulse is
        // never lost between two renders.
        if let Some(i) = self.triggers.iter().position(|(c, _)| *c == channel) {
            let high = value >= 0.5;
            if high && !self.triggers[i].1 {
                self.pending[i] += 1;
            }
            self.triggers[i].1 = high;
        }
    }

    fn read_input(&self, channel: usize) -> f32 {
        self.values.get(channel).copied().unwrap_or(0.0)
    }

    fn set_input_connections(&mut self, connected: &[bool]) {
        if let Some(ch) = self.gradient_channel {
            let port = self.inputs.iter().enumerate().scan(0usize, |acc, (i, p)| {
                let base = *acc;
                *acc += p.port_type.channel_count();
                Some((i, base))
            }).find(|(_, base)| *base == ch).map(|(i, _)| i);
            self.gradient_wired = port.and_then(|p| connected.get(p)).copied().unwrap_or(false);
        }
    }

    fn process(&mut self) {
        let now = Instant::now();
        let dt = self.last_tick.map(|t| now.duration_since(t).as_secs_f32().min(0.1)).unwrap_or(0.0);
        self.last_tick = Some(now);

        self.kind.update(&UpdateCtx { dt, triggers: &self.pending });
        self.pending.fill(0);

        if !self.gate.due() {
            return;
        }
        let geometry = if self.stack.is_empty() { None } else { self.hub.lock().geometry.get(&self.stack).cloned() };
        let Some(geometry) = geometry else {
            self.linked = false;
            self.level = 0.0;
            self.unpublish();
            return;
        };
        self.linked = true;

        if !geometry.fits(&self.buf) {
            self.buf = geometry.empty_buffer();
        } else {
            super::hub::clear(&mut self.buf);
        }
        let gradient = match (self.gradient_wired, self.gradient_channel) {
            (true, Some(ch)) => Gradient::from_channels(&self.values[ch..ch + GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS]),
            _ => self.white.clone(),
        };
        self.kind.render(&RenderCtx { geometry: &geometry, gradient: &gradient, inputs: &self.values }, &mut self.buf);
        self.level = layer_level(&self.buf);

        let mut hub = self.hub.lock();
        match hub.layers.get_mut(&self.id.0) {
            Some(entry) => {
                entry.stack.clone_from(&self.stack);
                entry.buf.clone_from(&self.buf);
                entry.level = self.level;
            }
            None => {
                hub.layers.insert(
                    self.id.0,
                    super::hub::LayerEntry { stack: self.stack.clone(), buf: self.buf.clone(), level: self.level },
                );
            }
        }
    }

    fn params(&self) -> Vec<ParamDef> { self.kind.params() }
    fn set_param(&mut self, index: usize, value: ParamValue) { self.kind.set_param(index, value) }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "stack": self.stack }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(name) = data.get("stack").and_then(|v| v.as_str())
            && name != self.stack
        {
            self.unpublish();
            self.stack = name.to_string();
            self.linked = false;
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(EffectDisplay {
            stack: self.stack.clone(),
            linked: self.linked,
            level: self.level,
        }));
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
