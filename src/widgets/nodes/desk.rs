//! The Desk's own data and geometry: pages, the lock flag, snapping, finding a
//! free spot, and which page an item falls on. Everything here is pure and in
//! unzoomed pixels; the graph (`graph/desk.rs`) does the drawing.

use egui::{Pos2, Rect, Vec2};
use serde::{Deserialize, Serialize};

use crate::engine::types::DeskRect;

/// Items snap to this grid when they are moved or resized.
pub const GRID: f32 = 20.0;
/// The smallest an item can be dragged to.
pub const MIN_ITEM: Vec2 = Vec2::new(60.0, 40.0);
/// Where a new item looks for room: left to right up to this width, then a
/// new row. Wide enough for a desktop window, narrow enough to stay in view.
const SPOT_WIDTH: f32 = 1400.0;
/// Height of the strip along the top of an item that moves it.
pub const TITLE_H: f32 = 16.0;
/// Side of the square in an item's corner that resizes it.
pub const HANDLE: f32 = 12.0;

/// One tab of the Desk. The id never changes; the order is the list's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeskPage {
    pub id: u64,
    pub name: String,
}

/// What the Desk keeps once per project: the pages and whether it is locked.
/// Saved at the root level (`ProjectFile::desk`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeskDoc {
    pub pages: Vec<DeskPage>,
    pub locked: bool,
    /// Ids already handed out, so that a deleted page's id is never given to
    /// a new one (its items would land on it).
    pub issued: u64,
}

impl DeskDoc {
    /// A project without a Desk writes nothing.
    pub fn is_default(&self) -> bool {
        self.pages.is_empty() && !self.locked
    }

    fn next_id(&self) -> u64 {
        self.pages.iter().map(|p| p.id).max().unwrap_or(0).max(self.issued) + 1
    }

    /// The first page exists as soon as anything is put on the Desk, so there
    /// is always one to land on.
    pub fn ensure_page(&mut self) -> u64 {
        if let Some(p) = self.pages.first() {
            return p.id;
        }
        self.add_page()
    }

    pub fn add_page(&mut self) -> u64 {
        let id = self.next_id();
        self.issued = id;
        let name = format!("Page {}", self.pages.len() + 1);
        self.pages.push(DeskPage { id, name });
        id
    }

    pub fn rename(&mut self, id: u64, name: String) {
        if let Some(p) = self.pages.iter_mut().find(|p| p.id == id) {
            p.name = name;
        }
    }

    /// Moves the page one place; false at the ends.
    pub fn move_page(&mut self, id: u64, left: bool) -> bool {
        let Some(i) = self.pages.iter().position(|p| p.id == id) else { return false };
        let j = if left { i.checked_sub(1) } else { (i + 1 < self.pages.len()).then_some(i + 1) };
        let Some(j) = j else { return false };
        self.pages.swap(i, j);
        true
    }

    pub fn remove_page(&mut self, id: u64) {
        self.pages.retain(|p| p.id != id);
    }

    /// The page an item names, or the first one when it is gone.
    pub fn resolve(&self, page: u64) -> Option<u64> {
        if self.pages.iter().any(|p| p.id == page) {
            Some(page)
        } else {
            self.pages.first().map(|p| p.id)
        }
    }

    /// The page after `id` is removed (the one the Desk shows next).
    pub fn neighbour_of(&self, id: u64) -> Option<u64> {
        let i = self.pages.iter().position(|p| p.id == id)?;
        self.pages.get(i + 1).or_else(|| i.checked_sub(1).and_then(|j| self.pages.get(j))).map(|p| p.id)
    }
}

pub fn snap(v: f32) -> f32 {
    (v / GRID).round() * GRID
}

pub fn snap_pos(p: Pos2) -> Pos2 {
    Pos2::new(snap(p.x), snap(p.y))
}

/// A size on the grid, no smaller than `min` (itself rounded up to the grid).
pub fn snap_size(size: Vec2, min: Vec2) -> Vec2 {
    let up = |v: f32| (v / GRID).ceil() * GRID;
    Vec2::new(snap(size.x).max(up(min.x.max(MIN_ITEM.x))), snap(size.y).max(up(min.y.max(MIN_ITEM.y))))
}

/// Whether two rectangles share area; egui's `intersects` also counts a
/// shared edge, which would leave a gap between neighbours.
pub fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

/// The top-left of the first spot on the grid where `size` fits among
/// `taken`, scanning rows left to right, top to bottom.
pub fn free_spot(taken: &[Rect], size: Vec2) -> Pos2 {
    let mut y = 0.0;
    // Past the lowest item there is always room, so the scan ends.
    let bottom = taken.iter().map(|r| r.max.y).fold(0.0_f32, f32::max);
    while y <= bottom {
        let mut x = 0.0;
        while x + size.x <= SPOT_WIDTH.max(size.x) {
            let candidate = Rect::from_min_size(Pos2::new(x, y), size);
            if !taken.iter().any(|t| overlaps(t, &candidate)) {
                return candidate.min;
            }
            x += GRID;
        }
        y += GRID;
    }
    Pos2::new(0.0, snap(bottom + GRID))
}

