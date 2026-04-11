use egui::{self, Color32, Ui};

use crate::objects::channel::{Channel, ChannelKind, ColorMode};
use crate::objects::fixture::Fixture;

/// Manages fixture templates (channel definitions, no addresses).
pub struct FixtureManager {
    pub fixtures: Vec<Fixture>,
    next_id: u32,
}

impl FixtureManager {
    pub fn new() -> Self {
        Self { fixtures: Vec::new(), next_id: 1 }
    }

    pub fn from_fixtures(fixtures: Vec<Fixture>) -> Self {
        let next_id = fixtures.iter().map(|f| f.id).max().unwrap_or(0) + 1;
        Self { fixtures, next_id }
    }

    pub fn show(&mut self, ui: &mut Ui) {
        ui.heading("Fixture Templates");
        ui.separator();

        if self.fixtures.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No fixture templates.");
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
                                    &n, ColorMode::Rgbw { white_temperature: 6500 },
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
                        if ui.small_button("Delete template").clicked() {
                            remove_id = Some(fixture.id);
                        }
                    });
                });
            }
        });

        if let Some(id) = remove_id {
            self.fixtures.retain(|f| f.id != id);
        }

        ui.separator();
        if ui.button("Add Fixture Template").clicked() {
            let id = self.next_id;
            self.next_id += 1;
            let mut fixture = Fixture::new(id, format!("Fixture {}", id));
            fixture.add_channel(Channel::dimmer("Dimmer"));
            fixture.add_channel(Channel::color("Color", ColorMode::Rgb));
            self.fixtures.push(fixture);
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
