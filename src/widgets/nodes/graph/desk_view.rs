//! The Desk view: the nodes the user put there, from every level, without
//! wires, each drawn live the way a front-panel cell is, on pages.
//!
//! A node is the same node in both views; the Desk only knows a page, a place
//! and a size (`NodeSharedState::desk`). What both views do every frame to
//! keep the engine and the widgets in step is `NodeGraph::maintain`; this file
//! only draws and edits the placements.

use crate::widgets::nodes::panel::PARAM_CELL;
use super::desk::{DeskEntry, DeskKind, collect_desk};
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
    pub menu: Option<(NodeId, DeskKind, Pos2)>,
    drag: Option<ItemDrag>,
    /// Where the items of the page were drawn this frame, bottom to top:
    /// their title strip and their whole rectangle on screen.
    pub shown: Vec<(NodeId, DeskKind, Rect, Rect)>,
}

struct Renaming {
    page: u64,
    text: String,
    focused: bool,
}

/// A move or resize in progress, from where it started, so that the snapping
/// is applied to the whole drag and not summed up frame by frame.
#[derive(Clone)]
struct ItemDrag {
    id: NodeId,
    part: DeskKind,
    what: Drag,
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
        // The canvas is not drawn, so nothing there would ever switch the
        // hover scope off; the engine would keep recording the port.
        self.hover_port = None;
        self.hover_samples.clear();
        crate::engine::probe::set_target(None);
        let mut outcome = DeskOutcome { to_patch: self.handle_jumps() };

        let reachable = self.reachable_levels();
        let mut entries = collect_desk(&self.levels, &reachable, &self.desk);
        if !entries.is_empty() && self.desk.pages.is_empty() {
            self.desk.ensure_page();
            entries = collect_desk(&self.levels, &reachable, &self.desk);
        }
        let locked = self.desk.locked;
        self.show_desk_bar(ui, locked);
        ui.add_space(2.0);

