use egui::{self, Color32, Ui};

use crate::objects::channel::ChannelKind;
use crate::objects::group::{FixturePlacement, Group, StripLayout};
use crate::theme;
use crate::objects::object::Object;

/// Manages groups of objects.
pub struct GroupManager {
    pub groups: Vec<Group>,
    next_id: u32,
    pub needs_refresh: bool,
}

impl GroupManager {
    pub fn new() -> Self {
        Self { groups: Vec::new(), next_id: 1, needs_refresh: false }
    }

    pub fn from_groups(groups: Vec<Group>) -> Self {
        let next_id = groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
        Self { groups, next_id, needs_refresh: true }
    }

    pub fn show(&mut self, ui: &mut Ui, objects: &[Object]) {
        ui.heading("Groups");
        ui.separator();

        if self.groups.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No groups.");
        }

        let mut remove_id = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for group in &mut self.groups {
                ui.push_id(group.id, |ui| {
                    let caps = group.capabilities(objects);
                    let caps_str = caps.iter().map(|c| c.label()).collect::<Vec<_>>().join(", ");

                    egui::CollapsingHeader::new(
                        egui::RichText::new(&group.name).strong(),
                    )
                    .id_salt(group.id)
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Name:");
                            ui.text_edit_singleline(&mut group.name);
                        });

                        if !caps_str.is_empty() {
                            ui.colored_label(Color32::from_gray(140), format!("Capabilities: {}", caps_str));
                        }

                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Members (order = position on the group's axis)").strong());

                        let mut remove_obj_id = None;
                        let mut swap: Option<(usize, usize)> = None;
                        let n = group.object_ids.len();
                        for (i, oid) in group.object_ids.iter().enumerate() {
                            if let Some(obj) = objects.iter().find(|o| o.id == *oid) {
                                ui.horizontal(|ui| {
                                    ui.label(format!("{}. {}", i + 1, obj.name));
                                    if i > 0 && ui.small_button(egui_phosphor::regular::ARROW_UP).clicked() {
                                        swap = Some((i, i - 1));
                                    }
                                    if i + 1 < n && ui.small_button(egui_phosphor::regular::ARROW_DOWN).clicked() {
                                        swap = Some((i, i + 1));
                                    }
                                    if ui.small_button(egui_phosphor::regular::X).clicked() {
                                        remove_obj_id = Some(*oid);
                                    }
                                });
                            }
                        }
                        if let Some((a, b)) = swap {
                            group.object_ids.swap(a, b);
                            self.needs_refresh = true;
                        }
                        if let Some(oid) = remove_obj_id {
                            group.object_ids.retain(|id| *id != oid);
                            self.needs_refresh = true;
                        }

                        let available: Vec<&Object> = objects.iter()
                            .filter(|o| !group.object_ids.contains(&o.id))
                            .collect();
                        if !available.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Add:");
                                for obj in &available {
                                    if ui.small_button(&obj.name).clicked() {
                                        group.object_ids.push(obj.id);
                                        self.needs_refresh = true;
                                    }
                                }
                            });
                        }

                        // ---- Strip layout editor ----
                        // Show LED strip members and let the user assign each one a
                        // logical_start/logical_end on the group's 0..1 axis.
                        let strip_members: Vec<&Object> = group.object_ids.iter()
                            .filter_map(|oid| objects.iter().find(|o| o.id == *oid))
                            .filter(|o| o.channels.iter().any(|c| matches!(c.kind, ChannelKind::LedStrip { .. })))
                            .collect();

                        if !strip_members.is_empty() {
                            ui.add_space(4.0);
                            ui.separator();
                            ui.label(egui::RichText::new("LED Strip layout").strong());
                            ui.colored_label(
                                Color32::from_gray(120),
                                "Each strip's first/last LED maps onto the group's 0..1 axis. Set end<start to reverse.",
                            );

                            let mut layout_changed = false;
                            for obj in &strip_members {
                                ui.horizontal(|ui| {
                                    ui.label(format!("{}:", obj.name));

                                    // Get or insert the layout entry.
                                    let entry_idx = group.strip_layout.iter().position(|l| l.object_id == obj.id);
                                    let (mut start, mut end) = match entry_idx {
                                        Some(i) => (group.strip_layout[i].logical_start, group.strip_layout[i].logical_end),
                                        None => (0.0_f32, 1.0_f32),
                                    };

                                    let mut changed = false;
                                    if ui.add(
                                        egui::DragValue::new(&mut start)
                                            .range(0.0..=1.0).speed(0.01).fixed_decimals(2).prefix("start: ")
                                    ).changed() { changed = true; }
                                    if ui.add(
                                        egui::DragValue::new(&mut end)
                                            .range(0.0..=1.0).speed(0.01).fixed_decimals(2).prefix("end: ")
                                    ).changed() { changed = true; }

                                    if ui.small_button("Reverse").clicked() {
                                        std::mem::swap(&mut start, &mut end);
                                        changed = true;
                                    }

                                    let mut height = entry_idx.and_then(|i| group.strip_layout[i].y);
                                    let mut manual = height.is_some();
                                    if ui.checkbox(&mut manual, "height").on_hover_text("Place this strip at a height by hand; off puts it in a row by its order").changed() {
                                        height = manual.then_some(0.5);
                                        changed = true;
                                    }
                                    if let Some(y) = &mut height
                                        && ui.add(egui::DragValue::new(y).range(0.0..=1.0).speed(0.01).fixed_decimals(2)).changed()
                                    {
                                        changed = true;
                                    }

                                    if changed {
                                        match entry_idx {
                                            Some(i) => {
                                                group.strip_layout[i].logical_start = start;
                                                group.strip_layout[i].logical_end = end;
                                                group.strip_layout[i].y = height;
                                            }
                                            None => {
                                                group.strip_layout.push(StripLayout {
                                                    object_id: obj.id,
                                                    logical_start: start,
                                                    logical_end: end,
                                                    y: height,
                                                });
                                            }
                                        }
                                        layout_changed = true;
                                    }
                                });
                            }

                            layout_changed |= layout_canvas(ui, group, objects);

                            // Drop layout entries for objects no longer in the group.
                            let valid_ids: Vec<u32> = strip_members.iter().map(|o| o.id).collect();
                            let before = group.strip_layout.len();
                            group.strip_layout.retain(|l| valid_ids.contains(&l.object_id));
                            if group.strip_layout.len() != before {
                                layout_changed = true;
                            }

                            if layout_changed {
                                self.needs_refresh = true;
                            }
                        }

                        ui.add_space(4.0);
                        if ui.small_button("Delete group").clicked() {
                            remove_id = Some(group.id);
                        }
                    });
                });
            }
        });

        if let Some(id) = remove_id {
            self.groups.retain(|g| g.id != id);
            self.needs_refresh = true;
        }

        ui.separator();
        if ui.button("Add Group").clicked() {
            let id = self.next_id;
            self.next_id += 1;
            self.groups.push(Group::new(id, format!("Group {}", id)));
            self.needs_refresh = true;
        }
    }
}


