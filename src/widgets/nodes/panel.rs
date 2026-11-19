//! Geometry of a subgraph's front panel: the controls its inner nodes put on
//! the Subgraph node's body. Everything here is pure and in unzoomed pixels;
//! the graph (`graph/panel.rs`) does the drawing.

use egui::{Pos2, Rect, Vec2};

use super::node::NodeLayout;
use crate::engine::types::NodeId;
use crate::widgets::setup_ui::Move;

/// Space between two cells of the panel.
pub const PANEL_GAP: f32 = 4.0;

/// One line of a Subgraph's "Front panel" list in its inspector.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelRow {
    pub node_id: NodeId,
    /// The setting, for a row that is one setting of the node.
    pub param: Option<String>,
    pub label: String,
}

/// What the user asked of the list; the graph carries it out on the inner level.
#[derive(Clone, Debug, PartialEq)]
pub enum PanelRequest {
    /// Moves every item of that node: they stay together.
    Move(NodeId, Move),
    Remove(NodeId, Option<String>),
}

/// A single setting takes this much room: the node's title over the control.
pub const PARAM_CELL: Vec2 = Vec2::new(200.0, 40.0);

/// How a cell of a single setting is laid out.
pub fn param_layout() -> NodeLayout {
    NodeLayout::compact().with_side_inset(4.0)
}

/// Size of the cell a node takes on a panel: the node's own size, less the
/// room its ports would need under the title, which a panel has none of.
pub fn cell_size(layout: &NodeLayout, node_size: Vec2) -> Vec2 {
    Vec2::new(node_size.x, node_size.y - (layout.content_top - layout.title_h).max(0.0))
}

/// Where the widget draws inside its cell, in the cell's own scale `zoom`.
pub fn cell_content_rect(cell: Rect, layout: &NodeLayout, zoom: f32) -> Rect {
    Rect::from_min_max(
        Pos2::new(cell.min.x + (layout.side_inset + layout.gutter_left) * zoom, cell.min.y + layout.title_h * zoom),
        Pos2::new(cell.max.x - (layout.side_inset + layout.gutter_right) * zoom, cell.max.y - layout.bottom_pad * zoom),
    )
}

/// Cells placed left to right, wrapping to a new row when the next one does
/// not fit in `width`. A cell wider than `width` gets a row to itself.
pub struct Flow {
    /// Top-left corner of each cell, relative to the panel's corner.
    pub origins: Vec<Pos2>,
    pub size: Vec2,
}

pub fn flow(sizes: &[Vec2], width: f32) -> Flow {
    let mut origins = Vec::with_capacity(sizes.len());
    let (mut x, mut y, mut row_h, mut used_w) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
    for s in sizes {
        if x > 0.0 && x + s.x > width {
            y += row_h + PANEL_GAP;
            x = 0.0;
            row_h = 0.0;
        }
        origins.push(Pos2::new(x, y));
        x += s.x + PANEL_GAP;
        used_w = used_w.max(x - PANEL_GAP);
        row_h = row_h.max(s.y);
    }
    Flow { origins, size: Vec2::new(used_w, y + row_h) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_wrap_when_the_next_one_does_not_fit() {
        let sizes = [Vec2::new(60.0, 40.0), Vec2::new(60.0, 50.0), Vec2::new(60.0, 30.0)];
        let f = flow(&sizes, 130.0);
        assert_eq!(f.origins[0], Pos2::new(0.0, 0.0));
        assert_eq!(f.origins[1], Pos2::new(64.0, 0.0));
        // Third wraps under the first, below the taller of the row above.
        assert_eq!(f.origins[2], Pos2::new(0.0, 50.0 + PANEL_GAP));
        assert_eq!(f.size, Vec2::new(124.0, 50.0 + PANEL_GAP + 30.0));
    }

    #[test]
    fn a_narrower_panel_stacks_the_cells() {
        let sizes = [Vec2::new(60.0, 40.0), Vec2::new(60.0, 40.0)];
        let f = flow(&sizes, 100.0);
        assert_eq!(f.origins[1], Pos2::new(0.0, 40.0 + PANEL_GAP));
        assert_eq!(f.size, Vec2::new(60.0, 84.0));
    }

    #[test]
    fn an_empty_panel_takes_no_room() {
        let f = flow(&[], 100.0);
        assert_eq!(f.size, Vec2::ZERO);
    }

    #[test]
    fn a_cell_drops_the_port_room_under_the_title() {
        let l = NodeLayout::STANDARD;
        assert_eq!(cell_size(&l, Vec2::new(100.0, 80.0)).y, 80.0 - (l.content_top - l.title_h));
        let c = NodeLayout::compact();
        assert_eq!(cell_size(&c, Vec2::new(100.0, 80.0)).y, 80.0);
    }
}
