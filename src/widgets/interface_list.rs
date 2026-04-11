use egui::{self, Color32, Ui};

use crate::objects::output::OutputConfig;

/// Standalone interface manager — holds all DMX output configurations.
pub struct InterfaceManager {
    pub interfaces: Vec<InterfaceEntry>,
    next_id: u32,
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
        Self { interfaces, next_id }
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
        }

        let mut remove_id = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for entry in &mut self.interfaces {
                ui.push_id(entry.id, |ui| {
                    egui::CollapsingHeader::new(
                        egui::RichText::new(&entry.name).strong(),
                    )
                    .id_salt(entry.id)
                    .default_open(true)
                    .show(ui, |ui| {
                        // Name
                        ui.horizontal(|ui| {
                            ui.label("Name:");
                            ui.text_edit_singleline(&mut entry.name);
                        });

                        // Enabled
                        ui.checkbox(&mut entry.enabled, "Enabled");

                        // Config
                        match &mut entry.config {
                            OutputConfig::ArtNet { host, port } => {
                                ui.label("Art-Net");
                                ui.horizontal(|ui| {
                                    ui.label("Host:");
                                    ui.text_edit_singleline(host);
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Port:");
                                    let mut p = *port as i32;
                                    if ui.add(egui::DragValue::new(&mut p).range(1..=65535)).changed() {
                                        *port = p as u16;
                                    }
                                });
                            }
                            OutputConfig::Sacn { source_name } => {
                                ui.label("sACN (E1.31)");
                                ui.horizontal(|ui| {
                                    ui.label("Source:");
                                    ui.text_edit_singleline(source_name);
                                });
                            }
                            OutputConfig::None => {
                                ui.colored_label(Color32::from_gray(120), "Preview only");
                            }
                        }

                        ui.add_space(4.0);
                        if ui.small_button("Delete").clicked() {
                            remove_id = Some(entry.id);
                        }
                    });
                });
            }
        });

        if let Some(id) = remove_id {
            self.interfaces.retain(|e| e.id != id);
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
            }
        });
    }
}
