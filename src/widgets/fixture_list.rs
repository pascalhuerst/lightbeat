use egui::{self, Ui};

use crate::objects::channel::{Channel, ChannelKind, ColorMode, PixelChannel, PixelFormat};
use crate::objects::fixture::Fixture;
use crate::theme;
use crate::widgets::setup_ui::{self, ItemHeader, Move};

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
        if self.fixtures.is_empty() {
            setup_ui::empty_state(ui, "No templates yet. Add one below.");
        }

        let open = setup_ui::open_by_default(self.fixtures.len());
        let mut remove_id = None;
        let mut duplicate_id = None;

        for fixture in &mut self.fixtures {
            let title = fixture.name.clone();
            let mut header = ItemHeader::new(&title, "Delete template");
            header.duplicate = Some("Duplicate template");
            let actions = setup_ui::item_header(ui, fixture.id, open, header, |ui| {
                setup_ui::name_field(ui, &mut fixture.name);
                setup_ui::info_row(ui, "Footprint", &format!("{} DMX channels", fixture.dmx_footprint()));

                ui.add_space(4.0);
                setup_ui::section(ui, "Channels");
                show_channels(ui, fixture);
                ui.add_space(4.0);
            });
            if actions.delete {
                remove_id = Some(fixture.id);
            }
            if actions.duplicate {
                duplicate_id = Some(fixture.id);
            }
        }

        if let Some(id) = remove_id {
            self.fixtures.retain(|f| f.id != id);
        }
        if let Some(id) = duplicate_id {
            self.duplicate(id);
        }

        ui.separator();
        if setup_ui::add_button(ui, "Add template") {
            let id = self.next_id;
            self.next_id += 1;
            let mut fixture = Fixture::new(id, format!("Template {}", id));
            fixture.add_channel(Channel::dimmer("Dimmer"));
            fixture.add_channel(Channel::color("Color", ColorMode::Rgb));
            self.fixtures.push(fixture);
        }
    }

    /// A copy right after the original, with a fresh id.
    fn duplicate(&mut self, id: u32) {
        let Some(pos) = self.fixtures.iter().position(|f| f.id == id) else { return };
        let mut copy = self.fixtures[pos].clone();
        copy.id = self.next_id;
        self.next_id += 1;
        copy.name = setup_ui::copy_name(&copy.name);
        self.fixtures.insert(pos + 1, copy);
    }
}

/// Every kind a channel can have, with the base name a new one gets.
const NEW_CHANNEL_KINDS: &[(&str, &str)] = &[
    ("Dimmer", "Dimmer"),
    ("RGB color", "Color"),
    ("RGBW color", "Color"),
    ("CMY color", "Color"),
    ("Hue and saturation", "Color"),
    ("Pan/tilt", "PanTilt"),
    ("Pan/tilt 16 bit", "PanTilt"),
    ("Raw channels", "Raw"),
    ("LED strip", "Strip"),
];

fn new_channel(kind_index: usize, number: usize) -> Channel {
    let name = format!("{} {}", NEW_CHANNEL_KINDS[kind_index].1, number);
    match kind_index {
        0 => Channel::dimmer(&name),
        1 => Channel::color(&name, ColorMode::Rgb),
        2 => Channel::color(&name, ColorMode::Rgbw { white_temperature: 6500 }),
        3 => Channel::color(&name, ColorMode::Cmy),
        4 => Channel::color(&name, ColorMode::Hs),
        5 => Channel::pan_tilt(&name, false),
        6 => Channel::pan_tilt(&name, true),
        7 => Channel::raw(&name, 1),
        _ => Channel::led_strip(&name, 60, PixelFormat::rgb()),
    }
}

/// Offsets follow the order of the list, so every change that reorders or
/// removes a channel renumbers them.
fn remove_channel(fixture: &mut Fixture, index: usize) {
    if index < fixture.channels.len() {
        fixture.channels.remove(index);
        fixture.recalc_offsets();
    }
}

fn move_channel(fixture: &mut Fixture, index: usize, mv: Move) {
    if setup_ui::apply_move(&mut fixture.channels, index, mv) {
        fixture.recalc_offsets();
    }
}

