//! Effect Stack — composes multiple patterns with blend modes onto an
//! LED-strip group.

use serde::{Deserialize, Serialize};

use crate::color::{BlendMode, Rgb};
use crate::dmx_io::SharedObjectStore;
use crate::engine::patterns::{create_pattern, pattern_channel_count, Pattern, Pixel, StripFrame};
use crate::engine::types::*;
use crate::objects::channel::ChannelKind;
use crate::objects::group::StripLayout;

/// Display state for the widget (group/strip count + per-layer summary).
pub struct EffectStackDisplay {
    pub group_ids: Vec<u32>,
    pub group_names: Vec<String>,
    pub strip_count: usize,
    pub layer_count: usize,
}

/// Persistent layer config — what gets sent in `pending_config` and saved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectLayerConfig {
    pub pattern_type: String,
    pub blend: BlendMode,
    pub opacity: f32,
}

/// Layer at runtime: the boxed pattern + its config.
struct RuntimeLayer {
    pattern: Box<dyn Pattern>,
    blend: BlendMode,
    opacity: f32,
    /// Pre-computed input channel count for this layer.
    channel_count: usize,
}

/// One render target. Either an LED strip with N pixels and a logical
/// span, or a single non-strip fixture at a fixed logical position.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StripTarget {
    Strip {
        object_id: u32,
        logical_start: f32,
        logical_end: f32,
    },
    Fixture {
        object_id: u32,
        /// Where on the group's 0..1 axis this single fixture sits.
        position: f32,
    },
}

impl StripTarget {
    fn object_id(&self) -> u32 {
        match self {
            StripTarget::Strip { object_id, .. } => *object_id,
            StripTarget::Fixture { object_id, .. } => *object_id,
        }
    }
}

pub struct EffectStackProcessNode {
    id: NodeId,
    group_ids: Vec<u32>,
    group_names: Vec<String>,
    strips: Vec<StripTarget>,
    layers: Vec<RuntimeLayer>,

    /// Total input channel count = sum of layers' channel counts.
    total_input_channels: usize,
    /// Channel-flat input buffer.
    input_values: Vec<f32>,
    /// Cached input port list (rebuilt when layers change).
    inputs: Vec<PortDef>,

    object_store: SharedObjectStore,
}

impl EffectStackProcessNode {
    pub fn new(id: NodeId, object_store: SharedObjectStore) -> Self {
        Self {
            id,
            group_ids: Vec::new(),
            group_names: Vec::new(),
            strips: Vec::new(),
            layers: Vec::new(),
            total_input_channels: 0,
            input_values: Vec::new(),
            inputs: Vec::new(),
            object_store,
        }
    }

    /// Rebuild `inputs` and `input_values` from the current layer list.
    /// Each layer's ports are prefixed `L{n}.` to disambiguate.
    fn rebuild_inputs(&mut self) {
        let mut ports = Vec::new();
        let mut total = 0usize;
        for (i, layer) in self.layers.iter().enumerate() {
            for port in layer.pattern.input_ports() {
                let prefixed = PortDef::new(
                    format!("L{}.{}", i + 1, port.name),
                    port.port_type,
                );
                total += port.port_type.channel_count();
                ports.push(prefixed);
            }
        }
        self.inputs = ports;
        self.total_input_channels = total;
        self.input_values.resize(total, 0.0);
    }
}

