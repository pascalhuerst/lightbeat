use egui::{self, Color32, Ui};

use crate::objects::channel::{Channel, ChannelKind, ColorMode};
use crate::objects::fixture::{DmxAddress, Fixture};

/// Standalone fixture manager — holds all fixtures, shown in a dedicated window.
pub struct FixtureManager {
    pub fixtures: Vec<Fixture>,
    next_id: u32,
}

impl FixtureManager {
    pub fn new() -> Self {
        Self {
            fixtures: Vec::new(),
            next_id: 1,
        }
    }

    pub fn from_fixtures(fixtures: Vec<Fixture>) -> Self {
        let next_id = fixtures.iter().map(|f| f.id).max().unwrap_or(0) + 1;
        Self { fixtures, next_id }
    }

    pub fn add_fixture(&mut self, name: impl Into<String>, address: DmxAddress) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let mut fixture = Fixture::new(id, name, address);
        fixture.add_channel(Channel::dimmer("Dimmer"));
        fixture.add_channel(Channel::color("Color", ColorMode::Rgb));
        self.fixtures.push(fixture);
        id
    }

    pub fn remove_fixture(&mut self, id: u32) {
        self.fixtures.retain(|f| f.id != id);
    }

    /// Show the fixture list window contents.
    pub fn show(&mut self, ui: &mut Ui) {
        ui.heading("Fixtures");
        ui.separator();

        if self.fixtures.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No fixtures.");
        }

        let mut remove_id = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for fixture in &mut self.fixtures {
                ui.push_id(fixture.id, |ui| {
                    egui::CollapsingHeader::new(
                        egui::RichText::new(&fixture.name).strong(),
                    )
                    .id_salt(fixture.id)
                    .default_open(false)
                    .show(ui, |ui| {
                        // Name
                        ui.horizontal(|ui| {
                            ui.label("Name:");
                            ui.text_edit_singleline(&mut fixture.name);
                        });

                        // Address
                        ui.horizontal(|ui| {
                            ui.label("Address:");
                            let mut addr = fixture.address.start_channel as i32;
                            if ui.add(egui::DragValue::new(&mut addr).range(1..=512)).changed() {
                                fixture.address.start_channel = addr as u16;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Universe:");
                            let mut u = fixture.address.universe as i32;
                            if ui.add(egui::DragValue::new(&mut u).range(0..=15)).changed() {
                                fixture.address.universe = u as u8;
                            }
                            ui.label("Subnet:");
                            let mut s = fixture.address.subnet as i32;
                            if ui.add(egui::DragValue::new(&mut s).range(0..=15)).changed() {
                                fixture.address.subnet = s as u8;
                            }
                            ui.label("Net:");
                            let mut n = fixture.address.net as i32;
                            if ui.add(egui::DragValue::new(&mut n).range(0..=127)).changed() {
                                fixture.address.net = n as u8;
                            }
                        });

                        ui.colored_label(
                            Color32::from_gray(100),
                            format!("Footprint: {} DMX channels", fixture.dmx_footprint()),
                        );

                        // Channels
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Channels").strong());
                        for ch in &fixture.channels {
                            ui.horizontal(|ui| {
                                ui.label(format!("  {} ({})", ch.name, kind_label(&ch.kind)));
                            });
                        }

                        // Add channel buttons
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.small_button("+ Dimmer").clicked() {
                                let n = format!("Dimmer {}", fixture.channels.len() + 1);
                                fixture.add_channel(Channel::dimmer(&n));
                            }
                            if ui.small_button("+ RGB").clicked() {
                                let n = format!("Color {}", fixture.channels.len() + 1);
                                fixture.add_channel(Channel::color(&n, ColorMode::Rgb));
                            }
                            if ui.small_button("+ RGBW").clicked() {
                                let n = format!("Color {}", fixture.channels.len() + 1);
                                fixture.add_channel(Channel::color(
                                    &n,
                                    ColorMode::Rgbw { white_temperature: 6500 },
                                ));
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.small_button("+ Pan/Tilt").clicked() {
                                let n = format!("PanTilt {}", fixture.channels.len() + 1);
                                fixture.add_channel(Channel::pan_tilt(&n, false));
                            }
                            if !fixture.channels.is_empty() && ui.small_button("- Remove last").clicked() {
                                fixture.channels.pop();
                                fixture.recalc_offsets();
                            }
                        });

                        ui.add_space(4.0);
                        if ui.small_button("Delete fixture").clicked() {
                            remove_id = Some(fixture.id);
                        }
                    });
                });
            }
        });

        if let Some(id) = remove_id {
            self.remove_fixture(id);
        }

        ui.separator();
        if ui.button("Add Fixture").clicked() {
            let next_addr = self.fixtures.last()
                .map(|f| f.address.start_channel + f.dmx_footprint() as u16)
                .unwrap_or(1);
            self.add_fixture(
                format!("Fixture {}", self.fixtures.len() + 1),
                DmxAddress { start_channel: next_addr, ..Default::default() },
            );
        }
    }
}

fn kind_label(kind: &ChannelKind) -> &'static str {
    match kind {
        ChannelKind::Dimmer => "Dimmer",
        ChannelKind::Color { mode } => match mode {
            ColorMode::Rgb => "RGB",
            ColorMode::Rgbw { .. } => "RGBW",
            ColorMode::Cmy => "CMY",
            ColorMode::Hs => "H/S",
        },
        ChannelKind::PanTilt { fine } => if *fine { "Pan/Tilt 16bit" } else { "Pan/Tilt" },
        ChannelKind::Raw { .. } => "Raw",
    }
}