fn show_channels(ui: &mut Ui, fixture: &mut Fixture) {
    let mut channels_changed = false;
    let mut remove = None;
    let mut mv = None;
    let n = fixture.channels.len();
    for (i, ch) in fixture.channels.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            ui.horizontal(|ui| {
                if let Some(m) = setup_ui::move_buttons(ui, i, n) {
                    mv = Some((i, m));
                }
                ui.add(egui::TextEdit::singleline(&mut ch.name).desired_width(110.0));
                ui.colored_label(theme::TEXT_DIM, kind_label(&ch.kind));
                match &mut ch.kind {
                    ChannelKind::LedStrip { count, format } => {
                        let mut c = *count as i32;
                        if ui.add(egui::DragValue::new(&mut c).range(1..=512).prefix("LEDs ")).changed() {
                            *count = c.max(1) as usize;
                            ch.values.resize(*count * 3, 0.0);
                            channels_changed = true;
                        }

                        egui::ComboBox::from_id_salt("strip_fmt")
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

                        let mut w_index = 0;
                        for ch_kind in format.channels.iter_mut() {
                            if let PixelChannel::White { temperature_k } = ch_kind {
                                w_index += 1;
                                let mut t = *temperature_k as i32;
                                if ui.add(
                                    egui::DragValue::new(&mut t)
                                        .range(2000..=10000)
                                        .suffix("K")
                                        .prefix(format!("W{} ", w_index))
                                ).changed() {
                                    *temperature_k = t.clamp(2000, 10000) as u16;
                                    channels_changed = true;
                                }
                            }
                        }
                    }
                    ChannelKind::Raw { count } => {
                        let mut c = *count as i32;
                        if ui.add(egui::DragValue::new(&mut c).range(1..=512).suffix(" ch")).changed() {
                            *count = c.max(1) as usize;
                            ch.values.resize(*count, 0.0);
                            channels_changed = true;
                        }
                    }
                    _ => {}
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if setup_ui::delete_button(ui, "Delete channel") {
                        remove = Some(i);
                    }
                });
            });
        });
    }
    if channels_changed {
        fixture.recalc_offsets();
    }
    if let Some((i, m)) = mv {
        move_channel(fixture, i, m);
    }
    if let Some(i) = remove {
        remove_channel(fixture, i);
    }

    let kinds = NEW_CHANNEL_KINDS.iter().enumerate().map(|(i, (label, _))| (i, label.to_string())).collect();
    if let Some(k) = setup_ui::add_menu(ui, ("add_channel", fixture.id), "Add channel...", kinds) {
        let number = fixture.channels.len() + 1;
        fixture.add_channel(new_channel(k, number));
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
        ChannelKind::PanTilt { fine } => if *fine { "Pan/tilt 16 bit" } else { "Pan/tilt" },
        ChannelKind::Raw { .. } => "Raw",
        ChannelKind::LedStrip { .. } => "LED strip",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Fixture {
        let mut f = Fixture::new(1, "Par");
        f.add_channel(Channel::dimmer("Dimmer"));
        f.add_channel(Channel::color("Color", ColorMode::Rgb));
        f.add_channel(Channel::pan_tilt("Pan", false));
        f
    }

    fn offsets(f: &Fixture) -> Vec<u16> {
        f.channels.iter().map(|c| c.offset).collect()
    }

    #[test]
    fn deleting_a_channel_closes_the_gap() {
        let mut f = sample();
        assert_eq!(offsets(&f), [0, 1, 4]);
        remove_channel(&mut f, 1);
        assert_eq!(offsets(&f), [0, 1]);
        assert_eq!(f.dmx_footprint(), 3);
    }

    #[test]
    fn moving_a_channel_renumbers_offsets() {
        let mut f = sample();
        move_channel(&mut f, 1, Move::Down);
        assert_eq!(f.channels[1].name, "Pan");
        assert_eq!(offsets(&f), [0, 1, 3]);
        move_channel(&mut f, 0, Move::Up);
        assert_eq!(offsets(&f), [0, 1, 3]);
    }

    #[test]
    fn every_new_kind_builds_a_channel() {
        let mut f = Fixture::new(1, "All");
        for k in 0..NEW_CHANNEL_KINDS.len() {
            let n = f.channels.len() + 1;
            f.add_channel(new_channel(k, n));
        }
        assert_eq!(f.channels.len(), NEW_CHANNEL_KINDS.len());
        assert_eq!(f.dmx_footprint(), 1 + 3 + 4 + 3 + 2 + 2 + 4 + 1 + 180);
    }

    #[test]
    fn duplicate_gets_a_fresh_id_and_name() {
        let mut m = FixtureManager::from_fixtures(vec![sample()]);
        m.duplicate(1);
        assert_eq!(m.fixtures.len(), 2);
        assert_eq!(m.fixtures[1].id, 2);
        assert_eq!(m.fixtures[1].name, "Par copy");
        assert_eq!(m.fixtures[1].channels, m.fixtures[0].channels);
    }
}