impl ProcessNode for EffectStackProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Effect Stack" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, ch: usize, v: f32) {
        if ch < self.input_values.len() { self.input_values[ch] = v; }
    }
    fn read_input(&self, ch: usize) -> f32 {
        self.input_values.get(ch).copied().unwrap_or(0.0)
    }

    fn process(&mut self) {
        if self.strips.is_empty() || self.layers.is_empty() {
            return;
        }

        let mut store = self.object_store.lock().unwrap();

        // ---- Build a uniform per-target render plan ----
        // For each target, produce: (layout, pixel_count). Single fixtures are
        // treated as a 1-pixel strip with logical_start == logical_end == position.
        let mut layouts: Vec<StripLayout> = Vec::with_capacity(self.strips.len());
        let mut pixel_counts: Vec<usize> = Vec::with_capacity(self.strips.len());
        let mut composited: Vec<Vec<Rgb>> = Vec::with_capacity(self.strips.len());

        for st in &self.strips {
            match st {
                StripTarget::Strip { object_id, logical_start, logical_end } => {
                    let pc = store.objects.iter()
                        .find(|o| o.id == *object_id)
                        .and_then(|o| o.channels.iter().find(|c| matches!(c.kind, ChannelKind::LedStrip { .. })))
                        .map(|c| c.pixel_count())
                        .unwrap_or(0);
                    layouts.push(StripLayout {
                        object_id: *object_id,
                        logical_start: *logical_start,
                        logical_end: *logical_end,
                    });
                    pixel_counts.push(pc);
                    composited.push(vec![Rgb::BLACK; pc]);
                }
                StripTarget::Fixture { object_id, position } => {
                    // Single-pixel "strip" at a fixed point on the axis.
                    layouts.push(StripLayout {
                        object_id: *object_id,
                        logical_start: *position,
                        logical_end: *position,
                    });
                    pixel_counts.push(1);
                    composited.push(vec![Rgb::BLACK; 1]);
                }
            }
        }

        // Per-layer scratch buffer reused across strips.
        let mut layer_scratch: Vec<Vec<Pixel>> = pixel_counts.iter()
            .map(|&n| vec![Pixel::TRANSPARENT; n])
            .collect();

        // Walk layers bottom → top.
        let mut input_cursor = 0usize;
        for layer in &self.layers {
            let n = layer.channel_count;
            let lo = input_cursor;
            let hi = (input_cursor + n).min(self.input_values.len());
            input_cursor = hi;
            let layer_inputs = &self.input_values[lo..hi];

            let mut frames: Vec<StripFrame<'_>> = layer_scratch.iter_mut().enumerate()
                .map(|(i, buf)| StripFrame {
                    layout: &layouts[i],
                    pixel_count: pixel_counts[i],
                    buf: buf.as_mut_slice(),
                })
                .collect();

            layer.pattern.render(layer_inputs, &mut frames);

            for (s, scratch) in layer_scratch.iter().enumerate() {
                for (i, pixel) in scratch.iter().enumerate() {
                    let weight = (pixel.alpha * layer.opacity).clamp(0.0, 1.0);
                    if weight > 0.0 {
                        composited[s][i] = layer.blend.blend(composited[s][i], pixel.color, weight);
                    }
                }
            }
        }

        // ---- Write composited buffers back to channels ----
        for (s, st) in self.strips.iter().enumerate() {
            let obj = match store.objects.iter_mut().find(|o| o.id == st.object_id()) {
                Some(o) => o,
                None => continue,
            };
            match st {
                StripTarget::Strip { .. } => {
                    if let Some(ch) = obj.channels.iter_mut().find(|c| matches!(c.kind, ChannelKind::LedStrip { .. })) {
                        for (i, color) in composited[s].iter().enumerate() {
                            ch.set_pixel(i, color.clamped());
                        }
                    }
                }
                StripTarget::Fixture { .. } => {
                    // Write the single rendered color to the fixture's Color channel (if any).
                    let color = composited[s][0].clamped();
                    if let Some(ch) = obj.channels.iter_mut().find(|c| matches!(c.kind, ChannelKind::Color { .. })) {
                        ch.set_color(color);
                    }
                }
            }
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        if self.group_ids.is_empty() && self.layers.is_empty() {
            return None;
        }
        let layers: Vec<EffectLayerConfig> = self.layers.iter()
            .map(|l| EffectLayerConfig {
                pattern_type: l.pattern.type_name().to_string(),
                blend: l.blend,
                opacity: l.opacity,
            })
            .collect();
        Some(serde_json::json!({
            "group_ids": self.group_ids,
            "group_names": self.group_names,
            "strips": self.strips,
            "layers": layers,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        self.group_ids = data.get("group_ids")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_u64().map(|n| n as u32)).collect())
            .unwrap_or_default();
        self.group_names = data.get("group_names")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        self.strips = data.get("strips")
            .and_then(|v| serde_json::from_value::<Vec<StripTarget>>(v.clone()).ok())
            .unwrap_or_default();

        // Rebuild layers from configs.
        let layer_cfgs: Vec<EffectLayerConfig> = data.get("layers")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        self.layers.clear();
        for cfg in layer_cfgs {
            if let Some(pattern) = create_pattern(&cfg.pattern_type) {
                let channel_count = pattern_channel_count(&cfg.pattern_type);
                self.layers.push(RuntimeLayer {
                    pattern,
                    blend: cfg.blend,
                    opacity: cfg.opacity,
                    channel_count,
                });
            }
        }
        self.rebuild_inputs();
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(EffectStackDisplay {
            group_ids: self.group_ids.clone(),
            group_names: self.group_names.clone(),
            strip_count: self.strips.len(),
            layer_count: self.layers.len(),
        }));
    }
}
