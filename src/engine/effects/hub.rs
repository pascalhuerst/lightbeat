//! Where effect layers and stack images meet.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::color::{BlendMode, Rgb};
use crate::dmx_io::SharedObjectStore;
use crate::objects::channel::ChannelKind;

use super::{Geometry, LayerBuffer, Pixel, TargetSpec};

/// A layer an effect node published.
pub struct LayerEntry {
    /// Name of the stack it belongs to.
    pub stack: String,
    pub buf: LayerBuffer,
    /// Brightest pixel, for the activity meter.
    pub level: f32,
}

/// A stack's blended image.
pub struct StackEntry {
    pub priority: i32,
    pub blend: BlendMode,
    pub amount: f32,
    pub geometry: Arc<Geometry>,
    pub buf: LayerBuffer,
}

#[derive(Default)]
pub struct HubState {
    /// Geometry per stack name, for the effects linked to it.
    pub geometry: HashMap<String, Arc<Geometry>>,
    /// Layers by effect node id.
    pub layers: HashMap<u64, LayerEntry>,
    /// Stack images by stack node id.
    pub stacks: HashMap<u64, StackEntry>,
    /// Objects written by the last flatten, to black out the ones dropped since.
    lit: HashSet<u32>,
}

#[derive(Clone, Default)]
pub struct EffectHub(Arc<Mutex<HubState>>);

impl EffectHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lock(&self) -> MutexGuard<'_, HubState> {
        self.0.lock().unwrap()
    }

    /// Blend every stack, by priority, into the objects' pixels.
    pub fn flatten(&self, store: &SharedObjectStore) {
        let hub = self.lock();

        let mut order: Vec<(&u64, &StackEntry)> = hub.stacks.iter().collect();
        order.sort_by_key(|(id, s)| (s.priority, **id));

        // Accumulate per object, so two stacks on overlapping groups blend.
        let mut canvas: HashMap<u32, Vec<Rgb>> = HashMap::new();
        for (_, stack) in order {
            if !stack.geometry.fits(&stack.buf) {
                continue;
            }
            for (target, pixels) in stack.geometry.targets.iter().zip(&stack.buf) {
                let acc = canvas
                    .entry(target.spec.object_id())
                    .or_insert_with(|| vec![Rgb::BLACK; target.count]);
                if acc.len() < pixels.len() {
                    acc.resize(pixels.len(), Rgb::BLACK);
                }
                for (a, p) in acc.iter_mut().zip(pixels) {
                    let weight = (p.alpha * stack.amount).clamp(0.0, 1.0);
                    if weight > 0.0 {
                        *a = stack.blend.blend(*a, p.color, weight);
                    }
                }
            }
        }

        // Which target kind each object is, for writing back.
        let mut kinds: HashMap<u32, bool> = HashMap::new();
        for stack in hub.stacks.values() {
            for t in &stack.geometry.targets {
                kinds.insert(t.spec.object_id(), matches!(t.spec, TargetSpec::Strip { .. }));
            }
        }

        let mut store = store.lock().unwrap();
        let mut lit_now = HashSet::new();
        for (object_id, pixels) in &canvas {
            lit_now.insert(*object_id);
            write_object(&mut store.objects, *object_id, pixels, kinds.get(object_id).copied().unwrap_or(true));
        }
        for object_id in hub.lit.iter().filter(|id| !lit_now.contains(id)) {
            write_object(&mut store.objects, *object_id, &[Rgb::BLACK], true);
        }
        drop(store);
        drop(hub);
        self.lock().lit = lit_now;
    }
}

fn write_object(objects: &mut [crate::objects::object::Object], id: u32, pixels: &[Rgb], is_strip: bool) {
    let Some(obj) = objects.iter_mut().find(|o| o.id == id) else { return };
    if is_strip {
        if let Some(ch) = obj.channels.iter_mut().find(|c| matches!(c.kind, ChannelKind::LedStrip { .. })) {
            let n = ch.pixel_count();
            // A blacked-out strip gets a single black pixel; spread it.
            for i in 0..n {
                let c = pixels.get(i).or(pixels.first()).copied().unwrap_or(Rgb::BLACK);
                ch.set_pixel(i, c.clamped());
            }
        }
    } else if let Some(ch) = obj.channels.iter_mut().find(|c| matches!(c.kind, ChannelKind::Color { .. })) {
        ch.set_color(pixels.first().copied().unwrap_or(Rgb::BLACK).clamped());
    }
}

/// Blend `src` over `dst`: `opacity` scales the layer's own coverage.
pub fn blend_layer(dst: &mut LayerBuffer, src: &LayerBuffer, blend: BlendMode, opacity: f32) {
    for (d_strip, s_strip) in dst.iter_mut().zip(src) {
        for (d, s) in d_strip.iter_mut().zip(s_strip) {
            let w = (s.alpha * opacity).clamp(0.0, 1.0);
            if w > 0.0 {
                d.color = blend.blend(d.color, s.color, w);
                d.alpha += w * (1.0 - d.alpha);
            }
        }
    }
}

/// Reset a buffer to fully transparent.
pub fn clear(buf: &mut LayerBuffer) {
    for strip in buf.iter_mut() {
        strip.fill(Pixel::CLEAR);
    }
}
