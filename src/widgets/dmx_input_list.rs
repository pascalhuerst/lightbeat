//! The "DMX Inputs" tab of the setup: where lighting desks that send us
//! Art-Net are described, and a live view of what arrives.

use std::net::Ipv4Addr;
use std::time::Duration;

use egui::{self, Sense, Stroke, Ui, Vec2};

use crate::dmx_input::artnet::local_interfaces;
use crate::dmx_input::{ARTNET_PORT, DmxInputConfig, DmxInputHub, DmxInputKind, Universe, join_address, split_address};
use crate::theme;

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

fn status_dot(ui: &mut Ui, on: bool) {
    let (r, p) = ui.allocate_painter(Vec2::splat(10.0), Sense::hover());
    p.circle_filled(r.rect.center(), 4.0, if on { theme::SEM_SUCCESS } else { theme::STROKE });
}

pub fn show(ui: &mut Ui, hub: &DmxInputHub) {
    ui.heading("DMX Inputs");
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
    let interfaces = local_interfaces();

    if inputs.is_empty() {
        ui.colored_label(theme::TEXT_DIM, "No DMX inputs yet.");
    }
    let heard = hub.heard_unconfigured();

    for input in &mut inputs {
        let status = hub.status(input.id);
        ui.push_id(input.id, |ui| {
            let live = status.as_ref().is_some_and(|s| s.live);
            ui.horizontal(|ui| {
                status_dot(ui, live);
                ui.label(egui::RichText::new(format!("{}  ({})", input.name, input.kind.label())).strong());
            });
            ui.indent("dmx_input_body", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    changed |= ui.text_edit_singleline(&mut input.name).changed();
                    changed |= ui.checkbox(&mut input.enabled, "Enabled").changed();
                    if ui.small_button(egui_phosphor::regular::X).on_hover_text("Remove this input").clicked() {
                        remove = Some(input.id);
                    }
                });
                let DmxInputKind::ArtNet { universes, interface } = &mut input.kind;
                ui.horizontal(|ui| {
                    ui.label("Announce on:");
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

                ui.label(egui::RichText::new("Universes").strong());
                let mut drop_universe: Option<usize> = None;
                for (i, u) in universes.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let (mut net, mut sub, mut uni) = split_address(*u);
                        let c = ui.add(egui::DragValue::new(&mut net).range(0..=127).prefix("net ")).changed()
                            | ui.add(egui::DragValue::new(&mut sub).range(0..=15).prefix("sub ")).changed()
                            | ui.add(egui::DragValue::new(&mut uni).range(0..=15).prefix("uni ")).changed();
                        if c {
                            *u = join_address(net, sub, uni);
                            changed = true;
                        }
                        ui.colored_label(theme::TEXT_DIM, format!("= {}", *u))
                            .on_hover_text("Art-Net port address. Most desks count universes from 1, so 0 is their universe 1.");
                        if let Some(st) = &status
                            && let Some((_, data)) = st.universes.get(i)
                        {
                            status_dot(ui, data.live());
                            match (data.live(), data.from) {
                                (true, Some(from)) => {
                                    ui.colored_label(theme::TEXT, format!("{:.0} packets/s from {from}", data.rate));
                                }
                                _ => {
                                    ui.colored_label(theme::TEXT_DIM, "no data");
                                }
                            }
                        }
                        if ui.small_button(egui_phosphor::regular::X).on_hover_text("Remove this universe").clicked() {
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
                if ui.small_button(format!("{} Add universe", egui_phosphor::regular::PLUS)).clicked() {
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
                    } else if input.enabled && !universes.is_empty() {
                        if st.listening {
                            ui.colored_label(theme::TEXT_DIM, format!("Listening on UDP port {ARTNET_PORT}."));
                        }
                    } else {
                        ui.colored_label(theme::TEXT_DIM, "Not listening: enable it and add a universe.");
                    }
                }
            });
            ui.add_space(8.0);
        });
    }

    if let Some(id) = remove {
        inputs.retain(|i| i.id != id);
        changed = true;
    }
    if ui.button(format!("{} Add Art-Net input", egui_phosphor::regular::PLUS)).clicked() {
        let id = inputs.iter().map(|i| i.id).max().map_or(1, |m| m + 1);
        inputs.push(DmxInputConfig::new_art_net(id, if inputs.is_empty() { "Desk".to_string() } else { format!("Desk {id}") }));
        changed = true;
    }
    if changed {
        hub.set_inputs(&inputs);
    }
}
