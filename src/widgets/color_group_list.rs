use egui::{self, Color32, Ui};

use crate::objects::color_palette::{ColorGroup, ColorStack, STACK_SIZE};

pub struct ColorGroupManager {
    pub groups: Vec<ColorGroup>,
    next_id: u32,
}

impl ColorGroupManager {
    pub fn new() -> Self {
        Self { groups: Vec::new(), next_id: 1 }
    }

    pub fn from_groups(groups: Vec<ColorGroup>) -> Self {
        let next_id = groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
        Self { groups, next_id }
    }

    pub fn show(&mut self, ui: &mut Ui, stacks: &[ColorStack]) {
        ui.heading("Color Groups");
        ui.separator();

        if self.groups.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No color groups.");
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

                        // Member stacks.
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Stacks").strong());

                        let mut remove_stack_id = None;
                        for sid in &group.stack_ids {
                            if let Some(stack) = stacks.iter().find(|s| s.id == *sid) {
                                ui.horizontal(|ui| {
                                    // Mini swatches.
                                    for i in 0..STACK_SIZE {
                                        let c = stack.colors[i];
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
                                    ui.label(&stack.name);
                                    if ui.small_button("x").clicked() {
                                        remove_stack_id = Some(*sid);
                                    }
                                });
                            }
                        }
                        if let Some(sid) = remove_stack_id {
                            group.stack_ids.retain(|id| *id != sid);
                        }

                        // Add stack.
                        let available: Vec<&ColorStack> = stacks.iter()
                            .filter(|s| !group.stack_ids.contains(&s.id))
                            .collect();
                        if !available.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Add:");
                                for stack in &available {
                                    if ui.small_button(&stack.name).clicked() {
                                        group.stack_ids.push(stack.id);
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
        if ui.button("Add Color Group").clicked() {
            let id = self.next_id;
            self.next_id += 1;
            self.groups.push(ColorGroup::new(id, format!("Color Group {}", id)));
        }
    }
}