/// The group's space as a picture: strips as lines from their first to last
/// LED (the dot is LED 0), fixtures as dots, top of the group at the top.
/// Drag one to place it; dragging a strip keeps its length.
fn layout_canvas(ui: &mut Ui, group: &mut Group, objects: &[Object]) -> bool {
    use egui::{Pos2, Rect, Sense, Stroke, Vec2};
    let mut changed = false;

    let is_strip = |o: &Object| o.channels.iter().any(|c| matches!(c.kind, ChannelKind::LedStrip { .. }));
    let fixtures: Vec<u32> = group
        .object_ids
        .iter()
        .filter_map(|oid| objects.iter().find(|o| o.id == *oid))
        .filter(|o| !is_strip(o) && o.channels.iter().any(|c| matches!(c.kind, ChannelKind::Color { .. })))
        .map(|o| o.id)
        .collect();
    let strips: Vec<u32> = group
        .object_ids
        .iter()
        .filter_map(|oid| objects.iter().find(|o| o.id == *oid))
        .filter(|o| is_strip(o))
        .map(|o| o.id)
        .collect();

    ui.add_space(4.0);
    ui.label(egui::RichText::new("Space").strong());
    ui.colored_label(theme::TEXT_DIM, "Drag strips and fixtures to place them. Effects measure distances here.");

    let size = Vec2::new(ui.available_width().clamp(160.0, 420.0), 150.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, theme::BG_DEEP);
    let inner = rect.shrink(10.0);
    let to_screen = |x: f32, y: f32| Pos2::new(inner.min.x + x * inner.width(), inner.min.y + y * inner.height());
    for k in 0..=4 {
        let f = k as f32 / 4.0;
        painter.line_segment([to_screen(0.0, f), to_screen(1.0, f)], Stroke::new(1.0_f32, theme::BG_GRID));
        painter.line_segment([to_screen(f, 0.0), to_screen(f, 1.0)], Stroke::new(1.0_f32, theme::BG_GRID));
    }

    // The heights strips get without one, as the effects will see them.
    let auto_specs: Vec<(u32, f32, f32, Option<f32>)> = group
        .strip_layout
        .iter()
        .map(|l| (l.object_id, l.logical_start, l.logical_end, l.y))
        .collect();
    let geometry = crate::engine::effects::Geometry::from_specs_for_layout(&auto_specs);

    let n_fix = fixtures.len();
    for (k, id) in strips.iter().enumerate() {
        let entry = group.strip_layout.iter().position(|l| l.object_id == *id);
        let (start, end, y) = match entry {
            Some(i) => {
                let l = &group.strip_layout[i];
                (l.logical_start, l.logical_end, geometry.get(&l.object_id).copied().unwrap_or(0.5))
            }
            None => (0.0, 1.0, 0.5),
        };
        let a = to_screen(start, y);
        let b = to_screen(end, y);
        let hit = Rect::from_two_pos(a, b).expand2(Vec2::new(4.0, 8.0));
        let resp = ui.interact(hit, ui.id().with(("space_strip", group.id, *id)), Sense::click_and_drag());
        let color = if resp.hovered() || resp.dragged() { theme::SEM_PRIMARY } else { theme::TEXT_MUTED };
        painter.line_segment([a, b], Stroke::new(3.0_f32, color));
        painter.circle_filled(a, 3.5, theme::SEM_WARNING);
        if let Some(o) = objects.iter().find(|o| o.id == *id) {
            painter.text(Pos2::new(a.x.min(b.x), a.y - 11.0), egui::Align2::LEFT_BOTTOM, &o.name, egui::FontId::proportional(10.0), theme::TEXT_DIM);
        }
        let _ = k;
        if resp.dragged() {
            let d = resp.drag_delta();
            let dx = d.x / inner.width();
            let (lo, hi) = (start.min(end), start.max(end));
            let dx = dx.clamp(-lo, 1.0 - hi);
            let new_y = (y + d.y / inner.height()).clamp(0.0, 1.0);
            match entry {
                Some(i) => {
                    group.strip_layout[i].logical_start += dx;
                    group.strip_layout[i].logical_end += dx;
                    group.strip_layout[i].y = Some(new_y);
                }
                None => group.strip_layout.push(StripLayout { object_id: *id, logical_start: start + dx, logical_end: end + dx, y: Some(new_y) }),
            }
            changed = true;
        }
    }

    for (k, id) in fixtures.iter().enumerate() {
        let placed = group.fixture_layout.iter().position(|f| f.object_id == *id);
        let (x, y) = match placed {
            Some(i) => (group.fixture_layout[i].x, group.fixture_layout[i].y),
            None => (if n_fix <= 1 { 0.5 } else { k as f32 / (n_fix - 1) as f32 }, 0.5),
        };
        let p = to_screen(x, y);
        let resp = ui.interact(Rect::from_center_size(p, Vec2::splat(14.0)), ui.id().with(("space_fix", group.id, *id)), Sense::click_and_drag());
        let color = if resp.hovered() || resp.dragged() { theme::SEM_PRIMARY } else { theme::TYPE_COLOR };
        painter.circle_filled(p, 5.0, color);
        if let Some(o) = objects.iter().find(|o| o.id == *id) {
            painter.text(p + Vec2::new(7.0, -2.0), egui::Align2::LEFT_BOTTOM, &o.name, egui::FontId::proportional(10.0), theme::TEXT_DIM);
        }
        if resp.dragged() {
            let d = resp.drag_delta();
            let nx = (x + d.x / inner.width()).clamp(0.0, 1.0);
            let ny = (y + d.y / inner.height()).clamp(0.0, 1.0);
            match placed {
                Some(i) => {
                    group.fixture_layout[i].x = nx;
                    group.fixture_layout[i].y = ny;
                }
                None => group.fixture_layout.push(FixturePlacement { object_id: *id, x: nx, y: ny }),
            }
            changed = true;
        }
        if resp.secondary_clicked()
            && let Some(i) = placed
        {
            group.fixture_layout.remove(i);
            changed = true;
        }
    }
    if !fixtures.is_empty() {
        ui.colored_label(theme::TEXT_DIM, "Right-click a fixture to spread it along the axis again.");
    }
    group.fixture_layout.retain(|f| fixtures.contains(&f.object_id));
    changed
}
