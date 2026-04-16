use egui::{self, Color32, Ui};

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
        }

        let mut remove_id = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for palette in &mut self.palettes {
                ui.push_id(palette.id, |ui| {
                    // Show color swatches inline in the header.
                    egui::CollapsingHeader::new(
                        egui::RichText::new(&palette.name).strong(),
                    )
                    .id_salt(palette.id)
                    .default_open(false)
                    .show(ui, |ui| {
                        // Name
                        ui.horizontal(|ui| {
                            ui.label("Name:");
                            ui.text_edit_singleline(&mut palette.name);
                        });

                        // Color swatches with pickers.
                        for i in 0..PALETTE_SIZE {
                            ui.horizontal(|ui| {
                                ui.label(SLOT_NAMES[i]);
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

                        ui.add_space(4.0);
                        if ui.small_button("Delete palette").clicked() {
                            remove_id = Some(palette.id);
                        }
                    });

                    // Show mini swatches next to collapsed header.
                    ui.horizontal(|ui| {
                        for i in 0..PALETTE_SIZE {
                            let c = palette.colors[i];
                            let color = egui::Color32::from_rgb(
                                (c.r.clamp(0.0, 1.0) * 255.0) as u8,
                                (c.g.clamp(0.0, 1.0) * 255.0) as u8,
                                (c.b.clamp(0.0, 1.0) * 255.0) as u8,
                            );
                            let (r, p) = ui.allocate_painter(
                                egui::Vec2::new(14.0, 14.0),
                                egui::Sense::hover(),
                            );
                            p.rect_filled(r.rect, 2.0, color);
                        }
                    });
                });
            }
        });

        if let Some(id) = remove_id {
            self.palettes.retain(|s| s.id != id);
        }

        ui.separator();
        if ui.button("Add Color Palette").clicked() {
            let id = self.next_id;
            self.next_id += 1;
            self.palettes.push(ColorPalette::new(id, format!("Palette {}", id)));
        }
    }
}