pub fn rect_of(r: &DeskRect) -> Rect {
    Rect::from_min_size(Pos2::new(r.pos[0], r.pos[1]), Vec2::new(r.size[0], r.size[1]))
}

pub fn desk_rect(page: u64, rect: Rect) -> DeskRect {
    DeskRect { page, pos: [rect.min.x, rect.min.y], size: [rect.width(), rect.height()] }
}

/// What a drag of an item does to its rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drag {
    Move,
    Resize,
}

/// The rectangle after dragging by `delta` from the one at the start of the
/// drag, snapped; `min` is the smallest size the item can have.
pub fn dragged(start: Rect, delta: Vec2, drag: Drag, min: Vec2) -> Rect {
    match drag {
        Drag::Move => {
            let p = snap_pos(start.min + delta);
            Rect::from_min_size(Pos2::new(p.x.max(0.0), p.y.max(0.0)), start.size())
        }
        Drag::Resize => Rect::from_min_size(start.min, snap_size(start.size() + delta, min)),
    }
}

/// Where the title strip (the handle that moves an item) and the cell the
/// node is drawn in lie inside an item's rectangle. A node with a title bar
/// is moved by that; one without gets a strip of the Desk's own on top.
pub fn strip_and_cell(item: Rect, title_h: f32) -> (Rect, Rect) {
    if title_h > 0.0 {
        let strip = Rect::from_min_size(item.min, Vec2::new(item.width(), title_h.min(item.height())));
        (strip, item)
    } else {
        let h = TITLE_H.min(item.height());
        let strip = Rect::from_min_size(item.min, Vec2::new(item.width(), h));
        (strip, Rect::from_min_max(Pos2::new(item.min.x, item.min.y + h), item.max))
    }
}

/// The room an item needs at least, given the cell its node asks for.
pub fn item_min(cell_min: Vec2, title_h: f32) -> Vec2 {
    if title_h > 0.0 { cell_min } else { Vec2::new(cell_min.x, cell_min.y + TITLE_H) }
}

/// The size a node gets when it is put on the Desk: what it asks for, on the grid.
pub fn default_size(cell_min: Vec2, title_h: f32) -> Vec2 {
    let min = item_min(cell_min, title_h);
    snap_size(min, min)
}

