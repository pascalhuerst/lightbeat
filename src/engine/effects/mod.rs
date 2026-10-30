//! Light effects for groups of LED strips and fixtures.
//!
//! An *effect* node renders its own layer of pixels about 60 times a second
//! and hands it to the `EffectHub` under the name of the stack it is linked
//! to. A *stack* node blends its effects' layers in order and publishes one
//! image. Once per frame the hub flattens every stack, by priority, into the
//! objects' pixels. Effects never touch the objects themselves, so they cannot
//! fight over them and their order is explicit.
//!
//! Effects come in two kinds: a **Loop** is a function of a phase (a wave that
//! travels once per bar), a **Burst** is spawned by a trigger and lives for
//! a while (a ripple from the kick).

pub mod fill;
pub mod geometry;
pub mod hub;
pub mod node;
pub mod ripple;
pub mod stack;
pub mod wave;

use std::time::{Duration, Instant};

use crate::color::Rgb;

pub use geometry::{Geometry, Target, TargetSpec};
pub use hub::EffectHub;

/// One pixel of a layer: colour plus how much of the pixel the layer covers.
#[derive(Debug, Clone, Copy)]
pub struct Pixel {
    pub color: Rgb,
    pub alpha: f32,
}

impl Pixel {
    pub const CLEAR: Self = Self { color: Rgb::BLACK, alpha: 0.0 };
}

/// One buffer per target of a geometry, one pixel per LED.
pub type LayerBuffer = Vec<Vec<Pixel>>;

/// How often layers are rendered and the result is written to the LEDs. The
/// interfaces send at about 44 Hz, so rendering a thousand times a second, as
/// the old group nodes did, only burns CPU.
pub const RENDER_HZ: f64 = 60.0;

/// Lets a node render at `RENDER_HZ` while it still processes inputs every tick.
pub struct RenderGate {
    interval: Duration,
    last: Option<Instant>,
}

impl RenderGate {
    pub fn new() -> Self {
        Self { interval: Duration::from_secs_f64(1.0 / RENDER_HZ), last: None }
    }

    pub fn due(&mut self) -> bool {
        let now = Instant::now();
        match self.last {
            Some(t) if now.duration_since(t) < self.interval => false,
            _ => {
                self.last = Some(now);
                true
            }
        }
    }
}

impl Default for RenderGate {
    fn default() -> Self {
        Self::new()
    }
}

