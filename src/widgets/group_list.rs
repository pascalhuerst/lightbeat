use egui::{self, Color32, Ui};

use crate::objects::group::Group;
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
                        ui.label(egui::RichText::new("Members").strong());

                        let mut remove_obj_id = None;
                        for oid in &group.object_ids {
                            if let Some(obj) = objects.iter().find(|o| o.id == *oid) {
                                ui.horizontal(|ui| {
                                    ui.label(format!("  {}", obj.name));
                                    if ui.small_button("x").clicked() {
                                        remove_obj_id = Some(*oid);
                                    }
                                });
                            }
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
