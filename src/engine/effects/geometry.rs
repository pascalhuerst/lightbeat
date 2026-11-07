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
    Strip {
        object_id: u32,
        logical_start: f32,
        logical_end: f32,
        /// Height in the group's space, 0..1 from top; none = in a row of its own order.
        #[serde(default)]
        y: Option<f32>,
    },
    Fixture {
        object_id: u32,
        position: f32,
        #[serde(default)]
        y: Option<f32>,
    },
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
    /// Row of each target, counted from the top by height. Targets without a
    /// height of their own are put in rows by their spans: strips whose spans
    /// on the group's axis only touch (the two halves of a shelf row, each
    /// starting at the centre) share a row; a strip that overlaps one already
    /// there starts the next row.
    pub rows: Vec<usize>,
    /// Height of each target in the group's space, 0..1 from the top.
    pub ys: Vec<f32>,
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
        Self::from_targets(targets)
    }

    /// Heights the effects will see for strips given as (object, start, end,
    /// height), for the group editor's picture.
    pub fn from_specs_for_layout(strips: &[(u32, f32, f32, Option<f32>)]) -> std::collections::HashMap<u32, f32> {
        let targets = strips
            .iter()
            .map(|(id, start, end, y)| Target {
                spec: TargetSpec::Strip { object_id: *id, logical_start: *start, logical_end: *end, y: *y },
                count: 2,
            })
            .collect();
        let g = Self::from_targets(targets);
        strips.iter().zip(&g.ys).map(|((id, ..), y)| (*id, *y)).collect()
    }

    fn from_targets(targets: Vec<Target>) -> Self {
        let auto = assign_rows(&targets);
        let auto_count = auto.iter().max().map_or(1, |m| m + 1);
        let ys: Vec<f32> = targets
            .iter()
            .zip(&auto)
            .map(|(t, row)| match explicit_y(&t.spec) {
                Some(y) => y.clamp(0.0, 1.0),
                None => (*row as f32 + 0.5) / auto_count as f32,
            })
            .collect();
        let rows = rank_rows(&ys);
        Self { targets, rows, ys }
    }

    pub fn row_count(&self) -> usize {
        self.rows.iter().max().map_or(1, |m| m + 1)
    }

    /// Position of pixel `i` of target `t` in the group's space: along the
    /// axis (x) and from the top (y), both on 0..1.
    pub fn position(&self, t: usize, i: usize) -> (f32, f32) {
        let target = &self.targets[t];
        let y = self.ys.get(t).copied().unwrap_or(0.5);
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

fn explicit_y(spec: &TargetSpec) -> Option<f32> {
    match spec {
        TargetSpec::Strip { y, .. } | TargetSpec::Fixture { y, .. } => *y,
    }
}

/// Row numbers from top to bottom for heights, equal heights sharing a row.
fn rank_rows(ys: &[f32]) -> Vec<usize> {
    const SAME: f32 = 1e-3;
    let mut distinct: Vec<f32> = Vec::new();
    let mut sorted = ys.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    for y in sorted {
        if distinct.last().is_none_or(|l| y - l > SAME) {
            distinct.push(y);
        }
    }
    ys.iter().map(|y| distinct.iter().position(|d| (y - d).abs() <= SAME).unwrap_or(0)).collect()
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
        if explicit_y(&target.spec).is_some() {
            rows.push(0);
            continue;
        }
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
        Target { spec: TargetSpec::Strip { object_id: id, logical_start: start, logical_end: end, y: None }, count: 10 }
    }

    fn geometry(targets: Vec<Target>) -> Geometry {
        Geometry::from_targets(targets)
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

    #[test]
    fn a_height_set_by_hand_wins_over_the_automatic_rows() {
        let mut low = strip(2, 0.0, 1.0);
        low.spec = TargetSpec::Strip { object_id: 2, logical_start: 0.0, logical_end: 1.0, y: Some(0.9) };
        let mut top = strip(1, 0.0, 1.0);
        top.spec = TargetSpec::Strip { object_id: 1, logical_start: 0.0, logical_end: 1.0, y: Some(0.1) };
        let g = geometry(vec![low, top]);
        assert_eq!(g.position(0, 0).1, 0.9);
        assert_eq!(g.position(1, 0).1, 0.1);
        assert_eq!(g.rows, vec![1, 0], "rows count from the top");
    }

    #[test]
    fn fixtures_can_stand_anywhere_and_share_a_row_with_a_strip() {
        let fixture = Target { spec: TargetSpec::Fixture { object_id: 3, position: 0.25, y: Some(0.5) }, count: 1 };
        let g = geometry(vec![strip(1, 0.0, 1.0), fixture]);
        assert_eq!(g.position(1, 0), (0.25, 0.5));
        assert_eq!(g.rows, vec![0, 0]);
    }
}