/// Brightest value in a layer, for activity meters.
pub fn layer_level(buf: &LayerBuffer) -> f32 {
    buf.iter()
        .flatten()
        .map(|p| p.alpha * p.color.r.max(p.color.g).max(p.color.b))
        .fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::fill::Fill;
    use super::hub::StackEntry;
    use super::node::EffectNode;
    use super::ripple::Ripple;
    use super::stack::EffectStackProcessNode;
    use super::*;
    use crate::color::BlendMode;
    use crate::dmx_io::{ObjectStore, SharedObjectStore};
    use crate::engine::types::{NodeId, ProcessNode};
    use crate::objects::channel::{Channel, PixelFormat};
    use crate::objects::fixture::{DmxAddress, Fixture};
    use crate::objects::object::Object;

    fn store_with_strips(counts: &[usize]) -> SharedObjectStore {
        let mut objects = Vec::new();
        for (i, &n) in counts.iter().enumerate() {
            let mut f = Fixture::new(1, "strip");
            f.add_channel(Channel::led_strip("px", n, PixelFormat::rgb()));
            objects.push(Object::new(i as u32 + 1, "o", &f, DmxAddress::default()));
        }
        Arc::new(Mutex::new(ObjectStore { objects }))
    }

    fn pixel(store: &SharedObjectStore, object: u32, i: usize) -> Rgb {
        let s = store.lock().unwrap();
        s.objects.iter().find(|o| o.id == object).unwrap().channels[0].pixel(i)
    }

    fn stack_config(name: &str, layers: &[u64], targets: &[(u32, f32, f32)]) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "targets": targets.iter().map(|(id, a, b)| serde_json::json!({
                "kind": "strip", "object_id": id, "logical_start": a, "logical_end": b })).collect::<Vec<_>>(),
            "layers": layers.iter().map(|id| serde_json::json!({ "node_id": id })).collect::<Vec<_>>(),
        })
    }

    /// Run the nodes for a few frames, as the engine would.
    fn run(nodes: &mut [&mut dyn ProcessNode], hub: &EffectHub, store: &SharedObjectStore, frames: usize) {
        for _ in 0..frames {
            std::thread::sleep(Duration::from_millis(20));
            for n in nodes.iter_mut() {
                n.process();
            }
            hub.flatten(store);
        }
    }

    #[test]
    fn a_fill_in_a_stack_lights_the_strip() {
        let (hub, store) = (EffectHub::new(), store_with_strips(&[10]));
        let mut stack = EffectStackProcessNode::new(NodeId(1), hub.clone(), store.clone());
        stack.load_data(&stack_config("shelf", &[2], &[(1, 0.0, 1.0)]));
        let mut fill = EffectNode::new(NodeId(2), Fill::default(), hub.clone());
        fill.load_data(&serde_json::json!({ "stack": "shelf" }));

        run(&mut [&mut stack, &mut fill], &hub, &store, 4);
        for i in 0..10 {
            assert!(pixel(&store, 1, i).r > 0.99, "pixel {i}");
        }
    }

    #[test]
    fn muting_a_layer_removes_it_from_the_image() {
        let (hub, store) = (EffectHub::new(), store_with_strips(&[4]));
        let mut stack = EffectStackProcessNode::new(NodeId(1), hub.clone(), store.clone());
        let mut config = stack_config("shelf", &[2], &[(1, 0.0, 1.0)]);
        config["layers"][0]["muted"] = true.into();
        stack.load_data(&config);
        let mut fill = EffectNode::new(NodeId(2), Fill::default(), hub.clone());
        fill.load_data(&serde_json::json!({ "stack": "shelf" }));

        run(&mut [&mut stack, &mut fill], &hub, &store, 4);
        assert!(pixel(&store, 1, 0).r < 0.01);
    }

    #[test]
    fn a_trigger_sends_a_ring_out_from_the_origin_which_then_fades() {
        let (hub, store) = (EffectHub::new(), store_with_strips(&[101]));
        let mut stack = EffectStackProcessNode::new(NodeId(1), hub.clone(), store.clone());
        stack.load_data(&stack_config("shelf", &[2], &[(1, 0.0, 1.0)]));
        let mut ripple = EffectNode::new(NodeId(2), Ripple::default(), hub.clone());
        ripple.load_data(&serde_json::json!({ "stack": "shelf" }));
        // Narrow and fast, so a short test sees the ring move.
        ripple.set_param(1, crate::engine::types::ParamValue::Float(4.0));
        ripple.set_param(2, crate::engine::types::ParamValue::Float(0.2));
        ripple.set_param(3, crate::engine::types::ParamValue::Float(0.6));

        run(&mut [&mut stack, &mut ripple], &hub, &store, 2);
        assert!(pixel(&store, 1, 50).r < 0.01, "nothing before the trigger");

        ripple.write_input(0, 1.0);
        ripple.write_input(0, 0.0);
        // The stack blends the layers of the frame before, so allow two frames.
        run(&mut [&mut stack, &mut ripple], &hub, &store, 2);
        let near_centre: f32 = (20..=80).map(|i| pixel(&store, 1, i).r).fold(0.0, f32::max);
        assert!(near_centre > 0.5, "a ring is moving out from the centre ({near_centre})");
        assert!(pixel(&store, 1, 3).r < 0.01, "and has not reached the end");

        run(&mut [&mut stack, &mut ripple], &hub, &store, 8);
        let lit: f32 = (0..101).map(|i| pixel(&store, 1, i).r).sum();
        assert!(lit < 0.01, "gone once its decay time has passed, but {lit} is left");
    }

    #[test]
    fn a_trigger_pulse_shorter_than_a_frame_is_not_lost() {
        let (hub, store) = (EffectHub::new(), store_with_strips(&[11]));
        let mut stack = EffectStackProcessNode::new(NodeId(1), hub.clone(), store.clone());
        stack.load_data(&stack_config("shelf", &[2], &[(1, 0.0, 1.0)]));
        let mut ripple = EffectNode::new(NodeId(2), Ripple::default(), hub.clone());
        ripple.load_data(&serde_json::json!({ "stack": "shelf" }));
        run(&mut [&mut stack, &mut ripple], &hub, &store, 2);

        // High and low within one tick, before the effect ever sees a frame.
        ripple.write_input(0, 1.0);
        ripple.write_input(0, 0.0);
        ripple.process();
        assert_eq!(ripple.read_input(0), 0.0);
        run(&mut [&mut stack, &mut ripple], &hub, &store, 2);
        let lit: f32 = (0..11).map(|i| pixel(&store, 1, i).r).sum();
        assert!(lit > 0.1, "the ripple was spawned");
    }

    #[test]
    fn higher_priority_stacks_are_blended_over_lower_ones() {
        let (hub, store) = (EffectHub::new(), store_with_strips(&[3]));
        let geometry = Arc::new(Geometry::resolve(
            &[geometry::TargetSpec::Strip { object_id: 1, logical_start: 0.0, logical_end: 1.0 }],
            &store.lock().unwrap(),
        ));
        let solid = |c: Rgb| -> LayerBuffer {
            geometry.targets.iter().map(|t| vec![Pixel { color: c, alpha: 1.0 }; t.count]).collect()
        };
        let mut state = hub.lock();
        // Inserted high first, so only the priority can decide the order.
        state.stacks.insert(1, StackEntry { priority: 5, blend: BlendMode::Override, amount: 1.0, geometry: geometry.clone(), buf: solid(Rgb::new(0.0, 1.0, 0.0)) });
        state.stacks.insert(2, StackEntry { priority: 0, blend: BlendMode::Override, amount: 1.0, geometry: geometry.clone(), buf: solid(Rgb::new(1.0, 0.0, 0.0)) });
        drop(state);
        hub.flatten(&store);
        assert_eq!(pixel(&store, 1, 0), Rgb::new(0.0, 1.0, 0.0));

        // Halving the top stack's amount shows the one below through it.
        hub.lock().stacks.get_mut(&1).unwrap().amount = 0.5;
        hub.flatten(&store);
        let p = pixel(&store, 1, 0);
        assert!((p.r - 0.5).abs() < 0.02 && (p.g - 0.5).abs() < 0.02, "{p:?}");
    }

    #[test]
    fn a_removed_stack_blacks_out_its_strips() {
        let (hub, store) = (EffectHub::new(), store_with_strips(&[3]));
        let mut stack = EffectStackProcessNode::new(NodeId(1), hub.clone(), store.clone());
        stack.load_data(&stack_config("shelf", &[2], &[(1, 0.0, 1.0)]));
        let mut fill = EffectNode::new(NodeId(2), Fill::default(), hub.clone());
        fill.load_data(&serde_json::json!({ "stack": "shelf" }));
        run(&mut [&mut stack, &mut fill], &hub, &store, 4);
        assert!(pixel(&store, 1, 0).r > 0.9);

        drop(stack);
        hub.flatten(&store);
        assert!(pixel(&store, 1, 0).r < 0.01);
    }
}
