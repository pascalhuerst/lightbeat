//! The Desk view: the nodes the user put there, from every level, without
//! wires, each drawn live the way a front-panel cell is, on pages.
//!
//! A node is the same node in both views; the Desk only knows a page, a place
//! and a size (`NodeSharedState::desk`). What both views do every frame to
//! keep the engine and the widgets in step is `NodeGraph::maintain`; this file
//! only draws and edits the placements.

use super::desk::{DeskEntry, collect_desk};
use super::panel::{PanelPaint, draw_cell};
use super::*;
use crate::engine::types::DeskItem;
use crate::widgets::nodes::desk::{self as geometry, Drag};
use crate::widgets::nodes::panel as cell_geometry;
use crate::widgets::setup_ui;
use egui_phosphor::regular as icon;

/// UI state of the Desk that is not part of the project.
#[derive(Default)]
pub(super) struct DeskUi {
    /// The page being shown; the first when it is unset or gone.
    pub page: Option<u64>,
    renaming: Option<Renaming>,
    confirm_delete: Option<u64>,
    /// The right-click menu: the node it is for and where it sits.
    menu: Option<(NodeId, Pos2)>,
    drag: Option<ItemDrag>,
}

struct Renaming {
    page: u64,
    text: String,
    focused: bool,
}

/// A move or resize in progress, from where it started, so that the snapping
/// is applied to the whole drag and not summed up frame by frame.
#[derive(Clone, Copy)]
struct ItemDrag {
    id: NodeId,
    kind: Drag,
    start: Rect,
    press: Pos2,
}

/// What the app has to do after a frame of the Desk.
#[derive(Default)]
pub struct DeskOutcome {
    /// Switch to the Patch view (a jump to a node was made).
    pub to_patch: bool,
}

impl NodeGraph {
    /// Draw the Desk into `ui`: the page tabs, then the items of the current
    /// page. Also runs what the Patch view runs besides drawing
    /// (`NodeGraph::maintain`), so the graph and the engine stay in step.
    pub fn show_desk(&mut self, ui: &mut Ui) -> DeskOutcome {
        self.maintain();
        let mut outcome = DeskOutcome { to_patch: self.handle_jumps() };

        let reachable = self.reachable_levels();
        let mut entries = collect_desk(&self.levels, &reachable, &self.desk);
        if !entries.is_empty() && self.desk.pages.is_empty() {
            self.desk.ensure_page();
            entries = collect_desk(&self.levels, &reachable, &self.desk);
        }
        // A Subgraph with a front panel draws its inner nodes; the panel's
        // cells are only known after this, and only for the levels the canvas
        // has open otherwise.
        let mut levels: Vec<usize> = entries.iter().map(|e| e.level).collect();
        levels.sort_unstable();
        levels.dedup();
        for li in levels {
            self.refresh_panels(li);
        }

        let locked = self.desk.locked;
        self.show_desk_bar(ui, locked);
        ui.add_space(2.0);

        let page = self.current_desk_page();
        let items: Vec<DeskEntry> = entries.into_iter().filter(|e| Some(e.page) == page).collect();
        let mut opened_menu = false;
        egui::ScrollArea::both().auto_shrink([false, false]).drag_to_scroll(false).show(ui, |ui| {
            let rects: Vec<Rect> = items.iter().map(|e| e.rect).collect();
            let room = geometry::extent(&rects) + Vec2::splat(geometry::GRID * 4.0);
            let view = ui.clip_rect();
            let size = Vec2::new(room.x.max(view.width()), room.y.max(view.height()));
            let (canvas, _) = ui.allocate_exact_size(size, Sense::hover());
            if !locked {
                draw_desk_grid(ui, canvas, view);
            }
            if items.is_empty() {
                ui.painter().text(
                    view.center(),
                    egui::Align2::CENTER_CENTER,
                    "Nothing on this page. Right-click a node in the patch and choose Show on Desk.",
                    egui::FontId::proportional(14.0),
                    theme::TEXT_DIM,
                );
            }
            let right_click = ui.input(|i| i.pointer.secondary_clicked()).then(|| ui.input(|i| i.pointer.interact_pos())).flatten();
            for e in &items {
                self.draw_desk_item(ui, e, canvas.min.to_vec2(), locked, view);
            }
            if !locked
                && let Some(p) = right_click
                && ui.rect_contains_pointer(canvas)
                && let Some(hit) = items.iter().rev().find(|e| e.rect.translate(canvas.min.to_vec2()).contains(p))
            {
                self.desk_ui.menu = Some((hit.id, p));
                opened_menu = true;
            }
        });

        if locked {
            self.desk_ui.menu = None;
        }
        self.show_desk_menu(ui, opened_menu, &mut outcome);
        self.show_delete_page_prompt(ui.ctx());
        outcome
    }

