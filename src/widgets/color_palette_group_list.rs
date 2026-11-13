use egui::{self, Ui};

use crate::objects::color_palette::{ColorPalette, ColorPaletteGroup, PALETTE_SIZE};
use crate::widgets::setup_ui::{self, ItemHeader, Move};

pub struct ColorPaletteGroupManager {
    pub groups: Vec<ColorPaletteGroup>,
    next_id: u32,
    /// Set on any edit so the app can hand the new groups to Palette Select nodes.
    pub needs_refresh: bool,
}

impl ColorPaletteGroupManager {
    pub fn new() -> Self {
        Self { groups: Vec::new(), next_id: 1, needs_refresh: false }
    }

    pub fn from_groups(groups: Vec<ColorPaletteGroup>) -> Self {
        let next_id = groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
        Self { groups, next_id, needs_refresh: false }
    }

    pub fn show(&mut self, ui: &mut Ui, palettes: &[ColorPalette]) {
        if self.groups.is_empty() {
            setup_ui::empty_state(ui, "No palette groups yet. Add one below.");
        }

        let open = setup_ui::open_by_default(self.groups.len());
        let mut remove_id = None;
        let mut duplicate_id = None;

        for group in &mut self.groups {
            let title = group.name.clone();
            let mut header = ItemHeader::new(&title, "Delete palette group");
            header.duplicate = Some("Duplicate palette group");
            let needs_refresh = &mut self.needs_refresh;
            let actions = setup_ui::item_header(ui, group.id, open, header, |ui| {
                if setup_ui::name_field(ui, &mut group.name) {
                    *needs_refresh = true;
                }

                ui.add_space(4.0);
                setup_ui::section(ui, "Palettes (order = index for Palette Select)");

                let mut remove_palette_id = None;
                let mut mv: Option<(usize, Move)> = None;
                let n = group.palette_ids.len();
                for (i, pid) in group.palette_ids.iter().enumerate() {
                    let palette = palettes.iter().find(|s| s.id == *pid);
                    ui.horizontal(|ui| {
                        if let Some(m) = setup_ui::move_buttons(ui, i, n) {
                            mv = Some((i, m));
                        }
                        if let Some(palette) = palette {
                            for &c in palette.colors.iter().take(PALETTE_SIZE) {
                                let color = egui::Color32::from_rgb(
                                    (c.r.clamp(0.0, 1.0) * 255.0) as u8,
                                    (c.g.clamp(0.0, 1.0) * 255.0) as u8,
                                    (c.b.clamp(0.0, 1.0) * 255.0) as u8,
                                );
                                let (r, p) = ui.allocate_painter(
                                    egui::Vec2::new(10.0, 10.0),
                                    egui::Sense::hover(),
                                );
                                p.rect_filled(r.rect, 1.0, color);
                            }
                            ui.label(&palette.name);
                        } else {
                            ui.label(setup_ui::MISSING);
                        }
                        if setup_ui::delete_button(ui, "Remove from group") {
                            remove_palette_id = Some(*pid);
                        }
                    });
                }
                if let Some((i, m)) = mv
                    && setup_ui::apply_move(&mut group.palette_ids, i, m)
                {
                    *needs_refresh = true;
                }
                if let Some(pid) = remove_palette_id {
                    group.palette_ids.retain(|id| *id != pid);
                    *needs_refresh = true;
                }

                let available: Vec<(u32, String)> = palettes.iter()
                    .filter(|s| !group.palette_ids.contains(&s.id))
                    .map(|s| (s.id, s.name.clone()))
                    .collect();
                if !available.is_empty()
                    && let Some(id) = setup_ui::add_menu(ui, ("add_palette", group.id), "Add palette...", available)
                {
                    group.palette_ids.push(id);
                    *needs_refresh = true;
                }
                ui.add_space(4.0);
            });
            if actions.delete {
                remove_id = Some(group.id);
            }
            if actions.duplicate {
                duplicate_id = Some(group.id);
            }
        }

        if let Some(id) = remove_id {
            self.groups.retain(|g| g.id != id);
            self.needs_refresh = true;
        }
        if let Some(id) = duplicate_id {
            self.duplicate(id);
        }

        ui.separator();
        if setup_ui::add_button(ui, "Add palette group") {
            let id = self.next_id;
            self.next_id += 1;
            self.groups.push(ColorPaletteGroup::new(id, format!("Palette group {}", id)));
            self.needs_refresh = true;
        }
    }

    /// A copy right after the original, with a fresh id.
    fn duplicate(&mut self, id: u32) {
        let Some(pos) = self.groups.iter().position(|g| g.id == id) else { return };
        let mut copy = self.groups[pos].clone();
        copy.id = self.next_id;
        self.next_id += 1;
        copy.name = setup_ui::copy_name(&copy.name);
        self.groups.insert(pos + 1, copy);
        self.needs_refresh = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_keeps_palette_order() {
        let mut g = ColorPaletteGroup::new(2, "Warm");
        g.palette_ids = vec![5, 3, 9];
        let mut m = ColorPaletteGroupManager::from_groups(vec![g]);
        m.duplicate(2);
        assert_eq!(m.groups[1].id, 3);
        assert_eq!(m.groups[1].name, "Warm copy");
        assert_eq!(m.groups[1].palette_ids, vec![5, 3, 9]);
        assert!(m.needs_refresh);
    }

    #[test]
    fn reordering_changes_the_index_of_a_palette() {
        let mut ids = vec![5, 3, 9];
        assert!(setup_ui::apply_move(&mut ids, 2, Move::Up));
        assert_eq!(ids, vec![5, 9, 3]);
    }
}
