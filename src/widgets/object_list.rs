use egui::{self, Ui};
use egui_extras::{Column, TableBuilder};

use crate::objects::fixture::{DmxAddress, Fixture};
use crate::objects::object::Object;
use crate::theme;
use crate::widgets::setup_ui;

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

    /// A copy right after the original, with a fresh id. It is patched right
    /// behind the original when that fits in the universe, so the two do not
    /// overlap.
    fn duplicate(&mut self, id: u32) {
        let Some(pos) = self.objects.iter().position(|o| o.id == id) else { return };
        let mut copy = self.objects[pos].clone();
        copy.id = self.next_id;
        self.next_id += 1;
        copy.name = setup_ui::copy_name(&copy.name);
        let next_start = u32::from(copy.address.start_channel) + copy.dmx_footprint() as u32;
        if next_start + copy.dmx_footprint() as u32 <= 513 {
            copy.address.start_channel = next_start as u16;
        }
        self.objects.insert(pos + 1, copy);
        self.needs_sync = true;
    }

    pub fn show(&mut self, ui: &mut Ui, fixtures: &[Fixture], interface_names: &[(u32, String)]) {
        if self.objects.is_empty() {
            setup_ui::empty_state(ui, "No objects yet. Add some from a template below.");
        } else {
            let mut remove_id = None;
            let mut duplicate_id = None;
            let mut dirty = false;

            TableBuilder::new(ui)
                .striped(true)
                .column(Column::remainder().at_least(120.0).clip(true)) // Name
                .column(Column::remainder().at_least(120.0).clip(true)) // Template
                .column(Column::exact(60.0))   // Ch
                .column(Column::exact(150.0))  // Address
                .column(Column::initial(140.0).at_least(100.0).clip(true)) // Interface
                .column(Column::exact(50.0))   // Foot
                .column(Column::exact(52.0))   // duplicate and delete
                .header(20.0, |mut header| {
                    header.col(|ui| { ui.strong("Name"); });
                    header.col(|ui| { ui.strong("Template"); });
                    header.col(|ui| { ui.strong("Ch").on_hover_text("Start channel"); });
                    header.col(|ui| { ui.strong("Address").on_hover_text("Net.subnet.universe"); });
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
                                    .unwrap_or(setup_ui::MISSING);
                                ui.colored_label(theme::TEXT_MUTED, fixture_name);
                            });
                            row.col(|ui| {
                                let mut addr = obj.address.start_channel as i32;
                                if ui.add(egui::DragValue::new(&mut addr).range(1..=512)).changed() {
                                    obj.address.start_channel = addr as u16;
                                    dirty = true;
                                }
                            });
                            row.col(|ui| {
                                let a = &mut obj.address;
                                if setup_ui::dmx_address(ui, &mut a.net, &mut a.subnet, &mut a.universe) {
                                    dirty = true;
                                }
                            });
                            row.col(|ui| {
                                let current = if obj.interface_id == 0 {
                                    "(none)"
                                } else {
                                    interface_names.iter()
                                        .find(|(id, _)| *id == obj.interface_id)
                                        .map(|(_, name)| name.as_str())
                                        .unwrap_or(setup_ui::MISSING)
                                };
                                egui::ComboBox::from_id_salt(("iface", obj.id))
                                    .width(ui.available_width())
                                    .selected_text(current)
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_value(&mut obj.interface_id, 0, "(none)").changed() {
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
                                    theme::TEXT_DIM,
                                    format!("{}", obj.dmx_footprint()),
                                );
                            });
                            row.col(|ui| {
                                ui.horizontal(|ui| {
                                    if setup_ui::duplicate_button(ui, "Duplicate object") {
                                        duplicate_id = Some(obj.id);
                                    }
                                    if setup_ui::delete_button(ui, "Delete object") {
                                        remove_id = Some(obj.id);
                                    }
                                });
                            });
                        });
                    }
                });

            if let Some(id) = remove_id {
                self.objects.retain(|o| o.id != id);
                self.needs_sync = true;
            }
            if let Some(id) = duplicate_id {
                self.duplicate(id);
            }
            if dirty {
                self.needs_sync = true;
            }
        }

        // Batch creation.
        ui.separator();
        if fixtures.is_empty() {
            setup_ui::empty_state(ui, "Add a fixture template first.");
        } else {
            setup_ui::section(ui, "Add objects");

            // Fixture type selector.
            if self.batch_fixture_idx >= fixtures.len() {
                self.batch_fixture_idx = 0;
            }
            ui.horizontal(|ui| {
                ui.label("Template");
                egui::ComboBox::from_id_salt("batch_fixture")
                    .selected_text(&fixtures[self.batch_fixture_idx].name)
                    .show_ui(ui, |ui| {
                        for (i, f) in fixtures.iter().enumerate() {
                            ui.selectable_value(&mut self.batch_fixture_idx, i, &f.name);
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label("Count");
                ui.add(egui::DragValue::new(&mut self.batch_count).range(1..=100));
                ui.label("Start ch");
                ui.add(egui::DragValue::new(&mut self.batch_start_ch).range(1..=512));
                ui.label("Gap");
                ui.add(egui::DragValue::new(&mut self.batch_gap).range(0..=32));
            });

            // Preview.
            let fixture = &fixtures[self.batch_fixture_idx];
            let footprint = fixture.dmx_footprint() as i32;
            let stride = footprint + self.batch_gap;
            let last_ch = self.batch_start_ch + (self.batch_count - 1) * stride + footprint - 1;
            ui.colored_label(
                theme::TEXT_DIM,
                format!(
                    "{} × {} ({} ch each, stride {}), ch {}-{}",
                    self.batch_count, fixture.name, footprint, stride,
                    self.batch_start_ch, last_ch,
                ),
            );

            if setup_ui::add_button(ui, "Add objects") {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::channel::{Channel, ColorMode};

    fn template() -> Fixture {
        let mut f = Fixture::new(1, "Par");
        f.add_channel(Channel::color("Color", ColorMode::Rgb));
        f
    }

    #[test]
    fn duplicate_is_patched_behind_the_original() {
        let f = template();
        let obj = Object::new(4, "Par #4", &f, DmxAddress { start_channel: 10, ..Default::default() });
        let mut m = ObjectManager::from_objects(vec![obj]);
        m.duplicate(4);
        assert_eq!(m.objects[1].id, 5);
        assert_eq!(m.objects[1].name, "Par #4 copy");
        assert_eq!(m.objects[1].address.start_channel, 13);
        assert!(m.needs_sync);
    }

    #[test]
    fn duplicate_stays_put_when_the_universe_is_full() {
        let f = template();
        let obj = Object::new(1, "Par", &f, DmxAddress { start_channel: 511, ..Default::default() });
        let mut m = ObjectManager::from_objects(vec![obj]);
        m.duplicate(1);
        assert_eq!(m.objects[1].address.start_channel, 511);
    }
}
