use egui::{self, Color32, Ui};
use egui_extras::{Column, TableBuilder};

use crate::objects::fixture::{DmxAddress, Fixture};
use crate::objects::object::Object;

/// Manages object instances.
pub struct ObjectManager {
    pub objects: Vec<Object>,
    pub needs_sync: bool,
    next_id: u32,
    // Batch creation state.
    batch_count: i32,
    batch_start_ch: i32,
    batch_gap: i32,
    batch_fixture_idx: usize,
}

impl ObjectManager {
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            needs_sync: false,
            next_id: 1,
            batch_count: 1,
            batch_start_ch: 1,
            batch_gap: 0,
            batch_fixture_idx: 0,
        }
    }

    pub fn from_objects(objects: Vec<Object>) -> Self {
        let next_id = objects.iter().map(|o| o.id).max().unwrap_or(0) + 1;
        Self {
            objects,
            needs_sync: false,
            next_id,
            batch_count: 1,
            batch_start_ch: 1,
            batch_gap: 0,
            batch_fixture_idx: 0,
        }
    }

    pub fn show(&mut self, ui: &mut Ui, fixtures: &[Fixture], interface_names: &[(u32, String)]) {
        ui.heading("Objects");
        ui.separator();

        if self.objects.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No objects. Create one from a fixture template.");
        } else {
            let mut remove_id = None;
            let mut dirty = false;

            TableBuilder::new(ui)
                .striped(true)
                .column(Column::remainder().at_least(120.0).clip(true)) // Name
                .column(Column::remainder().at_least(120.0).clip(true)) // Template
                .column(Column::exact(60.0))   // Ch
                .column(Column::exact(50.0))   // Univ
                .column(Column::exact(50.0))   // Sub
                .column(Column::exact(50.0))   // Net
                .column(Column::initial(140.0).at_least(100.0).clip(true)) // Interface
                .column(Column::exact(50.0))   // Foot
                .column(Column::exact(24.0))   // delete
                .header(20.0, |mut header| {
                    header.col(|ui| { ui.strong("Name"); });
                    header.col(|ui| { ui.strong("Template"); });
                    header.col(|ui| { ui.strong("Ch").on_hover_text("Start channel"); });
                    header.col(|ui| { ui.strong("Univ"); });
                    header.col(|ui| { ui.strong("Sub"); });
                    header.col(|ui| { ui.strong("Net"); });
                    header.col(|ui| { ui.strong("Interface"); });
                    header.col(|ui| { ui.strong("Foot").on_hover_text("DMX footprint in channels"); });
                    header.col(|_ui| {});
                })
                .body(|mut body| {
                    for obj in &mut self.objects {
                        body.row(22.0, |mut row| {
                            row.col(|ui| {
                                ui.add_sized(
                                    [ui.available_width(), 20.0],
                                    egui::TextEdit::singleline(&mut obj.name)
                                        .id_salt(("name", obj.id)),
                                );
                            });
                            row.col(|ui| {
                                let fixture_name = fixtures.iter()
                                    .find(|f| f.id == obj.fixture_id)
                                    .map(|f| f.name.as_str())
                                    .unwrap_or("???");
                                ui.colored_label(Color32::from_gray(180), fixture_name);
                            });
                            row.col(|ui| {
                                let mut addr = obj.address.start_channel as i32;
                                if ui.add(egui::DragValue::new(&mut addr).range(1..=512)).changed() {
                                    obj.address.start_channel = addr as u16;
                                    dirty = true;
                                }
                            });
                            row.col(|ui| {
                                let mut u = obj.address.universe as i32;
                                if ui.add(egui::DragValue::new(&mut u).range(0..=15)).changed() {
                                    obj.address.universe = u as u8;
                                    dirty = true;
                                }
                            });
                            row.col(|ui| {
                                let mut s = obj.address.subnet as i32;
                                if ui.add(egui::DragValue::new(&mut s).range(0..=15)).changed() {
                                    obj.address.subnet = s as u8;
                                    dirty = true;
                                }
                            });
                            row.col(|ui| {
                                let mut n = obj.address.net as i32;
                                if ui.add(egui::DragValue::new(&mut n).range(0..=127)).changed() {
                                    obj.address.net = n as u8;
                                    dirty = true;
                                }
                            });
                            row.col(|ui| {
                                let current = interface_names.iter()
                                    .find(|(id, _)| *id == obj.interface_id)
                                    .map(|(_, name)| name.as_str())
                                    .unwrap_or("None");
                                egui::ComboBox::from_id_salt(("iface", obj.id))
                                    .width(ui.available_width())
                                    .selected_text(current)
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_value(&mut obj.interface_id, 0, "None").changed() {
                                            dirty = true;
                                        }
                                        for (iid, iname) in interface_names {
                                            if ui.selectable_value(&mut obj.interface_id, *iid, iname).changed() {
                                                dirty = true;
                                            }
                                        }
                                    });
                            });
                            row.col(|ui| {
                                ui.colored_label(
                                    Color32::from_gray(140),
                                    format!("{}", obj.dmx_footprint()),
                                );
                            });
                            row.col(|ui| {
                                if ui.small_button(egui_phosphor::regular::X).clicked() {
                                    remove_id = Some(obj.id);
                                }
                            });
                        });
                    }
                });

            if let Some(id) = remove_id {
                self.objects.retain(|o| o.id != id);
                self.needs_sync = true;
            }
            if dirty {
                self.needs_sync = true;
            }
        }

        // Batch creation.
        ui.separator();
        if fixtures.is_empty() {
            ui.colored_label(Color32::from_gray(120), "Create fixture templates first.");
        } else {
            ui.label(egui::RichText::new("Add Objects").strong());

            // Fixture type selector.
            if self.batch_fixture_idx >= fixtures.len() {
                self.batch_fixture_idx = 0;
            }
            ui.horizontal(|ui| {
                ui.label("Template:");
                egui::ComboBox::from_id_salt("batch_fixture")
                    .selected_text(&fixtures[self.batch_fixture_idx].name)
                    .show_ui(ui, |ui| {
                        for (i, f) in fixtures.iter().enumerate() {
                            ui.selectable_value(&mut self.batch_fixture_idx, i, &f.name);
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label("Count:");
                ui.add(egui::DragValue::new(&mut self.batch_count).range(1..=100));
                ui.label("Start ch:");
                ui.add(egui::DragValue::new(&mut self.batch_start_ch).range(1..=512));
                ui.label("Gap:");
                ui.add(egui::DragValue::new(&mut self.batch_gap).range(0..=32));
            });

            // Preview.
            let fixture = &fixtures[self.batch_fixture_idx];
            let footprint = fixture.dmx_footprint() as i32;
            let stride = footprint + self.batch_gap;
            let last_ch = self.batch_start_ch + (self.batch_count - 1) * stride + footprint - 1;
            ui.colored_label(
                Color32::from_gray(120),
                format!(
                    "{} × {} ({} ch each, stride {}), ch {}-{}",
                    self.batch_count, fixture.name, footprint, stride,
                    self.batch_start_ch, last_ch,
                ),
            );

            if ui.button("Create").clicked() {
                let fixture = &fixtures[self.batch_fixture_idx];
                for i in 0..self.batch_count {
                    let id = self.next_id;
                    self.next_id += 1;
                    let ch = self.batch_start_ch + i * stride;
                    let obj = Object::new(
                        id,
                        format!("{} #{}", fixture.name, id),
                        fixture,
                        DmxAddress { start_channel: ch as u16, ..Default::default() },
                    );
                    self.objects.push(obj);
                }
                self.needs_sync = true;
                // Advance start channel for next batch.
                self.batch_start_ch += self.batch_count * stride;
            }
        }
    }
}
