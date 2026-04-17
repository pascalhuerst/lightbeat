use egui::{self, Color32, Ui};
use egui_extras::{Column, TableBuilder};

use crate::color::Rgb;
use crate::objects::color_palette::{ColorPalette, SLOT_NAMES, PALETTE_SIZE};

pub struct ColorPaletteManager {
    pub palettes: Vec<ColorPalette>,
    next_id: u32,
}

impl ColorPaletteManager {
    pub fn new() -> Self {
        Self { palettes: Vec::new(), next_id: 1 }
    }

    pub fn from_palettes(palettes: Vec<ColorPalette>) -> Self {
        let next_id = palettes.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        Self { palettes, next_id }
    }

    pub fn show(&mut self, ui: &mut Ui) {
        ui.heading("Color Palettes");
        ui.separator();

        if self.palettes.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No color palettes.");
        } else {
            let mut remove_id = None;
            let swatch_w = 28.0;

            let mut builder = TableBuilder::new(ui)
                .striped(true)
                .column(Column::remainder().clip(true)); // name fills
            for _ in 0..PALETTE_SIZE {
                builder = builder.column(Column::exact(swatch_w));
            }
            builder
                .column(Column::exact(24.0)) // delete button
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
                                ui.add_sized(
                                    [ui.available_width(), 20.0],
                                    egui::TextEdit::singleline(&mut palette.name)
                                        .id_salt(("name", palette.id)),
                                );
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
                                    }
                                });
                            }
                            row.col(|ui| {
                                if ui.small_button(egui_phosphor::regular::X).clicked() {
                                    remove_id = Some(palette.id);
                                }
                            });
                        });
                    }
                });
            if let Some(id) = remove_id {
                self.palettes.retain(|s| s.id != id);
            }
        }

        ui.separator();
        if ui.button("Add Color Palette").clicked() {
            let id = self.next_id;
            self.next_id += 1;
            self.palettes.push(ColorPalette::new(id, format!("Palette {}", id)));
        }
    }
}
