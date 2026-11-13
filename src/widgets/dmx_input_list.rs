//! The "DMX Inputs" tab of the setup: where lighting desks that send us
//! Art-Net are described, and a live view of what arrives.

use std::net::Ipv4Addr;
use std::time::Duration;

use egui::{self, Sense, Stroke, Ui, Vec2};

use crate::dmx_input::artnet::local_interfaces;
use crate::dmx_input::{ARTNET_PORT, DmxInputConfig, DmxInputHub, DmxInputKind, Universe, join_address, split_address};
use crate::theme;
use crate::widgets::setup_ui::{self, ItemHeader, Status};

/// A bar per channel, so the shape of what the desk sends can be seen.
fn draw_strip(ui: &mut Ui, universe: &Universe) {
    let width = ui.available_width().max(128.0);
    let (resp, painter) = ui.allocate_painter(Vec2::new(width, 28.0), Sense::hover());
    painter.rect_filled(resp.rect, 2.0, theme::BG_DEEP);
    let slot = resp.rect.width() / 512.0;
    for (c, v) in universe.data.iter().enumerate() {
        if *v == 0 {
            continue;
        }
        let h = resp.rect.height() * f32::from(*v) / 255.0;
        let x = resp.rect.min.x + c as f32 * slot;
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(x, resp.rect.max.y - h), egui::pos2(x + slot.max(1.0), resp.rect.max.y)),
            0.0,
            theme::SEM_PRIMARY,
        );
    }
    painter.rect_stroke(resp.rect, 2.0, Stroke::new(1.0_f32, theme::STROKE), egui::StrokeKind::Inside);
    if let Some(pos) = resp.hover_pos() {
        let c = (((pos.x - resp.rect.min.x) / slot) as usize).min(511);
        resp.on_hover_text(format!("channel {}: {}", c + 1, universe.data[c]));
    }
}

/// Live, waiting, disabled or empty: the words every tab uses.
fn input_status(enabled: bool, universe_count: usize, live: bool) -> Status {
    if !enabled {
        Status::Disabled
    } else if universe_count == 0 {
        Status::NoMapping
    } else if live {
        Status::Connected
    } else {
        Status::Waiting
    }
}

/// A copy with a fresh id, kept as is otherwise: same universes, same network.
fn duplicate_input(inputs: &mut Vec<DmxInputConfig>, id: u32) {
    let Some(pos) = inputs.iter().position(|i| i.id == id) else { return };
    let mut copy = inputs[pos].clone();
    copy.id = inputs.iter().map(|i| i.id).max().map_or(1, |m| m + 1);
    copy.name = setup_ui::copy_name(&copy.name);
    inputs.insert(pos + 1, copy);
}

