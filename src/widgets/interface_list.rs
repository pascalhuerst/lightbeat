use egui::{self, Color32, Ui};
use egui_extras::{Column, TableBuilder};

use crate::objects::output::OutputConfig;

/// Standalone interface manager — holds all DMX output configurations.
pub struct InterfaceManager {
    pub interfaces: Vec<InterfaceEntry>,
    next_id: u32,
    /// Set to true when interfaces are added/removed/enabled/disabled.
    pub needs_sync: bool,
}

pub struct InterfaceEntry {
    pub id: u32,
    pub name: String,
    pub config: OutputConfig,
    pub enabled: bool,
}

impl InterfaceManager {
    pub fn new() -> Self {
        Self {
            interfaces: Vec::new(),
            next_id: 1,
            needs_sync: false,
        }
    }

    pub fn from_saved(saved: Vec<crate::setup::SavedInterface>) -> Self {
        let next_id = saved.iter().map(|e| e.id).max().unwrap_or(0) + 1;
        let interfaces = saved
            .into_iter()
            .map(|s| InterfaceEntry {
                id: s.id,
                name: s.name,
                config: s.config,
                enabled: s.enabled,
            })
            .collect();
        Self { interfaces, next_id, needs_sync: true }
    }

    pub fn to_saved(&self) -> Vec<crate::setup::SavedInterface> {
        self.interfaces
            .iter()
            .map(|e| crate::setup::SavedInterface {
                id: e.id,
                name: e.name.clone(),
                config: e.config.clone(),
                enabled: e.enabled,
            })
            .collect()
    }

    pub fn show(&mut self, ui: &mut Ui) {
        ui.heading("Interfaces");
        ui.separator();

        if self.interfaces.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No interfaces configured.");
        } else {
            let mut remove_id = None;
            let mut dirty = false;

            // Name and Config split the leftover space; everything else auto-sizes.
            TableBuilder::new(ui)
                .striped(true)
                .column(Column::remainder().at_least(140.0).clip(true)) // Name
                .column(Column::exact(30.0))                              // On
                .column(Column::auto())                                   // Kind
                .column(Column::remainder().at_least(180.0).clip(true)) // Config
                .column(Column::exact(24.0))                              // delete
                .header(20.0, |mut header| {
                    header.col(|ui| { ui.strong("Name"); });
                    header.col(|ui| { ui.strong("On"); });
                    header.col(|ui| { ui.strong("Kind"); });
                    header.col(|ui| { ui.strong("Config"); });
                    header.col(|_ui| {});
                })
                .body(|mut body| {
                    for entry in &mut self.interfaces {
                        body.row(22.0, |mut row| {
                            row.col(|ui| {
                                ui.add_sized(
                                    [ui.available_width(), 20.0],
                                    egui::TextEdit::singleline(&mut entry.name)
                                        .id_salt(("name", entry.id)),
                                );
                            });
                            row.col(|ui| {
                                if ui.checkbox(&mut entry.enabled, "").changed() {
                                    dirty = true;
                                }
                            });
                            row.col(|ui| {
                                ui.label(kind_label(&entry.config));
                            });
                            row.col(|ui| {
                                show_config_editor(ui, entry.id, &mut entry.config);
                            });
                            row.col(|ui| {
                                if ui.small_button(egui_phosphor::regular::X).clicked() {
                                    remove_id = Some(entry.id);
                                }
                            });
                        });
                    }
                });

            if let Some(id) = remove_id {
                self.interfaces.retain(|e| e.id != id);
                self.needs_sync = true;
            }
            if dirty {
                self.needs_sync = true;
            }
        }

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("+ Art-Net").clicked() {
                let id = self.next_id;
                self.next_id += 1;
                self.interfaces.push(InterfaceEntry {
                    id,
                    name: format!("Art-Net {}", id),
                    config: OutputConfig::ArtNet {
                        host: "255.255.255.255".to_string(),
                        port: 6454,
                    },
                    enabled: true,
                });
                self.needs_sync = true;
            }
            if ui.button("+ sACN").clicked() {
                let id = self.next_id;
                self.next_id += 1;
                self.interfaces.push(InterfaceEntry {
                    id,
                    name: format!("sACN {}", id),
                    config: OutputConfig::Sacn {
                        source_name: "LightBeat".to_string(),
                    },
                    enabled: true,
                });
                self.needs_sync = true;
            }
        });
    }
}

fn kind_label(cfg: &OutputConfig) -> &'static str {
    match cfg {
        OutputConfig::ArtNet { .. } => "Art-Net",
        OutputConfig::Sacn { .. } => "sACN",
        OutputConfig::None => "Preview",
    }
}

fn show_config_editor(ui: &mut Ui, id: u32, cfg: &mut OutputConfig) {
    match cfg {
        OutputConfig::ArtNet { host, port } => {
            ui.horizontal(|ui| {
                let mut p = *port as i32;
                let port_w = 70.0;
                let host_w = (ui.available_width() - port_w - 20.0).max(60.0);
                ui.add_sized(
                    [host_w, 20.0],
                    egui::TextEdit::singleline(host).id_salt(("host", id)),
                );
                ui.label(":");
                if ui.add(egui::DragValue::new(&mut p).range(1..=65535)).changed() {
                    *port = p as u16;
                }
            });
        }
        OutputConfig::Sacn { source_name } => {
            ui.add_sized(
                [ui.available_width(), 20.0],
                egui::TextEdit::singleline(source_name).id_salt(("src", id)),
            );
        }
        OutputConfig::None => {
            ui.colored_label(Color32::from_gray(120), "preview only");
        }
    }
}
