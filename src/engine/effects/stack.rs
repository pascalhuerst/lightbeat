//! Effect Stack: the effects and other stacks linked to one name, blended in
//! order. A stack with nothing to send to is a root and writes to its groups;
//! the others blend into their parent, so stacks form a tree.

use std::any::Any;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::color::BlendMode;
use crate::dmx_io::SharedObjectStore;
use crate::engine::types::*;

use super::hub::{StackEntry, blend_layer, clear};
use super::{EffectHub, Geometry, LayerBuffer, RenderGate, TargetSpec, layer_level};

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
    /// The stack this one is sent to, by name. Empty for a root, which is the
    /// only kind that writes to the groups (the others take their geometry
    /// from their parent).
    #[serde(default)]
    pub parent: String,
    /// How a root blends over other roots on the same objects.
    #[serde(default)]
    pub blend: BlendMode,
    /// The effects and stacks sent here, top first, as the list reads on screen.
    #[serde(default)]
    pub layers: Vec<LayerConfig>,
    /// Show only the status, not the layer list.
    #[serde(default)]
    pub folded: bool,
}

/// One target in the preview: where it sits and what it shows.
#[derive(Clone)]
pub struct PreviewStrip {
    pub row: usize,
    /// Where the first and last pixel are on the group's 0..1 axis; the
    /// first is the larger for a reversed strip.
    pub start: f32,
    pub end: f32,
    pub colors: Vec<[u8; 3]>,
}

/// What the widget shows.
pub struct StackDisplay {
    pub pixels: usize,
    /// The composed image, at most 64 columns per target.
    pub preview: Vec<PreviewStrip>,
    /// Rows of the group's layout.
    pub rows: usize,
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
        if self.config.name.is_empty() {
            self.unpublish();
            return;
        }
        let is_root = self.config.parent.is_empty();

        // A root works out its geometry from its groups, and pixel counts can
        // change when a fixture is edited, so look them up each frame. The
        // others inherit their parent's.
        let geometry = if is_root {
            if self.config.targets.is_empty() {
                self.unpublish();
                return;
            }
            let resolved = Geometry::resolve(&self.config.targets, &self.store.lock().unwrap());
            match &self.geometry {
                Some(g) if **g == resolved => g.clone(),
                _ => Arc::new(resolved),
            }
        } else {
            let parent = self.hub.lock().geometry.get(&self.config.parent).cloned();
            match parent {
                Some(g) => g,
                None => {
                    self.unpublish();
                    return;
                }
            }
        };
        let same = self.geometry.as_ref().is_some_and(|g| Arc::ptr_eq(g, &geometry));
        if !same || self.published_name != self.config.name {
            let mut hub = self.hub.lock();
            if !self.published_name.is_empty() && self.published_name != self.config.name {
                hub.geometry.remove(&self.published_name);
            }
            hub.geometry.insert(self.config.name.clone(), geometry.clone());
            drop(hub);
            self.published_name.clone_from(&self.config.name);
            self.geometry = Some(geometry.clone());
        }

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
                // A child is an effect's layer or another stack's image.
                let (buf, level, linked, amount) = if let Some(e) = hub.layers.get(&layer.node_id) {
                    (&e.buf, e.level, e.stack == self.config.name, 1.0)
                } else if let Some(e) = hub.stacks.get(&layer.node_id) {
                    (&e.buf, layer_level(&e.buf), e.parent == self.config.name, e.amount)
                } else {
                    continue;
                };
                self.levels.push((layer.node_id, level));
                if layer.muted || (soloed && !layer.solo) || !linked {
                    continue;
                }
                if geometry.fits(buf) {
                    blend_layer(&mut self.composed, buf, layer.blend, layer.opacity * amount);
                }
            }
        }

        let amount = if self.amount_wired { self.amount } else { 1.0 };
        let mut hub = self.hub.lock();
        match hub.stacks.get_mut(&self.id.0) {
            Some(entry) => {
                entry.name.clone_from(&self.config.name);
                entry.parent.clone_from(&self.config.parent);
                entry.blend = self.config.blend;
                entry.amount = amount;
                entry.geometry = geometry;
                entry.buf.clone_from(&self.composed);
            }
            None => {
                hub.stacks.insert(
                    self.id.0,
                    StackEntry {
                        name: self.config.name.clone(),
                        parent: self.config.parent.clone(),
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
        let geometry = self.geometry.as_deref();
        let preview = self
            .composed
            .iter()
            .enumerate()
            .filter(|(_, strip)| !strip.is_empty())
            .map(|(t, strip)| {
                let n = strip.len().min(COLUMNS);
                let colors = (0..n)
                    .map(|c| {
                        let p = strip[c * strip.len() / n];
                        let (r, g, b) = p.color.scaled(p.alpha.clamp(0.0, 1.0)).clamped().to_u8();
                        [r, g, b]
                    })
                    .collect();
                let (row, start, end) = match geometry.and_then(|g| g.targets.get(t).map(|x| (g, x))) {
                    Some((g, target)) => {
                        let (start, end) = match &target.spec {
                            TargetSpec::Strip { logical_start, logical_end, .. } => (*logical_start, *logical_end),
                            TargetSpec::Fixture { position, .. } => (*position, *position),
                        };
                        (g.rows.get(t).copied().unwrap_or(0), start, end)
                    }
                    None => (t, 0.0, 1.0),
                };
                PreviewStrip { row, start, end, colors }
            })
            .collect();
        shared.display = Some(Box::new(StackDisplay {
            pixels: self.composed.iter().map(Vec::len).sum(),
            preview,
            rows: geometry.map_or(1, |g| g.row_count()),
            levels: self.levels.clone(),
        }));
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
