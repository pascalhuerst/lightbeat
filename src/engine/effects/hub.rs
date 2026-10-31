//! Where effect layers and stack images meet.
//!
//! Effects publish layers, stacks publish their blended image, and a stack can
//! be sent to another stack. Only the stacks that are sent nowhere (the roots)
//! are written to the objects.

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
    pub name: String,
    /// The stack it is sent to; empty for a root, which writes to the groups.
    pub parent: String,
    /// How a root blends over the other roots on the same objects.
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
    /// Nodes (effects and stacks) whose image nothing shows right now: muted,
    /// at zero opacity or in a scene that is not on. They skip rendering.
    /// Anything not listed is wanted, so a node nobody has judged yet renders.
    pub unwanted: HashSet<u64>,
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

    /// Blend every root stack into the objects' pixels. Stacks sent to
    /// another stack have already been blended into it. Two roots on one
    /// object are blended in the order they were made.
    pub fn flatten(&self, store: &SharedObjectStore) {
        let hub = self.lock();

        let mut order: Vec<(&u64, &StackEntry)> = hub.stacks.iter().filter(|(_, s)| s.parent.is_empty()).collect();
        order.sort_by_key(|(id, _)| **id);

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
        for stack in hub.stacks.values().filter(|s| s.parent.is_empty()) {
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
///
/// Colours are not premultiplied, so the layer's share of the result is its
/// coverage out of the combined coverage: over an empty pixel it is the whole
/// colour (a half-opaque layer on its own keeps its colour and only its
/// alpha is half), over an opaque one it is just the coverage.
pub fn blend_layer(dst: &mut LayerBuffer, src: &LayerBuffer, blend: BlendMode, opacity: f32) {
    for (d_strip, s_strip) in dst.iter_mut().zip(src) {
        for (d, s) in d_strip.iter_mut().zip(s_strip) {
            let w = (s.alpha * opacity).clamp(0.0, 1.0);
            if w > 0.0 {
                let alpha = w + d.alpha * (1.0 - w);
                d.color = blend.blend(d.color, s.color, w / alpha);
                d.alpha = alpha;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn one(color: Rgb, alpha: f32) -> LayerBuffer {
        vec![vec![Pixel { color, alpha }]]
    }

    #[test]
    fn a_half_opaque_layer_alone_keeps_its_colour() {
        let mut dst = one(Rgb::BLACK, 0.0);
        blend_layer(&mut dst, &one(Rgb::WHITE, 1.0), BlendMode::Override, 0.5);
        assert_eq!(dst[0][0].color, Rgb::WHITE, "the colour is not dimmed twice");
        assert_eq!(dst[0][0].alpha, 0.5);
    }

    #[test]
    fn over_an_opaque_layer_the_coverage_is_the_share_and_alpha_stays_full() {
        let mut dst = one(Rgb::new(1.0, 0.0, 0.0), 1.0);
        blend_layer(&mut dst, &one(Rgb::new(0.0, 1.0, 0.0), 1.0), BlendMode::Override, 0.25);
        let p = dst[0][0];
        assert!((p.color.r - 0.75).abs() < 1e-6 && (p.color.g - 0.25).abs() < 1e-6);
        assert_eq!(p.alpha, 1.0);
    }

    #[test]
    fn two_translucent_layers_combine_their_coverage() {
        let mut dst = one(Rgb::BLACK, 0.0);
        blend_layer(&mut dst, &one(Rgb::new(1.0, 0.0, 0.0), 1.0), BlendMode::Override, 0.5);
        blend_layer(&mut dst, &one(Rgb::new(0.0, 0.0, 1.0), 1.0), BlendMode::Override, 0.5);
        let p = dst[0][0];
        assert!((p.alpha - 0.75).abs() < 1e-6);
        // The newer layer is a third of what is there: 0.5 of 0.75 coverage.
        assert!((p.color.b - 2.0 / 3.0).abs() < 1e-5 && (p.color.r - 1.0 / 3.0).abs() < 1e-5, "{p:?}");
    }
}
