use egui::{self, Ui};
use egui_extras::{Column, TableBuilder};

use crate::color::Rgb;
use crate::widgets::setup_ui;
use crate::objects::color_palette::{ColorPalette, SLOT_NAMES, PALETTE_SIZE};

pub struct ColorPaletteManager {
    pub palettes: Vec<ColorPalette>,
    next_id: u32,
    /// Set on any edit so the app can hand the new palettes to Palette Select nodes.
    pub needs_refresh: bool,
}

impl ColorPaletteManager {
    pub fn new() -> Self {
        Self { palettes: Vec::new(), next_id: 1, needs_refresh: false }
    }

    pub fn from_palettes(palettes: Vec<ColorPalette>) -> Self {
        let next_id = palettes.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        Self { palettes, next_id, needs_refresh: false }
    }

    pub fn show(&mut self, ui: &mut Ui) {
        if self.palettes.is_empty() {
            setup_ui::empty_state(ui, "No palettes yet. Add one below.");
        } else {
            let mut remove_id = None;
            let mut duplicate_id = None;
            let swatch_w = 28.0;

            let mut builder = TableBuilder::new(ui)
                .striped(true)
                .column(Column::remainder().clip(true)); // name fills
            for _ in 0..PALETTE_SIZE {
                builder = builder.column(Column::exact(swatch_w));
            }
            builder
                .column(Column::exact(52.0)) // duplicate and delete
                .header(20.0, |mut header| {
                    header.col(|ui| { ui.strong("Name"); });
                    for i in 0..PALETTE_SIZE {
                        header.col(|ui| { ui.strong(SLOT_NAMES[i]); });
                    }
                    header.col(|_ui| {});
                })
                .body(|mut body| {
                    for palette in &mut self.palettes {
                        body.row(22.0, |mut row| {
                            row.col(|ui| {
                                let r = ui.add_sized(
                                    [ui.available_width(), 20.0],
                                    egui::TextEdit::singleline(&mut palette.name)
                                        .id_salt(("name", palette.id)),
                                );
                                if r.changed() { self.needs_refresh = true; }
                            });
                            for i in 0..PALETTE_SIZE {
                                row.col(|ui| {
                                    let mut color = [
                                        palette.colors[i].r,
                                        palette.colors[i].g,
                                        palette.colors[i].b,
                                    ];
                                    if ui.color_edit_button_rgb(&mut color).changed() {
                                        palette.colors[i] = Rgb::new(color[0], color[1], color[2]);
                                        self.needs_refresh = true;
                                    }
                                });
                            }
                            row.col(|ui| {
                                ui.horizontal(|ui| {
                                    if setup_ui::duplicate_button(ui, "Duplicate palette") {
                                        duplicate_id = Some(palette.id);
                                    }
                                    if setup_ui::delete_button(ui, "Delete palette") {
                                        remove_id = Some(palette.id);
                                    }
                                });
                            });
                        });
                    }
                });
            if let Some(id) = remove_id {
                self.palettes.retain(|s| s.id != id);
                self.needs_refresh = true;
            }
            if let Some(id) = duplicate_id {
                self.duplicate(id);
            }
        }

        ui.separator();
        if setup_ui::add_button(ui, "Add palette") {
            let id = self.next_id;
            self.next_id += 1;
            self.palettes.push(ColorPalette::new(id, format!("Palette {}", id)));
            self.needs_refresh = true;
        }
    }

    /// A copy right after the original, with a fresh id.
    fn duplicate(&mut self, id: u32) {
        let Some(pos) = self.palettes.iter().position(|p| p.id == id) else { return };
        let mut copy = self.palettes[pos].clone();
        copy.id = self.next_id;
        self.next_id += 1;
        copy.name = setup_ui::copy_name(&copy.name);
        self.palettes.insert(pos + 1, copy);
        self.needs_refresh = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_copies_colors_with_a_fresh_id() {
        let mut p = ColorPalette::new(4, "Sunset");
        p.colors[1] = Rgb::new(1.0, 0.5, 0.0);
        let mut m = ColorPaletteManager::from_palettes(vec![p]);
        m.duplicate(4);
        assert_eq!(m.palettes[1].id, 5);
        assert_eq!(m.palettes[1].name, "Sunset copy");
        assert_eq!(m.palettes[1].colors, m.palettes[0].colors);
        assert!(m.needs_refresh);
    }
}