/// Room the page needs, for the scroll area.
pub fn extent(rects: &[Rect]) -> Vec2 {
    rects.iter().fold(Vec2::ZERO, |m, r| Vec2::new(m.x.max(r.max.x), m.y.max(r.max.y)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(n: usize) -> DeskDoc {
        let mut d = DeskDoc::default();
        for _ in 0..n {
            d.add_page();
        }
        d
    }

    #[test]
    fn snapping_rounds_to_the_grid_and_sizes_never_go_below_the_minimum() {
        assert_eq!(snap(29.0), 20.0);
        assert_eq!(snap(31.0), 40.0);
        assert_eq!(snap_pos(Pos2::new(9.0, 11.0)), Pos2::new(0.0, 20.0));
        assert_eq!(snap_size(Vec2::new(5.0, 5.0), Vec2::new(100.0, 50.0)), Vec2::new(100.0, 60.0));
        assert_eq!(snap_size(Vec2::new(5.0, 5.0), Vec2::ZERO), Vec2::new(60.0, 40.0));
        assert_eq!(snap_size(Vec2::new(230.0, 95.0), Vec2::new(100.0, 50.0)), Vec2::new(240.0, 100.0));
    }

    #[test]
    fn a_drag_moves_or_resizes_on_the_grid() {
        let r = Rect::from_min_size(Pos2::new(40.0, 40.0), Vec2::new(100.0, 60.0));
        let moved = dragged(r, Vec2::new(17.0, -3.0), Drag::Move, Vec2::ZERO);
        assert_eq!(moved, Rect::from_min_size(Pos2::new(60.0, 40.0), Vec2::new(100.0, 60.0)));
        let grown = dragged(r, Vec2::new(31.0, 12.0), Drag::Resize, Vec2::new(80.0, 40.0));
        assert_eq!(grown.min, r.min);
        assert_eq!(grown.size(), Vec2::new(140.0, 80.0));
        let off = dragged(r, Vec2::new(-500.0, -500.0), Drag::Move, Vec2::ZERO);
        assert_eq!(off.min, Pos2::ZERO, "an item cannot leave the page at the top left");
        let shrunk = dragged(r, Vec2::new(-90.0, -90.0), Drag::Resize, Vec2::new(80.0, 40.0));
        assert_eq!(shrunk.size(), Vec2::new(80.0, 40.0));
    }

    #[test]
    fn a_new_item_takes_the_first_free_spot() {
        let size = Vec2::new(200.0, 100.0);
        assert_eq!(free_spot(&[], size), Pos2::ZERO);
        let first = Rect::from_min_size(Pos2::ZERO, size);
        assert_eq!(free_spot(&[first], size), Pos2::new(200.0, 0.0));
        // A hole left by a removed item is found again.
        let far = Rect::from_min_size(Pos2::new(400.0, 0.0), size);
        assert_eq!(free_spot(&[first, far], size), Pos2::new(200.0, 0.0));
    }

    #[test]
    fn a_full_row_wraps_to_the_next() {
        let size = Vec2::new(700.0, 100.0);
        let a = Rect::from_min_size(Pos2::ZERO, size);
        let b = Rect::from_min_size(Pos2::new(700.0, 0.0), size);
        assert_eq!(free_spot(&[a, b], size), Pos2::new(0.0, 100.0));
    }

    #[test]
    fn an_item_wider_than_the_row_still_gets_a_spot() {
        let size = Vec2::new(2000.0, 100.0);
        let a = Rect::from_min_size(Pos2::ZERO, size);
        assert_eq!(free_spot(&[a], size), Pos2::new(0.0, 100.0));
    }

    #[test]
    fn items_never_overlap_the_spot_found_for_them() {
        let size = Vec2::new(180.0, 90.0);
        let mut taken: Vec<Rect> = Vec::new();
        for _ in 0..40 {
            let p = free_spot(&taken, size);
            let r = Rect::from_min_size(p, size);
            assert!(taken.iter().all(|t| !overlaps(t, &r)));
            taken.push(r);
        }
    }

    #[test]
    fn an_item_on_a_missing_page_falls_onto_the_first() {
        let d = doc(3);
        let ids: Vec<u64> = d.pages.iter().map(|p| p.id).collect();
        assert_eq!(d.resolve(ids[2]), Some(ids[2]));
        assert_eq!(d.resolve(999), Some(ids[0]));
        assert_eq!(DeskDoc::default().resolve(1), None);
    }

    #[test]
    fn reordering_and_deleting_pages_repoints_nothing() {
        let mut d = doc(3);
        let ids: Vec<u64> = d.pages.iter().map(|p| p.id).collect();
        assert!(d.move_page(ids[2], true));
        assert_eq!(d.pages.iter().map(|p| p.id).collect::<Vec<_>>(), vec![ids[0], ids[2], ids[1]]);
        assert!(!d.move_page(ids[0], true), "the first cannot go further left");
        assert!(!d.move_page(ids[1], false), "nor the last further right");
        d.remove_page(ids[2]);
        assert_eq!(d.resolve(ids[1]), Some(ids[1]), "an item of a page that stays still finds it");
        assert_eq!(d.resolve(ids[2]), Some(ids[0]));
        // A new page never reuses the id of a deleted one.
        assert!(!ids.contains(&d.add_page()));
    }

    #[test]
    fn the_page_shown_after_a_delete_is_a_neighbour() {
        let d = doc(3);
        let ids: Vec<u64> = d.pages.iter().map(|p| p.id).collect();
        assert_eq!(d.neighbour_of(ids[1]), Some(ids[2]));
        assert_eq!(d.neighbour_of(ids[2]), Some(ids[1]));
        assert_eq!(doc(1).neighbour_of(1), None);
    }

    #[test]
    fn a_titled_node_is_moved_by_its_own_title_and_a_headerless_one_gets_a_strip() {
        let item = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(200.0, 80.0));
        let (strip, cell) = strip_and_cell(item, 14.0);
        assert_eq!(cell, item);
        assert_eq!(strip.size(), Vec2::new(200.0, 14.0));
        let (strip, cell) = strip_and_cell(item, 0.0);
        assert_eq!(strip.size(), Vec2::new(200.0, TITLE_H));
        assert_eq!(cell.min.y, item.min.y + TITLE_H);
        assert_eq!(cell.max, item.max);
        assert_eq!(item_min(Vec2::new(60.0, 40.0), 0.0), Vec2::new(60.0, 40.0 + TITLE_H));
        assert_eq!(default_size(Vec2::new(105.0, 33.0), 14.0), Vec2::new(120.0, 40.0));
    }

    #[test]
    fn a_default_doc_is_not_written() {
        assert!(DeskDoc::default().is_default());
        assert!(!doc(1).is_default());
        let locked = DeskDoc { locked: true, ..Default::default() };
        assert!(!locked.is_default());
    }
}
