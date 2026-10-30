//! Where the pixels of a stack are.

use serde::{Deserialize, Serialize};

use super::{LayerBuffer, Pixel};
use crate::dmx_io::ObjectStore;
use crate::objects::channel::ChannelKind;

/// A render target as configured: an LED strip mapped onto the group's 0..1
/// axis, or a single fixture at one point of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetSpec {
    Strip { object_id: u32, logical_start: f32, logical_end: f32 },
    Fixture { object_id: u32, position: f32 },
}

impl TargetSpec {
    pub fn object_id(&self) -> u32 {
        match self {
            TargetSpec::Strip { object_id, .. } | TargetSpec::Fixture { object_id, .. } => *object_id,
        }
    }
}

/// A target with its pixel count looked up.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub spec: TargetSpec,
    pub count: usize,
}

/// Everything an effect needs to know about where to draw.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Geometry {
    pub targets: Vec<Target>,
}

impl Geometry {
    /// Look up pixel counts for `specs`. Objects that are gone or have no
    /// strip get zero pixels and are skipped when drawing.
    pub fn resolve(specs: &[TargetSpec], store: &ObjectStore) -> Self {
        let targets = specs
            .iter()
            .map(|spec| {
                let count = match spec {
                    TargetSpec::Strip { object_id, .. } => store
                        .objects
                        .iter()
                        .find(|o| o.id == *object_id)
                        .and_then(|o| o.channels.iter().find(|c| matches!(c.kind, ChannelKind::LedStrip { .. })))
                        .map(|c| c.pixel_count())
                        .unwrap_or(0),
                    TargetSpec::Fixture { object_id, .. } => {
                        usize::from(store.objects.iter().any(|o| o.id == *object_id))
                    }
                };
                Target { spec: spec.clone(), count }
            })
            .collect();
        Self { targets }
    }

    /// Position of pixel `i` of target `t` in the group's space. The second
    /// coordinate is the row; there is only one row until groups get a real
    /// layout, but effects already measure distances in two dimensions.
    pub fn position(&self, t: usize, i: usize) -> (f32, f32) {
        let target = &self.targets[t];
        match &target.spec {
            TargetSpec::Strip { logical_start, logical_end, .. } => {
                let f = if target.count > 1 { i as f32 / (target.count - 1) as f32 } else { 0.5 };
                (logical_start + f * (logical_end - logical_start), 0.5)
            }
            TargetSpec::Fixture { position, .. } => (*position, 0.5),
        }
    }

    pub fn empty_buffer(&self) -> LayerBuffer {
        self.targets.iter().map(|t| vec![Pixel::CLEAR; t.count]).collect()
    }

    /// True when `buf` has the shape of this geometry.
    pub fn fits(&self, buf: &LayerBuffer) -> bool {
        buf.len() == self.targets.len() && buf.iter().zip(&self.targets).all(|(b, t)| b.len() == t.count)
    }
}
