use egui::{self, Color32, Ui};

use crate::color::Rgb;
use crate::objects::color_palette::{ColorStack, SLOT_NAMES, STACK_SIZE};

pub struct ColorStackManager {
    pub stacks: Vec<ColorStack>,
    next_id: u32,
}

impl ColorStackManager {
    pub fn new() -> Self {
        Self { stacks: Vec::new(), next_id: 1 }
    }

    pub fn from_stacks(stacks: Vec<ColorStack>) -> Self {
        let next_id = stacks.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        Self { stacks, next_id }
    }

    pub fn show(&mut self, ui: &mut Ui) {
        ui.heading("Color Stacks");
        ui.separator();

        if self.stacks.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No color stacks.");
        }

        let mut remove_id = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for stack in &mut self.stacks {
                ui.push_id(stack.id, |ui| {
                    // Show color swatches inline in the header.
                    egui::CollapsingHeader::new(
                        egui::RichText::new(&stack.name).strong(),
                    )
                    .id_salt(stack.id)
                    .default_open(false)
                    .show(ui, |ui| {
                        // Name
                        ui.horizontal(|ui| {
                            ui.label("Name:");
                            ui.text_edit_singleline(&mut stack.name);
                        });

                        // Color swatches with pickers.
                        for i in 0..STACK_SIZE {
                            ui.horizontal(|ui| {
                                ui.label(SLOT_NAMES[i]);
                                let mut color = [
                                    stack.colors[i].r,
                                    stack.colors[i].g,
                                    stack.colors[i].b,
                                ];
                                if ui.color_edit_button_rgb(&mut color).changed() {
                                    stack.colors[i] = Rgb::new(color[0], color[1], color[2]);
                                }
                            });
                        }

                        ui.add_space(4.0);
                        if ui.small_button("Delete stack").clicked() {
                            remove_id = Some(stack.id);
                        }
                    });

                    // Show mini swatches next to collapsed header.
                    ui.horizontal(|ui| {
                        for i in 0..STACK_SIZE {
                            let c = stack.colors[i];
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
            self.stacks.retain(|s| s.id != id);
        }

        ui.separator();
        if ui.button("Add Color Stack").clicked() {
            let id = self.next_id;
            self.next_id += 1;
            self.stacks.push(ColorStack::new(id, format!("Stack {}", id)));
        }
    }
}
