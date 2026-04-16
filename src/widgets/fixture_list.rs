use egui::{self, Color32, Ui};

use crate::objects::channel::{Channel, ChannelKind, ColorMode, PixelChannel, PixelFormat};
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
                        let mut channels_changed = false;
                        for ch in &mut fixture.channels {
                            ui.horizontal(|ui| {
                                ui.label(format!("{} ({}):", ch.name, kind_label(&ch.kind)));
                                // Per-kind extra editors.
                                if let ChannelKind::LedStrip { count, format } = &mut ch.kind {
                                    let mut c = *count as i32;
                                    if ui.add(egui::DragValue::new(&mut c).range(1..=512).prefix("LEDs: ")).changed() {
                                        *count = c.max(1) as usize;
                                        ch.values.resize(*count * 3, 0.0);
                                        channels_changed = true;
                                    }

                                    // Format preset dropdown.
                                    egui::ComboBox::from_id_salt(("strip_fmt", &ch.name as *const _))
                                        .selected_text(format.label())
                                        .show_ui(ui, |ui| {
                                            let presets = [
                                                ("RGB",    PixelFormat::rgb()),
                                                ("GRB",    PixelFormat::grb()),
                                                ("RGBW",   PixelFormat::rgbw(6500)),
                                                ("GRBW",   PixelFormat::grbw(6500)),
                                                ("GRBWW",  PixelFormat::grbww(3000, 3000)),
                                            ];
                                            for (label, preset) in presets {
                                                if ui.selectable_label(false, label).clicked() {
                                                    *format = preset;
                                                    channels_changed = true;
                                                }
                                            }
                                        });

                                    // Per-W-channel temperature pickers.
                                    let mut w_index = 0;
                                    for ch_kind in format.channels.iter_mut() {
                                        if let PixelChannel::White { temperature_k } = ch_kind {
                                            w_index += 1;
                                            let mut t = *temperature_k as i32;
                                            if ui.add(
                                                egui::DragValue::new(&mut t)
                                                    .range(2000..=10000)
                                                    .suffix("K")
                                                    .prefix(format!("W{}: ", w_index))
                                            ).changed() {
                                                *temperature_k = t.clamp(2000, 10000) as u16;
                                                channels_changed = true;
                                            }
                                        }
                                    }
                                }
                            });
                        }
                        if channels_changed {
                            fixture.recalc_offsets();
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
                            if ui.small_button("+ LED Strip").clicked() {
                                let n = format!("Strip {}", fixture.channels.len() + 1);
                                fixture.add_channel(Channel::led_strip(&n, 60, PixelFormat::rgb()));
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
        ChannelKind::LedStrip { .. } => "LED Strip",
    }
}
