use egui::{self, Ui};
use egui_extras::{Column, TableBuilder};

use crate::dmx_input::{ARTNET_PORT, FoundNode, format_address};
use crate::objects::output::OutputConfig;
use crate::theme;
use crate::widgets::setup_ui;

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

    /// A copy right after the original, with a fresh id; the configuration is
    /// copied as it is.
    fn duplicate(&mut self, id: u32) {
        let Some(pos) = self.interfaces.iter().position(|e| e.id == id) else { return };
        let src = &self.interfaces[pos];
        let copy = InterfaceEntry {
            id: self.next_id,
            name: setup_ui::copy_name(&src.name),
            config: src.config.clone(),
            enabled: src.enabled,
        };
        self.next_id += 1;
        self.interfaces.insert(pos + 1, copy);
        self.needs_sync = true;
    }

    /// Whether an Art-Net interface already sends to `node`.
    fn has_node(&self, node: &FoundNode) -> bool {
        let ip = node.ip.to_string();
        self.interfaces.iter().any(|e| matches!(&e.config, OutputConfig::ArtNet { host, .. } if host.trim() == ip))
    }

    /// An Art-Net interface that sends straight to `node`, named after it.
    fn add_node(&mut self, node: &FoundNode) {
        let id = self.next_id;
        self.next_id += 1;
        let name = if node.name.is_empty() { format!("Art-Net {}", node.ip) } else { node.name.clone() };
        self.interfaces.push(InterfaceEntry {
            id,
            name,
            config: OutputConfig::ArtNet { host: node.ip.to_string(), port: ARTNET_PORT },
            enabled: true,
        });
        self.needs_sync = true;
    }

    /// The Art-Net nodes that answered the search, each with a button that
    /// makes an interface of it.
    fn show_found(&mut self, ui: &mut Ui, found: &[FoundNode], error: Option<&str>) {
        ui.add_space(8.0);
        setup_ui::section(ui, "Found on the network");
        if let Some(error) = error {
            ui.colored_label(theme::SEM_WARNING, format!("Cannot look for Art-Net nodes: {error}"));
            return;
        }
        if found.is_empty() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.colored_label(theme::TEXT_DIM, "Looking for Art-Net nodes...");
            });
            return;
        }
        let mut add = None;
        TableBuilder::new(ui)
            .id_salt("found_nodes")
            .striped(true)
            .column(Column::remainder().at_least(140.0).clip(true)) // Name
            .column(Column::auto().at_least(110.0))                  // Address
            .column(Column::remainder().at_least(120.0).clip(true)) // Universes
            .column(Column::exact(120.0))                             // add
            .header(20.0, |mut header| {
                header.col(|ui| { ui.strong("Name"); });
                header.col(|ui| { ui.strong("Address"); });
                header.col(|ui| { ui.strong("Universes"); });
                header.col(|_ui| {});
            })
            .body(|mut body| {
                for (index, node) in found.iter().enumerate() {
                    body.row(22.0, |mut row| {
                        row.col(|ui| {
                            let name = if node.name.is_empty() { "(no name)" } else { &node.name };
                            let label = ui.label(name);
                            if !node.long_name.is_empty() {
                                label.on_hover_text(&node.long_name);
                            }
                        });
                        row.col(|ui| { ui.label(node.ip.to_string()); });
                        row.col(|ui| {
                            let list: Vec<String> = node.universes.iter().map(|u| format_address(*u)).collect();
                            ui.label(list.join(", ")).on_hover_text("The universes this node outputs as DMX (net.sub-net.universe)");
                        });
                        row.col(|ui| {
                            if self.has_node(node) {
                                ui.colored_label(theme::TEXT_DIM, "in the list");
                            } else if setup_ui::add_button(ui, "Add interface") {
                                add = Some(index);
                            }
                        });
                    });
                }
            });
        if let Some(index) = add {
            self.add_node(&found[index]);
        }
    }

    /// `object_interface_ids`: the interface id of every object, to say what uses an interface.
    /// `found` and `discovery_error`: what the search for Art-Net nodes came up with.
    pub fn show(&mut self, ui: &mut Ui, object_interface_ids: &[u32], found: &[FoundNode], discovery_error: Option<&str>) {
        if self.interfaces.is_empty() {
            setup_ui::empty_state(ui, "No interfaces yet. Add one below.");
        } else {
            let mut remove_id = None;
            let mut duplicate_id = None;
            let mut dirty = false;

            // Name and Config split the leftover space; everything else auto-sizes.
            TableBuilder::new(ui)
                .striped(true)
                .column(Column::remainder().at_least(140.0).clip(true)) // Name
                .column(Column::exact(60.0))                              // Enabled
                .column(Column::auto())                                   // Kind
                .column(Column::remainder().at_least(180.0).clip(true)) // Config
                .column(Column::exact(52.0))                              // duplicate and delete
                .header(20.0, |mut header| {
                    header.col(|ui| { ui.strong("Name"); });
                    header.col(|ui| { ui.strong("Enabled"); });
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
                                ui.horizontal(|ui| {
                                    if setup_ui::duplicate_button(ui, "Duplicate interface") {
                                        duplicate_id = Some(entry.id);
                                    }
                                    let hover = setup_ui::delete_hover("Delete interface", setup_ui::use_count(entry.id, object_interface_ids), "object");
                                    if setup_ui::delete_button(ui, &hover) {
                                        remove_id = Some(entry.id);
                                    }
                                });
                            });
                        });
                    }
                });

            if let Some(id) = remove_id {
                self.interfaces.retain(|e| e.id != id);
                self.needs_sync = true;
            }
            if let Some(id) = duplicate_id {
                self.duplicate(id);
            }
            if dirty {
                self.needs_sync = true;
            }
        }

        ui.separator();
        ui.horizontal(|ui| {
            if setup_ui::add_button(ui, "Add Art-Net interface") {
                let id = self.next_id;
                self.next_id += 1;
                self.interfaces.push(InterfaceEntry {
                    id,
                    name: format!("Art-Net {}", id),
                    config: OutputConfig::ArtNet {
                        host: "255.255.255.255".to_string(),
                        port: ARTNET_PORT,
                    },
                    enabled: true,
                });
                self.needs_sync = true;
            }
            if setup_ui::add_button(ui, "Add sACN interface") {
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

        self.show_found(ui, found, discovery_error);
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
            ui.colored_label(theme::TEXT_DIM, "preview only");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_copies_the_configuration() {
        let mut m = InterfaceManager::new();
        m.interfaces.push(InterfaceEntry {
            id: 2,
            name: "Desk".into(),
            config: OutputConfig::ArtNet { host: "10.0.0.5".into(), port: 6454 },
            enabled: false,
        });
        m.next_id = 3;
        m.duplicate(2);
        assert_eq!(m.interfaces[1].id, 3);
        assert_eq!(m.interfaces[1].name, "Desk copy");
        assert!(!m.interfaces[1].enabled);
        assert!(matches!(&m.interfaces[1].config, OutputConfig::ArtNet { host, .. } if host == "10.0.0.5"));
        assert!(m.needs_sync);
    }

    #[test]
    fn a_found_node_becomes_an_interface_once() {
        let node = FoundNode {
            ip: std::net::Ipv4Addr::new(10, 0, 0, 7),
            name: "Shelf left".into(),
            long_name: String::new(),
            universes: vec![0, 1],
        };
        let mut m = InterfaceManager::new();
        assert!(!m.has_node(&node));
        m.add_node(&node);
        assert!(m.has_node(&node) && m.needs_sync);
        assert_eq!(m.interfaces[0].name, "Shelf left");
        assert!(matches!(&m.interfaces[0].config, OutputConfig::ArtNet { host, port } if host == "10.0.0.7" && *port == 6454));

        let unnamed = FoundNode { ip: std::net::Ipv4Addr::new(10, 0, 0, 8), name: String::new(), ..node };
        m.add_node(&unnamed);
        assert_eq!(m.interfaces[1].name, "Art-Net 10.0.0.8");
    }
}