pub fn show(ui: &mut Ui, hub: &DmxInputHub) {
    ui.colored_label(
        theme::TEXT_DIM,
        "A lighting desk that sends us DMX over Art-Net. LightBeat shows up as an Art-Net node with the universes below; \
         patch the desk's output to it (or to this computer's address). DMX Input nodes read the channels.",
    );
    ui.separator();
    ui.ctx().request_repaint_after(Duration::from_millis(100));

    let mut inputs = hub.export();
    let mut changed = false;
    let mut remove: Option<u32> = None;
    let mut duplicate: Option<u32> = None;
    let interfaces = local_interfaces();

    if inputs.is_empty() {
        setup_ui::empty_state(ui, "No DMX inputs yet. Add one below.");
    }
    let heard = hub.heard_unconfigured();
    let open = setup_ui::open_by_default(inputs.len());

    for input in &mut inputs {
        let status = hub.status(input.id);
        let title = format!("{} ({})", input.name, input.kind.label());
        let universe_count = match &input.kind {
            DmxInputKind::ArtNet { universes, .. } => universes.len(),
        };
        let mut header = ItemHeader::new(&title, "Delete DMX input");
        header.duplicate = Some("Duplicate DMX input");
        header.enabled = Some(input.enabled);
        header.status = Some(input_status(
            input.enabled,
            universe_count,
            status.as_ref().is_some_and(|s| s.live),
        ));
        let enabled_now = input.enabled;
        let actions = setup_ui::item_header(ui, input.id, open, header, |ui| {
            changed |= setup_ui::name_field(ui, &mut input.name);
            let DmxInputKind::ArtNet { universes, interface } = &mut input.kind;
            ui.horizontal(|ui| {
                ui.label("Announce on");
                let shown = if interface.is_empty() {
                    "All networks".to_string()
                } else {
                    interface.clone()
                };
                egui::ComboBox::from_id_salt("dmx_in_interface").selected_text(shown).show_ui(ui, |ui| {
                    if ui.selectable_label(interface.is_empty(), "All networks").clicked() && !interface.is_empty() {
                        interface.clear();
                        changed = true;
                    }
                    for i in &interfaces {
                        let ip = i.ip.to_string();
                        if ui.selectable_label(*interface == ip, format!("{}  ({})", ip, i.name)).clicked() && *interface != ip {
                            *interface = ip;
                            changed = true;
                        }
                    }
                });
            });

            setup_ui::section(ui, "Universes");
            let mut drop_universe: Option<usize> = None;
            for (i, u) in universes.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let (mut net, mut sub, mut uni) = split_address(*u);
                    if setup_ui::dmx_address(ui, &mut net, &mut sub, &mut uni) {
                        *u = join_address(net, sub, uni);
                        changed = true;
                    }
                    ui.colored_label(theme::TEXT_DIM, format!("= {}", *u))
                        .on_hover_text("Art-Net port address. Most desks count universes from 1, so 0 is their universe 1.");
                    if let Some(st) = &status
                        && let Some((_, data)) = st.universes.get(i)
                    {
                        match (data.live(), data.from) {
                            (true, Some(from)) => {
                                setup_ui::status_badge(ui, Status::Connected);
                                ui.colored_label(theme::TEXT, format!("{:.0} packets/s from {from}", data.rate));
                            }
                            _ => {
                                setup_ui::status_badge(ui, Status::Waiting);
                                ui.colored_label(theme::TEXT_DIM, "no data");
                            }
                        }
                    }
                    if setup_ui::delete_button(ui, "Delete universe") {
                        drop_universe = Some(i);
                    }
                });
                if let Some(st) = &status
                    && let Some((_, data)) = st.universes.get(i)
                {
                    draw_strip(ui, data);
                }
            }
            if let Some(i) = drop_universe {
                universes.remove(i);
                changed = true;
            }
            if setup_ui::add_button(ui, "Add universe") {
                let next = universes.iter().max().map_or(0, |m| (m + 1).min(0x7FFF));
                universes.push(next);
                changed = true;
            }

            // What the desk sends that nothing listens to yet.
            let unlistened: Vec<&(u16, Ipv4Addr)> = heard.iter().filter(|(u, _)| !universes.contains(u)).collect();
            for (u, from) in unlistened {
                ui.horizontal(|ui| {
                    ui.colored_label(theme::SEM_WARNING, format!("Also receiving universe {} from {from}.", crate::dmx_input::format_address(*u)));
                    if ui.small_button("Listen to it").clicked() {
                        universes.push(*u);
                        changed = true;
                    }
                });
            }

            if let Some(st) = &status {
                if let Some(err) = &st.bind_error {
                    ui.colored_label(theme::SEM_DANGER, err);
                } else if enabled_now && !universes.is_empty() {
                    if st.listening {
                        ui.colored_label(theme::TEXT_DIM, format!("Listening on UDP port {ARTNET_PORT}."));
                    }
                } else {
                    ui.colored_label(theme::TEXT_DIM, "Not listening: enable it and add a universe.");
                }
            }
            ui.add_space(4.0);
        });
        if let Some(on) = actions.enabled {
            input.enabled = on;
            changed = true;
        }
        if actions.delete {
            remove = Some(input.id);
        }
        if actions.duplicate {
            duplicate = Some(input.id);
        }
    }

    if let Some(id) = remove {
        inputs.retain(|i| i.id != id);
        changed = true;
    }
    if let Some(id) = duplicate {
        duplicate_input(&mut inputs, id);
        changed = true;
    }
    ui.separator();
    if setup_ui::add_button(ui, "Add Art-Net input") {
        let id = inputs.iter().map(|i| i.id).max().map_or(1, |m| m + 1);
        inputs.push(DmxInputConfig::new_art_net(id, if inputs.is_empty() { "Desk".to_string() } else { format!("Desk {id}") }));
        changed = true;
    }
    if changed {
        hub.set_inputs(&inputs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_copies_the_universes_with_a_fresh_id() {
        let mut a = DmxInputConfig::new_art_net(3, "Desk");
        let DmxInputKind::ArtNet { universes, .. } = &mut a.kind;
        universes.extend([0, 17]);
        let mut inputs = vec![a];
        duplicate_input(&mut inputs, 3);
        assert_eq!(inputs[1].id, 4);
        assert_eq!(inputs[1].name, "Desk copy");
        assert_eq!(inputs[1].kind, inputs[0].kind);
    }

    #[test]
    fn status_words() {
        assert_eq!(input_status(false, 2, true), Status::Disabled);
        assert_eq!(input_status(true, 0, false), Status::NoMapping);
        assert_eq!(input_status(true, 1, true), Status::Connected);
        assert_eq!(input_status(true, 1, false), Status::Waiting);
    }
}
