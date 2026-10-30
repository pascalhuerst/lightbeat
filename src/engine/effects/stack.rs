//! Effect Stack: the effects linked to one name, blended in order.

use std::any::Any;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::color::BlendMode;
use crate::dmx_io::SharedObjectStore;
use crate::engine::types::*;

use super::hub::{StackEntry, blend_layer, clear};
use super::{EffectHub, Geometry, LayerBuffer, RenderGate, TargetSpec};

fn one() -> f32 {
    1.0
}

/// How the stack treats one of its effects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerConfig {
    /// The effect node.
    pub node_id: u64,
    #[serde(default)]
    pub blend: BlendMode,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub solo: bool,
}

impl LayerConfig {
    pub fn new(node_id: u64) -> Self {
        Self { node_id, blend: BlendMode::Override, opacity: 1.0, muted: false, solo: false }
    }
}

/// Everything the UI sets on a stack.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StackConfig {
    /// The name effects link to.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub group_ids: Vec<u32>,
    #[serde(default)]
    pub group_names: Vec<String>,
    #[serde(default)]
    pub targets: Vec<TargetSpec>,
    /// Stacks are blended into the group in this order, lowest first.
    #[serde(default)]
    pub priority: i32,
    /// How this stack blends over the stacks below it.
    #[serde(default)]
    pub blend: BlendMode,
    /// Top layer first, as the list reads on screen.
    #[serde(default)]
    pub layers: Vec<LayerConfig>,
    /// Show only the status, not the layer list.
    #[serde(default)]
    pub folded: bool,
}

/// What the widget shows.
pub struct StackDisplay {
    pub pixels: usize,
    /// The composed image, at most 64 columns per target.
    pub preview: Vec<Vec<[u8; 3]>>,
    /// Activity of each layer by effect node id.
    pub levels: Vec<(u64, f32)>,
}

pub struct EffectStackProcessNode {
    id: NodeId,
    hub: EffectHub,
    store: SharedObjectStore,
    config: StackConfig,
    inputs: Vec<PortDef>,
    amount: f32,
    amount_wired: bool,
    gate: RenderGate,
    geometry: Option<Arc<Geometry>>,
    composed: LayerBuffer,
    levels: Vec<(u64, f32)>,
    /// Name under which this stack's geometry is currently published.
    published_name: String,
}

impl EffectStackProcessNode {
    pub fn new(id: NodeId, hub: EffectHub, store: SharedObjectStore) -> Self {
        Self {
            id,
            hub,
            store,
            config: StackConfig::default(),
            inputs: vec![PortDef::new("amount", PortType::Untyped)],
            amount: 1.0,
            amount_wired: false,
            gate: RenderGate::new(),
            geometry: None,
            composed: Vec::new(),
            levels: Vec::new(),
            published_name: String::new(),
        }
    }

    fn unpublish(&mut self) {
        let mut hub = self.hub.lock();
        hub.stacks.remove(&self.id.0);
        if !self.published_name.is_empty() {
            hub.geometry.remove(&self.published_name);
        }
        self.published_name.clear();
        self.geometry = None;
    }
}

impl Drop for EffectStackProcessNode {
    fn drop(&mut self) {
        self.unpublish();
    }
}

impl ProcessNode for EffectStackProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Effect Stack" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, channel: usize, value: f32) {
        if channel == 0 {
            self.amount = value.clamp(0.0, 1.0);
        }
    }

    fn read_input(&self, channel: usize) -> f32 {
        if channel == 0 { self.amount } else { 0.0 }
    }

    fn set_input_connections(&mut self, connected: &[bool]) {
        self.amount_wired = connected.first().copied().unwrap_or(false);
    }

    fn process(&mut self) {
        if !self.gate.due() {
            return;
        }
        if self.config.name.is_empty() || self.config.targets.is_empty() {
            self.unpublish();
            return;
        }

        // Pixel counts can change when a fixture is edited, so look them up
        // each frame; only publish when something actually differs.
        let resolved = Geometry::resolve(&self.config.targets, &self.store.lock().unwrap());
        if self.geometry.as_deref() != Some(&resolved) || self.published_name != self.config.name {
            let geometry = Arc::new(resolved);
            let mut hub = self.hub.lock();
            if !self.published_name.is_empty() && self.published_name != self.config.name {
                hub.geometry.remove(&self.published_name);
            }
            hub.geometry.insert(self.config.name.clone(), geometry.clone());
            drop(hub);
            self.published_name.clone_from(&self.config.name);
            self.geometry = Some(geometry);
        }
        let Some(geometry) = self.geometry.clone() else { return };

        if !geometry.fits(&self.composed) {
            self.composed = geometry.empty_buffer();
        } else {
            clear(&mut self.composed);
        }

        let soloed = self.config.layers.iter().any(|l| l.solo);
        self.levels.clear();
        {
            let hub = self.hub.lock();
            // The list reads top first, so draw it bottom first.
            for layer in self.config.layers.iter().rev() {
                let Some(entry) = hub.layers.get(&layer.node_id) else { continue };
                self.levels.push((layer.node_id, entry.level));
                if layer.muted || (soloed && !layer.solo) || entry.stack != self.config.name {
                    continue;
                }
                if geometry.fits(&entry.buf) {
                    blend_layer(&mut self.composed, &entry.buf, layer.blend, layer.opacity);
                }
            }
        }

        let amount = if self.amount_wired { self.amount } else { 1.0 };
        let mut hub = self.hub.lock();
        match hub.stacks.get_mut(&self.id.0) {
            Some(entry) => {
                entry.priority = self.config.priority;
                entry.blend = self.config.blend;
                entry.amount = amount;
                entry.geometry = geometry;
                entry.buf.clone_from(&self.composed);
            }
            None => {
                hub.stacks.insert(
                    self.id.0,
                    StackEntry {
                        priority: self.config.priority,
                        blend: self.config.blend,
                        amount,
                        geometry,
                        buf: self.composed.clone(),
                    },
                );
            }
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.config).ok()
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Ok(config) = serde_json::from_value::<StackConfig>(data.clone()) {
            self.config = config;
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        // Shrink long strips to a thumbnail the widget can draw cheaply.
        const COLUMNS: usize = 64;
        let preview = self
            .composed
            .iter()
            .map(|strip| {
                if strip.is_empty() {
                    return Vec::new();
                }
                let n = strip.len().min(COLUMNS);
                (0..n)
                    .map(|c| {
                        let p = strip[c * strip.len() / n];
                        let (r, g, b) = p.color.scaled(p.alpha.clamp(0.0, 1.0)).clamped().to_u8();
                        [r, g, b]
                    })
                    .collect()
            })
            .collect();
        shared.display = Some(Box::new(StackDisplay {
            pixels: self.composed.iter().map(Vec::len).sum(),
            preview,
            levels: self.levels.clone(),
        }));
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