    /// One item: its node in its rectangle, and, unless the Desk is locked,
    /// the handles that move and resize it.
    fn draw_desk_item(&mut self, ui: &mut Ui, e: &DeskEntry, origin: Vec2, locked: bool, view: Rect) {
        let item = e.rect.translate(origin);
        if !item.intersects(view) {
            return;
        }
        // The widgets of the node's level are taken out for the time of the
        // drawing (panels nested in the node borrow other levels), and put
        // back below whatever happens in between.
        let mut nodes = std::mem::take(&mut self.levels[e.level].nodes);
        let layout = nodes[e.index].layout();
        let min = geometry::item_min(
            cell_geometry::cell_size(&layout, node_size(nodes[e.index].as_ref(), None)),
            layout.title_h,
        );
        let (strip, cell) = geometry::strip_and_cell(item, layout.title_h);

        {
            let painter = ui.painter().with_clip_rect(view);
            if layout.title_h <= 0.0 {
                painter.rect_filled(strip, NODE_CORNER_RADIUS, theme::BG_HIGH);
                painter.text(
                    strip.left_center() + Vec2::new(6.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    nodes[e.index].display_title(),
                    egui::FontId::proportional(10.0),
                    theme::TEXT_DIM,
                );
            }
            let mut paint = PanelPaint { ui: &mut *ui, clip: view, style: &None, zoom: 1.0 };
            draw_cell(&mut self.levels, &mut nodes, e.index, None, cell, "desk", &mut paint, 0);
        }

        if !locked {
            let id = ui.id().with(("desk_item", e.id.0));
            let handle = Rect::from_min_size(item.max - Vec2::splat(geometry::HANDLE), Vec2::splat(geometry::HANDLE));
            let strip_resp = ui.interact(strip, id.with("move"), Sense::drag()).on_hover_cursor(CursorIcon::Grab);
            let handle_resp = ui.interact(handle, id.with("size"), Sense::drag()).on_hover_cursor(CursorIcon::ResizeNwSe);
            let painter = ui.painter().with_clip_rect(view);
            painter.line_segment(
                [handle.right_top() + Vec2::new(0.0, 3.0), handle.left_bottom() + Vec2::new(3.0, 0.0)],
                Stroke::new(1.5_f32, theme::TEXT_DIM),
            );
            let mut new_rect = None;
            for (resp, kind) in [(&strip_resp, Drag::Move), (&handle_resp, Drag::Resize)] {
                if resp.drag_started() {
                    let press = ui.input(|i| i.pointer.press_origin()).unwrap_or(item.center());
                    self.desk_ui.drag = Some(ItemDrag { id: e.id, kind, start: e.rect, press });
                }
                if resp.dragged()
                    && let Some(d) = self.desk_ui.drag.filter(|d| d.id == e.id && d.kind == kind)
                    && let Some(p) = ui.input(|i| i.pointer.interact_pos())
                {
                    new_rect = Some(geometry::dragged(d.start, p - d.press, kind, min));
                }
                if resp.drag_stopped() {
                    self.desk_ui.drag = None;
                }
            }
            if self.desk_ui.drag.is_some_and(|d| d.id == e.id) || strip_resp.hovered() || handle_resp.hovered() {
                painter.rect_stroke(item, NODE_CORNER_RADIUS, Stroke::new(1.5_f32, theme::SEM_PRIMARY), StrokeKind::Inside);
            }
            if let Some(r) = new_rect {
                DeskItem::edit(&mut nodes[e.index].shared_state().lock().desk, |d| d.node = Some(geometry::desk_rect(e.page, r)));
            }
        }
        self.levels[e.level].nodes = nodes;
    }

    /// The tab bar: the pages, and (unless the Desk is locked) adding,
    /// renaming, reordering and deleting them.
    fn show_desk_bar(&mut self, ui: &mut Ui, locked: bool) {
        let pages = self.desk.pages.clone();
        let current = self.current_desk_page();
        let mut select = None;
        let mut rename_start = None;
        let mut rename_done: Option<Option<String>> = None;
        let mut add = false;
        let mut shift: Option<(u64, bool)> = None;
        let mut delete = None;
        ui.horizontal(|ui| {
            for (n, page) in pages.iter().enumerate() {
                if let Some(r) = self.desk_ui.renaming.as_mut().filter(|r| r.page == page.id) {
                    let edit = ui.add(egui::TextEdit::singleline(&mut r.text).desired_width(110.0));
                    if !r.focused {
                        edit.request_focus();
                        r.focused = true;
                    }
                    if edit.lost_focus() {
                        let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
                        rename_done = Some((!cancelled).then(|| r.text.trim().to_string()));
                    }
                    continue;
                }
                let tab = ui.selectable_label(Some(page.id) == current, &page.name);
                if tab.clicked() {
                    select = Some(page.id);
                }
                if tab.double_clicked() && !locked {
                    rename_start = Some(page.id);
                }
                if !locked && Some(page.id) == current {
                    for left in [true, false] {
                        let (glyph, hover, room) = if left {
                            (icon::CARET_LEFT, "Move the page left", n > 0)
                        } else {
                            (icon::CARET_RIGHT, "Move the page right", n + 1 < pages.len())
                        };
                        if ui.add_enabled(room, egui::Button::new(glyph).small()).on_hover_text(hover).clicked() {
                            shift = Some((page.id, left));
                        }
                    }
                    if ui.add_enabled(pages.len() > 1, egui::Button::new(icon::X).small()).on_hover_text("Delete this page").clicked() {
                        delete = Some(page.id);
                    }
                    ui.separator();
                }
            }
            if !locked && setup_ui::add_button(ui, "Page") {
                add = true;
            }
        });

        if let Some(id) = select {
            self.desk_ui.page = Some(id);
        }
        if let Some(id) = rename_start {
            let text = pages.iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_default();
            self.desk_ui.renaming = Some(Renaming { page: id, text, focused: false });
        }
        if let Some(done) = rename_done {
            if let (Some(name), Some(r)) = (done, self.desk_ui.renaming.as_ref())
                && !name.is_empty()
            {
                let page = r.page;
                self.desk.rename(page, name);
            }
            self.desk_ui.renaming = None;
        }
        if add {
            let id = self.desk.add_page();
            self.desk_ui.page = Some(id);
        }
        if let Some((id, left)) = shift {
            self.desk.move_page(id, left);
        }
        if let Some(id) = delete {
            if self.desk_items_on(id) == 0 {
                self.delete_desk_page(id);
            } else {
                self.desk_ui.confirm_delete = Some(id);
            }
        }
    }

    fn show_delete_page_prompt(&mut self, ctx: &egui::Context) {
        let Some(id) = self.desk_ui.confirm_delete else { return };
        let Some(name) = self.desk.pages.iter().find(|p| p.id == id).map(|p| p.name.clone()) else {
            self.desk_ui.confirm_delete = None;
            return;
        };
        let count = self.desk_items_on(id);
        let mut choice = None;
        egui::Window::new("Delete page")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("Delete \"{name}\"? Its {count} item(s) move to the first page; the nodes stay where they are in the patch."));
                ui.horizontal(|ui| {
                    if ui.button(egui::RichText::new("Delete").color(theme::SEM_DANGER)).clicked() {
                        choice = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(false);
                    }
                });
            });
        match choice {
            Some(true) => {
                self.delete_desk_page(id);
                self.desk_ui.confirm_delete = None;
            }
            Some(false) => self.desk_ui.confirm_delete = None,
            None => {}
        }
    }

    /// Remove a page; what is on it goes to the first page that is left, to a
    /// free spot there.
    pub(super) fn delete_desk_page(&mut self, id: u64) {
        let next = self.desk.neighbour_of(id);
        let target = self.desk.pages.iter().find(|p| p.id != id).map(|p| p.id);
        if let Some(target) = target {
            let reachable = self.reachable_levels();
            let entries = collect_desk(&self.levels, &reachable, &self.desk);
            let mut taken: Vec<Rect> = entries.iter().filter(|e| e.page == target).map(|e| e.rect).collect();
            for e in entries.iter().filter(|e| e.page == id) {
                let rect = Rect::from_min_size(geometry::free_spot(&taken, e.rect.size()), e.rect.size());
                taken.push(rect);
                let node = &self.levels[e.level].nodes[e.index];
                DeskItem::edit(&mut node.shared_state().lock().desk, |d| d.node = Some(geometry::desk_rect(target, rect)));
            }
        }
        self.desk.remove_page(id);
        if self.desk_ui.page == Some(id) {
            self.desk_ui.page = next;
        }
    }

    fn show_desk_menu(&mut self, ui: &mut Ui, just_opened: bool, outcome: &mut DeskOutcome) {
        let Some((id, pos)) = self.desk_ui.menu else { return };
        let mut show_in_patch = false;
        let mut remove = false;
        let area = egui::Area::new(egui::Id::new("desk_item_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(140.0);
                    if ui.button("Show in patch").on_hover_text("Open the level the node is on and select it").clicked() {
                        show_in_patch = true;
                    }
                    if ui.button("Remove from Desk").clicked() {
                        remove = true;
                    }
                });
            });
        if show_in_patch {
            self.desk_ui.menu = None;
            if self.go_to_node(id) {
                outcome.to_patch = true;
            }
        } else if remove {
            self.desk_ui.menu = None;
            self.remove_from_desk(id);
        } else if !just_opened
            && ui.input(|i| i.pointer.any_pressed())
            && let Some(p) = ui.input(|i| i.pointer.interact_pos())
            && !area.response.rect.contains(p)
        {
            self.desk_ui.menu = None;
        }
    }
}

/// Lines on the grid items snap to, so that the unlocked Desk shows where they land.
fn draw_desk_grid(ui: &Ui, canvas: Rect, view: Rect) {
    let painter = ui.painter().with_clip_rect(view);
    let step = geometry::GRID * 2.0;
    let mut x = canvas.min.x + ((view.min.x - canvas.min.x) / step).floor().max(0.0) * step;
    while x <= view.max.x {
        painter.line_segment([Pos2::new(x, view.min.y), Pos2::new(x, view.max.y)], Stroke::new(1.0_f32, theme::BG_GRID));
        x += step;
    }
    let mut y = canvas.min.y + ((view.min.y - canvas.min.y) / step).floor().max(0.0) * step;
    while y <= view.max.y {
        painter.line_segment([Pos2::new(view.min.x, y), Pos2::new(view.max.x, y)], Stroke::new(1.0_f32, theme::BG_GRID));
        y += step;
    }
}
