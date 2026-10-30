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
    /// Row of each target. Strips whose spans on the group's axis only touch
    /// (the two halves of a shelf row, each starting at the centre) share a
    /// row; a strip that overlaps one already there starts the next row.
    pub rows: Vec<usize>,
}

impl Geometry {
    /// Look up pixel counts for `specs`. Objects that are gone or have no
    /// strip get zero pixels and are skipped when drawing.
    pub fn resolve(specs: &[TargetSpec], store: &ObjectStore) -> Self {
        let targets: Vec<Target> = specs
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
        let rows = assign_rows(&targets);
        Self { targets, rows }
    }

    pub fn row_count(&self) -> usize {
        self.rows.iter().max().map_or(1, |m| m + 1)
    }

    /// Position of pixel `i` of target `t` in the group's space: along the
    /// axis, and the middle of its row on 0..1 from first row to last.
    pub fn position(&self, t: usize, i: usize) -> (f32, f32) {
        let target = &self.targets[t];
        let y = (self.rows.get(t).copied().unwrap_or(0) as f32 + 0.5) / self.row_count() as f32;
        match &target.spec {
            TargetSpec::Strip { logical_start, logical_end, .. } => {
                let f = if target.count > 1 { i as f32 / (target.count - 1) as f32 } else { 0.5 };
                (logical_start + f * (logical_end - logical_start), y)
            }
            TargetSpec::Fixture { position, .. } => (*position, y),
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

fn span_of(spec: &TargetSpec) -> (f32, f32) {
    match spec {
        TargetSpec::Strip { logical_start, logical_end, .. } => {
            (logical_start.min(*logical_end), logical_start.max(*logical_end))
        }
        TargetSpec::Fixture { position, .. } => (*position, *position),
    }
}

fn assign_rows(targets: &[Target]) -> Vec<usize> {
    const TOUCH: f32 = 1e-4;
    let mut rows = Vec::with_capacity(targets.len());
    let mut row = 0;
    let mut spans: Vec<(f32, f32)> = Vec::new();
    for target in targets {
        let span = span_of(&target.spec);
        let overlaps = spans.iter().any(|s| span.0.max(s.0) + TOUCH < span.1.min(s.1));
        if overlaps {
            row += 1;
            spans.clear();
        }
        spans.push(span);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(id: u32, start: f32, end: f32) -> Target {
        Target { spec: TargetSpec::Strip { object_id: id, logical_start: start, logical_end: end }, count: 10 }
    }

    fn geometry(targets: Vec<Target>) -> Geometry {
        let rows = assign_rows(&targets);
        Geometry { targets, rows }
    }

    #[test]
    fn the_record_shelf_is_two_columns_by_three_rows() {
        let g = geometry(vec![
            strip(13, 0.5, 0.0), strip(14, 0.5, 1.0),
            strip(15, 0.5, 0.0), strip(16, 0.5, 1.0),
            strip(17, 0.5, 0.0), strip(18, 0.5, 1.0),
        ]);
        assert_eq!(g.rows, vec![0, 0, 1, 1, 2, 2]);
        assert_eq!(g.row_count(), 3);
        // LED 0 of both halves is at the centre of its row.
        assert_eq!(g.position(0, 0), (0.5, 1.0 / 6.0));
        assert_eq!(g.position(1, 0), (0.5, 1.0 / 6.0));
        assert_eq!(g.position(5, 9), (1.0, 5.0 / 6.0));
    }

    #[test]
    fn strips_over_the_whole_axis_each_get_a_row() {
        let g = geometry(vec![strip(1, 0.0, 1.0), strip(2, 0.0, 1.0)]);
        assert_eq!(g.rows, vec![0, 1]);
    }

    #[test]
    fn one_strip_stays_on_the_middle_line() {
        let g = geometry(vec![strip(1, 0.0, 1.0)]);
        assert_eq!(g.position(0, 0).1, 0.5);
    }
}
