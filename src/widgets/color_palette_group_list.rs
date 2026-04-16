use egui::{self, Color32, Ui};

use crate::objects::color_palette::{ColorPalette, ColorPaletteGroup, PALETTE_SIZE};

pub struct ColorPaletteGroupManager {
    pub groups: Vec<ColorPaletteGroup>,
    next_id: u32,
}

impl ColorPaletteGroupManager {
    pub fn new() -> Self {
        Self { groups: Vec::new(), next_id: 1 }
    }

    pub fn from_groups(groups: Vec<ColorPaletteGroup>) -> Self {
        let next_id = groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
        Self { groups, next_id }
    }

    pub fn show(&mut self, ui: &mut Ui, palettes: &[ColorPalette]) {
        ui.heading("Color Palette Groups");
        ui.separator();

        if self.groups.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No color palette groups.");
        }

        let mut remove_id = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for group in &mut self.groups {
                ui.push_id(group.id, |ui| {
                    egui::CollapsingHeader::new(
                        egui::RichText::new(&group.name).strong(),
                    )
                    .id_salt(group.id)
                    .default_open(false)
                    .show(ui, |ui| {
                        // Name
                        ui.horizontal(|ui| {
                            ui.label("Name:");
                            ui.text_edit_singleline(&mut group.name);
                        });

                        // Member palettes.
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Palettes").strong());

                        let mut remove_palette_id = None;
                        for pid in &group.palette_ids {
                            if let Some(palette) = palettes.iter().find(|s| s.id == *pid) {
                                ui.horizontal(|ui| {
                                    // Mini swatches.
                                    for i in 0..PALETTE_SIZE {
                                        let c = palette.colors[i];
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
                                    if ui.small_button("x").clicked() {
                                        remove_palette_id = Some(*pid);
                                    }
                                });
                            }
                        }
                        if let Some(pid) = remove_palette_id {
                            group.palette_ids.retain(|id| *id != pid);
                        }

                        // Add palette.
                        let available: Vec<&ColorPalette> = palettes.iter()
                            .filter(|s| !group.palette_ids.contains(&s.id))
                            .collect();
                        if !available.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Add:");
                                for palette in &available {
                                    if ui.small_button(&palette.name).clicked() {
                                        group.palette_ids.push(palette.id);
                                    }
                                }
                            });
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
        }

        ui.separator();
        if ui.button("Add Color Palette Group").clicked() {
            let id = self.next_id;
            self.next_id += 1;
            self.groups.push(ColorPaletteGroup::new(id, format!("Palette Group {}", id)));
        }
    }
}