        let page = self.current_desk_page();
        let items: Vec<DeskEntry> = entries.into_iter().filter(|e| Some(e.page) == page).collect();
        self.desk_ui.shown.clear();
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
                && let Some((id, kind, _, _)) = self.desk_ui.shown.iter().rev().find(|(_, _, _, item)| item.contains(p))
            {
                self.desk_ui.menu = Some((*id, kind.clone(), p));
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

    /// One item: a node in its rectangle, or a node's snapshot row, and,
    /// unless the Desk is locked, the handles that move and resize it.
    fn draw_desk_item(&mut self, ui: &mut Ui, e: &DeskEntry, origin: Vec2, locked: bool, view: Rect) {
        let item = e.rect.translate(origin);
        if !item.intersects(view) {
            return;
        }
        // The widgets of the node's level are taken out for the time of the
        // drawing (panels nested in the node borrow other levels), and put
        // back below whatever happens in between.
        let mut nodes = std::mem::take(&mut self.levels[e.level].nodes);
        let (min, strip) = match &e.kind {
            DeskKind::Node => {
                let layout = nodes[e.index].layout();
                let min = geometry::item_min(
                    cell_geometry::cell_size(&layout, node_size(nodes[e.index].as_ref(), None)),
                    layout.title_h,
                );
                let (strip, cell) = geometry::strip_and_cell(item, layout.title_h);
                let painter = ui.painter().with_clip_rect(view);
                if layout.title_h <= 0.0 {
                    draw_strip(&painter, strip, nodes[e.index].display_title());
                }
                let mut paint = PanelPaint { ui: &mut *ui, clip: view, style: &None, zoom: 1.0 };
                draw_cell(&mut self.levels, &mut nodes, e.index, None, cell, "desk", &mut paint, 0);
                (min, strip)
            }
            DeskKind::Param(name) => {
                let (strip, body) = geometry::strip_and_cell(item, 0.0);
                let title = format!("{}: {}", nodes[e.index].display_title(), nodes[e.index].param_display(name).1);
                draw_param_item(ui, nodes[e.index].as_ref(), name, item, strip, body, &title, view);
                (geometry::item_min(PARAM_CELL, 0.0), strip)
            }
            DeskKind::Snapshots => {
                let (strip, body) = geometry::strip_and_cell(item, 0.0);
                let title = format!("{}: snapshots", nodes[e.index].display_title());
                draw_snapshot_row(ui, nodes[e.index].as_ref(), item, strip, body, &title, view);
                (geometry::snapshot_row_min(), strip)
            }
        };
        self.desk_ui.shown.push((e.id, e.kind.clone(), strip, item));

        if !locked {
            let id = ui.id().with(("desk_item", e.id.0, &e.kind));
            let handle = Rect::from_min_size(item.max - Vec2::splat(geometry::HANDLE), Vec2::splat(geometry::HANDLE));
            let strip_resp = ui.interact(strip, id.with("move"), Sense::drag()).on_hover_cursor(CursorIcon::Grab);
            let handle_resp = ui.interact(handle, id.with("size"), Sense::drag()).on_hover_cursor(CursorIcon::ResizeNwSe);
            let painter = ui.painter().with_clip_rect(view);
            painter.line_segment(
                [handle.right_top() + Vec2::new(0.0, 3.0), handle.left_bottom() + Vec2::new(3.0, 0.0)],
                Stroke::new(1.5_f32, theme::TEXT_DIM),
            );
            let mut new_rect = None;
            for (resp, what) in [(&strip_resp, Drag::Move), (&handle_resp, Drag::Resize)] {
                if resp.drag_started() {
                    let press = ui.input(|i| i.pointer.press_origin()).unwrap_or(item.center());
                    self.desk_ui.drag = Some(ItemDrag { id: e.id, part: e.kind.clone(), what, start: e.rect, press });
                }
                if resp.dragged()
                    && let Some(d) = self.desk_ui.drag.clone().filter(|d| d.id == e.id && d.part == e.kind && d.what == what)
                    && let Some(p) = ui.input(|i| i.pointer.interact_pos())
                {
                    new_rect = Some(geometry::dragged(d.start, p - d.press, what, min));
                }
                if resp.drag_stopped() {
                    self.desk_ui.drag = None;
                }
            }
            let dragging = self.desk_ui.drag.as_ref().is_some_and(|d| d.id == e.id && d.part == e.kind);
            if dragging || strip_resp.hovered() || handle_resp.hovered() {
                painter.rect_stroke(item, NODE_CORNER_RADIUS, Stroke::new(1.5_f32, theme::SEM_PRIMARY), StrokeKind::Inside);
            }
            if let Some(r) = new_rect {
                DeskItem::edit(&mut nodes[e.index].shared_state().lock().desk, |d| {
                    e.kind.put(d, Some(geometry::desk_rect(e.page, r)));
                });
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
        let mut toggle_lock = false;
        let mut fullscreen = false;
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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let on_screen = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
                let (glyph, hover) = if on_screen { (icon::ARROWS_IN, "Leave full screen (F11)") } else { (icon::ARROWS_OUT, "Full screen (F11)") };
                fullscreen = ui.button(glyph).on_hover_text(hover).clicked();
                let (glyph, hover) = if locked {
                    (icon::LOCK, "Unlock: items can be moved and edited again")
                } else {
                    (icon::LOCK_OPEN, "Lock: only playing, no moving, resizing or editing")
                };
                toggle_lock = ui.selectable_label(locked, format!("{glyph} Lock")).on_hover_text(hover).clicked();
            });
        });
        if toggle_lock {
            self.desk.locked = !locked;
            self.desk_ui.renaming = None;
            self.desk_ui.menu = None;
            self.desk_ui.drag = None;
            self.desk_ui.confirm_delete = None;
        }
        if fullscreen {
            geometry::toggle_fullscreen(ui.ctx());
        }

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
                DeskItem::edit(&mut node.shared_state().lock().desk, |d| e.kind.put(d, Some(geometry::desk_rect(target, rect))));
            }
        }
        self.desk.remove_page(id);
        if self.desk_ui.page == Some(id) {
            self.desk_ui.page = next;
        }
    }

    fn show_desk_menu(&mut self, ui: &mut Ui, just_opened: bool, outcome: &mut DeskOutcome) {
        let Some((id, kind, pos)) = self.desk_ui.menu.clone() else { return };
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
            self.remove_from_desk(id, kind);
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

fn draw_strip(painter: &Painter, strip: Rect, title: &str) {
    painter.rect_filled(strip, NODE_CORNER_RADIUS, theme::BG_HIGH);
    painter.text(
        strip.left_center() + Vec2::new(6.0, 0.0),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(10.0),
        theme::TEXT_DIM,
    );
}

/// One setting of a node in its own box: the strip with "<node>: <setting>"
/// and the inspector's own control for it below.
fn draw_param_item(ui: &mut Ui, node: &dyn NodeWidget, name: &str, item: Rect, strip: Rect, body: Rect, title: &str, view: Rect) {
    let painter = ui.painter().with_clip_rect(view);
    painter.rect_filled(item, NODE_CORNER_RADIUS, theme::BG);
    painter.rect_stroke(item, NODE_CORNER_RADIUS, Stroke::new(1.0_f32, theme::STROKE), StrokeKind::Inside);
    draw_strip(&painter, strip, title);
    let content = body.shrink2(Vec2::new(4.0, 0.0));
    let clip = content.intersect(view);
    if content.width() <= 0.0 || content.height() <= 0.0 || clip.width() <= 0.0 || clip.height() <= 0.0 {
        return;
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("desk_param", node.node_id().0, name))
            .max_rect(content)
            .layout(egui::Layout::top_down(egui::Align::LEFT)),
    );
    child.set_clip_rect(clip);
    crate::widgets::inspector::param_control(&mut child, node, name);
}

/// A node's snapshots as a row of buttons, the one in effect lit. A click
/// recalls it the way the inspector's list does (`pending_recall`), so it
/// fades or cuts as the node is set to, and works on a locked Desk.
fn draw_snapshot_row(ui: &mut Ui, node: &dyn NodeWidget, item: Rect, strip: Rect, body: Rect, title: &str, view: Rect) {
    let (snapshots, active) = {
        let shared = node.shared_state().lock();
        let list: Vec<(String, u64)> = shared.snapshots.snapshots.iter().map(|s| (s.name.clone(), s.id)).collect();
        (list, shared.snapshot_active.or(shared.snapshots.active))
    };
    let painter = ui.painter().with_clip_rect(view);
    painter.rect_filled(item, NODE_CORNER_RADIUS, theme::BG);
    painter.rect_stroke(item, NODE_CORNER_RADIUS, Stroke::new(1.0_f32, theme::STROKE), StrokeKind::Inside);
    draw_strip(&painter, strip, title);
    if snapshots.is_empty() {
        painter.text(body.center(), egui::Align2::CENTER_CENTER, "no snapshots yet", egui::FontId::proportional(11.0), theme::TEXT_DIM);
        return;
    }
    let clip = body.intersect(view);
    let painter = ui.painter().with_clip_rect(clip);
    let buttons = geometry::snapshot_buttons(body, snapshots.len());
    for (i, ((name, id), rect)) in snapshots.iter().zip(buttons).enumerate() {
        if !rect.intersects(clip) {
            continue;
        }
        let resp = ui.interact(rect.intersect(clip), ui.id().with(("desk_snapshot", node.node_id().0, *id)), Sense::click());
        let lit = active == Some(*id);
        let fill = if lit { theme::SEM_SUCCESS } else if resp.hovered() { theme::BG_HIGH } else { theme::BG_DEEP };
        painter.rect_filled(rect, NODE_CORNER_RADIUS, fill);
        painter.rect_stroke(rect, NODE_CORNER_RADIUS, Stroke::new(1.0_f32, theme::STROKE), StrokeKind::Inside);
        painter.with_clip_rect(rect.intersect(clip)).text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            name,
            egui::FontId::proportional(11.0),
            if lit { theme::ON_LIGHT } else { theme::TEXT },
        );
        if resp.clicked() {
            node.shared_state().lock().pending_recall = Some(i);
        }
    }
}
