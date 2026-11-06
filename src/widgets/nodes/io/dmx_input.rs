use std::any::Any;
use std::time::{Duration, Instant};

use egui::{self, Sense, Ui, Vec2};

use crate::dmx_input::{Change, DmxInputHub, format_address};
use crate::engine::nodes::io::dmx_input::{DmxEntry, DmxInputNodeConfig, DmxKind, FineLayout, output_ports};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

/// The most entries a node has.
pub const MAX_ENTRIES: usize = 64;
/// After the first channel moves, wait this long for the rest of a group
/// (the three of a colour move together) before deciding which is the first.
const LEARN_SETTLE: Duration = Duration::from_millis(250);

/// An entry waiting for a channel to move on the desk.
#[derive(Debug, Clone, Copy)]
struct Learning {
    entry: u32,
    /// Changes up to this number came before and do not count.
    since: u64,
    first_seen: Option<Instant>,
}

/// Which channel to learn from what moved: the lowest one of the first
/// universe, since a multi-channel entry starts at its first channel.
pub fn learn_pick(changes: &[Change], universes: &[u16]) -> Option<(u16, usize)> {
    changes
        .iter()
        .map(|c| (universes.iter().position(|u| *u == c.universe).unwrap_or(usize::MAX), c.universe, c.channel))
        .min()
        .map(|(_, universe, channel)| (universe, channel))
}

/// Pairs of entries (by index) that use the same channel of the same universe.
pub fn overlaps(entries: &[DmxEntry]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (i, a) in entries.iter().enumerate() {
        for (j, b) in entries.iter().enumerate().skip(i + 1) {
            let (ca, cb) = (a.channels(), b.channels());
            if a.universe == b.universe && ca.start < cb.end && cb.start < ca.end {
                out.push((i, j));
            }
        }
    }
    out
}

/// Where the next entry could go: the channel after the last one of the last
/// entry in `universe`, or the first.
pub fn next_free(entries: &[DmxEntry], universe: u16) -> u16 {
    let end = entries.iter().filter(|e| e.universe == universe).map(|e| e.channels().end).max().unwrap_or(0);
    (end as u16 + 1).clamp(1, 512)
}

/// The channel layout as text, for patching the desk to match.
pub fn patch_text(entries: &[DmxEntry]) -> String {
    let mut rows: Vec<(u16, usize, String)> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let c = e.channels();
            let range = if c.len() <= 1 { format!("{}", c.start + 1) } else { format!("{}-{}", c.start + 1, c.end) };
            (e.universe, c.start, format!("universe {}  channel {range}: {} ({})", format_address(e.universe), e.port_name(i), e.kind.label()))
        })
        .collect();
    rows.sort();
    rows.into_iter().map(|(_, _, line)| line).collect::<Vec<_>>().join("\n")
}

pub struct DmxInputWidget {
    id: NodeId,
    shared: SharedState,
    hub: DmxInputHub,
    config: DmxInputNodeConfig,
    learning: Option<Learning>,
    /// Set when an entry is removed, for the graph to move the wires after it.
    removed: Option<usize>,
}

impl DmxInputWidget {
    pub fn new(id: NodeId, shared: SharedState, hub: DmxInputHub) -> Self {
        Self { id, shared, hub, config: DmxInputNodeConfig::default(), learning: None, removed: None }
    }

    #[cfg(test)]
    pub fn config(&self) -> &DmxInputNodeConfig {
        &self.config
    }

    fn push_config(&self) {
        if let Ok(value) = serde_json::to_value(&self.config) {
            self.shared.lock().unwrap().pending_config = Some(value);
        }
    }

    /// Restore the saved list before wires are attached, so wires to its
    /// ports still have ports to land on.
    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Ok(config) = serde_json::from_value::<DmxInputNodeConfig>(data.clone()) {
            self.config = config;
        }
    }

    /// Take entry `i` out of the list; the graph moves the wires after it.
    pub fn remove_entry(&mut self, i: usize) {
        let id = self.config.entries.remove(i).id;
        if self.learning.is_some_and(|l| l.entry == id) {
            self.learning = None;
        }
        self.removed = Some(i);
        self.push_config();
    }

    /// Look at what moved since learning started.
    fn learn_step(&mut self, ui: &Ui) {
        let Some(mut l) = self.learning else { return };
        let universes = self.hub.input_universes(self.config.input);
        let moved = self.hub.changes_since(l.since, &universes);
        if !moved.is_empty() && l.first_seen.is_none() {
            l.first_seen = Some(Instant::now());
            self.learning = Some(l);
        }
        if l.first_seen.is_some_and(|t| t.elapsed() >= LEARN_SETTLE)
            && let Some((universe, channel)) = learn_pick(&moved, &universes)
        {
            if let Some(e) = self.config.entries.iter_mut().find(|e| e.id == l.entry) {
                e.universe = universe;
                e.address = channel as u16 + 1;
            }
            self.learning = None;
            self.push_config();
        }
        ui.ctx().request_repaint_after(Duration::from_millis(50));
    }

    fn live(&self) -> bool {
        self.shared.lock().unwrap().outputs.first().copied().unwrap_or(0.0) >= 0.5
    }

    /// The first output channel of each entry.
    fn bases(&self) -> Vec<usize> {
        let mut base = 1;
        self.config
            .entries
            .iter()
            .map(|e| {
                let b = base;
                base += e.kind.port_type().channel_count();
                b
            })
            .collect()
    }
}

impl NodeWidget for DmxInputWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "DMX Input" }
    fn description(&self) -> &'static str {
        "Channels of a lighting desk that sends us DMX over Art-Net, as outputs: values, gates, triggers, colours and positions. Add an Art-Net input in Setup, then add an entry here for each thing the desk controls and press Learn to find its channel."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        output_ports(&self.config.entries).iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 170.0 }
    fn min_content_height(&self) -> f32 { 24.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn take_removed_port(&mut self) -> Option<(PortDir, usize)> {
        self.removed.take().map(|i| (PortDir::Output, i + 1))
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.learn_step(ui);
        match self.hub.input_name(self.config.input) {
            None => {
                ui.colored_label(theme::TEXT_DIM, "No input. Pick one in the inspector.");
            }
            Some(name) if self.live() => {
                ui.colored_label(theme::SEM_SUCCESS, format!("{name} (receiving)"));
            }
            Some(name) => {
                ui.colored_label(theme::SEM_WARNING, format!("{name} (no data)"));
            }
        }
        let bases = self.bases();
        let shared = self.shared.lock().unwrap();
        for (i, entry) in self.config.entries.iter().enumerate() {
            let value = |k: usize| shared.outputs.get(bases[i] + k).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            ui.horizontal(|ui| {
                let c = entry.channels();
                ui.colored_label(theme::TEXT_DIM, format!("{}  {}", entry.port_name(i), c.start + 1));
                if entry.kind.port_type() == PortType::Color {
                    let (r, p) = ui.allocate_painter(Vec2::new(24.0, 8.0), Sense::hover());
                    let rgb = |v: f32| (v * 255.0) as u8;
                    p.rect_filled(r.rect, 1.0, egui::Color32::from_rgb(rgb(value(0)), rgb(value(1)), rgb(value(2))));
                } else {
                    ui.add(egui::ProgressBar::new(value(0)).desired_width(40.0).desired_height(6.0));
                }
            });
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        self.learn_step(ui);
        let mut changed = false;

        let choices = self.hub.input_choices();
        ui.horizontal(|ui| {
            ui.label("Input:");
            let current = choices.iter().find(|(id, _)| *id == self.config.input).map(|(_, n)| n.clone()).unwrap_or_else(|| "(none)".into());
            egui::ComboBox::from_id_salt(("dmx_in_pick", self.id.0)).selected_text(current).show_ui(ui, |ui| {
                if ui.selectable_label(self.config.input == 0, "(none)").clicked() && self.config.input != 0 {
                    self.config.input = 0;
                    changed = true;
                }
                for (id, name) in &choices {
                    if ui.selectable_label(self.config.input == *id, name).clicked() && self.config.input != *id {
                        self.config.input = *id;
                        changed = true;
                    }
                }
            });
        });
        if choices.is_empty() {
            ui.colored_label(theme::SEM_WARNING, "There is no DMX input yet. Add one in Setup, DMX Inputs.");
        }
        let universes = self.hub.input_universes(self.config.input);

        ui.add_space(4.0);
        ui.label(egui::RichText::new("Channels").strong());
        let clashes = overlaps(&self.config.entries);
        let mut remove: Option<usize> = None;
        let mut learn: Option<u32> = None;
        for (i, entry) in self.config.entries.iter_mut().enumerate() {
            ui.horizontal_wrapped(|ui| {
                let hint = format!("ch {}", i + 1);
                changed |= ui.add(egui::TextEdit::singleline(&mut entry.name).hint_text(hint).desired_width(90.0)).changed();
                egui::ComboBox::from_id_salt(("dmx_in_kind", entry.id)).selected_text(entry.kind.label()).show_ui(ui, |ui| {
                    for kind in DmxKind::ALL {
                        if ui.selectable_label(entry.kind == kind, kind.label()).clicked() && entry.kind != kind {
                            entry.kind = kind;
                            changed = true;
                        }
                    }
                });
                if universes.len() > 1 || !universes.contains(&entry.universe) {
                    egui::ComboBox::from_id_salt(("dmx_in_universe", entry.id))
                        .selected_text(format!("univ. {}", format_address(entry.universe)))
                        .show_ui(ui, |ui| {
                            for u in &universes {
                                if ui.selectable_label(entry.universe == *u, format_address(*u)).clicked() && entry.universe != *u {
                                    entry.universe = *u;
                                    changed = true;
                                }
                            }
                        });
                }
                changed |= ui.add(egui::DragValue::new(&mut entry.address).range(1..=512).prefix("ch ")).changed();
                if entry.kind.has_threshold() {
                    changed |= ui.add(egui::DragValue::new(&mut entry.threshold).range(0.0..=1.0).speed(0.01).prefix("on above ")).changed();
                }
                if entry.kind.has_fine() {
                    egui::ComboBox::from_id_salt(("dmx_in_layout", entry.id)).selected_text(entry.layout.label()).show_ui(ui, |ui| {
                        for l in [FineLayout::Pairs, FineLayout::Block] {
                            if ui.selectable_label(entry.layout == l, l.label()).clicked() && entry.layout != l {
                                entry.layout = l;
                                changed = true;
                            }
                        }
                    });
                }
                let listening = self.learning.is_some_and(|l| l.entry == entry.id);
                let label = if listening { "Move a channel on the desk... (cancel)" } else { "Learn" };
                if ui.selectable_label(listening, label).on_hover_text("Then move the fader or press the button on the desk").clicked() {
                    learn = Some(entry.id);
                }
                if ui.small_button(egui_phosphor::regular::X).on_hover_text("Remove this channel").clicked() {
                    remove = Some(i);
                }
            });
            let c = entry.channels();
            let used = if c.len() <= 1 { format!("uses channel {}", c.start + 1) } else { format!("uses channels {} to {}", c.start + 1, c.end) };
            let clash = clashes.iter().find(|(a, b)| *a == i || *b == i);
            if !entry.fits() {
                ui.colored_label(theme::SEM_DANGER, format!("{used}: runs past channel 512"));
            } else if let Some((a, b)) = clash {
                ui.colored_label(theme::SEM_WARNING, format!("{used}: overlaps channel {}", if *a == i { *b + 1 } else { *a + 1 }));
            } else {
                ui.colored_label(theme::TEXT_DIM, used);
            }
        }
        if let Some(id) = learn {
            self.learning = if self.learning.is_some_and(|l| l.entry == id) {
                None
            } else {
                Some(Learning { entry: id, since: self.hub.latest_seq(), first_seen: None })
            };
        }
        if let Some(i) = remove {
            self.remove_entry(i);
            changed = false;
        }
        if self.config.entries.len() < MAX_ENTRIES && ui.button(format!("{} Add channel", egui_phosphor::regular::PLUS)).clicked() {
            let id = self.config.entries.iter().map(|e| e.id).max().map_or(1, |m| m + 1);
            let universe = universes.first().copied().unwrap_or(0);
            let mut e = DmxEntry::new(id, universe);
            e.address = next_free(&self.config.entries, universe);
            self.config.entries.push(e);
            changed = true;
        }

        if !self.config.entries.is_empty() {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Patch for the desk").strong());
            let text = patch_text(&self.config.entries);
            ui.colored_label(theme::TEXT_DIM, &text);
            if ui.small_button("Copy").clicked() {
                ui.ctx().copy_text(text);
            }
        }
        if changed {
            self.push_config();
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmx_input::{DmxInputConfig, DmxInputKind};
    use std::net::Ipv4Addr;

    fn entry(id: u32, kind: DmxKind, universe: u16, address: u16) -> DmxEntry {
        let mut e = DmxEntry::new(id, universe);
        e.kind = kind;
        e.address = address;
        e
    }

    fn change(universe: u16, channel: usize) -> Change {
        Change { seq: 1, universe, channel, value: 1 }
    }

    #[test]
    fn learning_picks_the_lowest_channel_of_the_first_universe_that_moved() {
        let universes = [4, 5];
        assert_eq!(learn_pick(&[], &universes), None);
        // A colour fader moves its three channels together; the first is the start.
        assert_eq!(learn_pick(&[change(4, 12), change(4, 11), change(4, 10)], &universes), Some((4, 10)));
        // The input's own order wins over the number.
        assert_eq!(learn_pick(&[change(5, 0), change(4, 300)], &[5, 4]), Some((5, 0)));
        assert_eq!(learn_pick(&[change(5, 7), change(4, 300)], &universes), Some((4, 300)));
    }

    #[test]
    fn overlapping_and_free_channels_are_found() {
        let e = vec![
            entry(1, DmxKind::Value, 0, 1),
            entry(2, DmxKind::Color, 0, 2),
            entry(3, DmxKind::Color16, 0, 4),
            entry(4, DmxKind::Value, 1, 4),
        ];
        // Color uses 2-4, Color16 uses 4-9: they meet at 4. The one in universe 1 is elsewhere.
        assert_eq!(overlaps(&e), vec![(1, 2)]);
        assert_eq!(next_free(&e, 0), 10);
        assert_eq!(next_free(&e, 1), 5);
        assert_eq!(next_free(&e, 9), 1);
        assert_eq!(next_free(&[entry(1, DmxKind::Value16, 0, 511)], 0), 512);
    }

    #[test]
    fn the_patch_text_lists_the_channels_in_order_for_the_desk() {
        let mut dim = entry(1, DmxKind::Value, 0, 1);
        dim.name = "Dimmer".into();
        let col = entry(2, DmxKind::Color, 0, 2);
        let text = patch_text(&[col, dim]);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("channel 1:") && lines[0].contains("Dimmer"));
        assert!(lines[1].contains("channel 2-4") && lines[1].contains("Color (RGB, 8 bit)"));
    }

    fn widget() -> (DmxInputWidget, DmxInputHub) {
        let hub = DmxInputHub::idle();
        let mut input = DmxInputConfig::new_art_net(1, "Desk");
        input.kind = DmxInputKind::ArtNet { universes: vec![0, 1], interface: String::new() };
        hub.set_inputs(&[input]);
        (DmxInputWidget::new(NodeId(3), new_shared_state(0, 200), hub.clone()), hub)
    }

    #[test]
    fn the_list_comes_back_from_the_saved_form_with_typed_ports() {
        let (mut w, _) = widget();
        w.restore_from_save_data(&serde_json::json!({ "input": 1, "entries": [
            {"id": 1, "name": "Dim", "kind": "value", "address": 1},
            {"id": 2, "kind": "color", "address": 2},
            {"id": 3, "kind": "gate", "address": 5}]}));
        let ports: Vec<(String, PortType)> = w.ui_outputs().into_iter().map(|p| (p.def.name, p.def.port_type)).collect();
        assert_eq!(ports[1], ("Dim".to_string(), PortType::Untyped));
        assert_eq!(ports[2].1, PortType::Color);
        assert_eq!(ports[3].1, PortType::Logic);
        assert_eq!(w.config().input, 1);
        assert_eq!(w.bases(), vec![1, 2, 5], "a colour takes three output channels");
        w.remove_entry(1);
        assert_eq!(w.take_removed_port(), Some((PortDir::Output, 2)));
        assert_eq!(w.take_removed_port(), None);
    }

    #[test]
    fn a_moved_channel_is_learned_after_the_group_has_settled() {
        let (mut w, hub) = widget();
        w.restore_from_save_data(&serde_json::json!({ "input": 1, "entries": [{"id": 1, "kind": "color", "address": 100}]}));
        let desk = Ipv4Addr::new(2, 0, 0, 1);
        hub.apply_dmx(1, &[0; 40], desk);
        w.learning = Some(Learning { entry: 1, since: hub.latest_seq(), first_seen: None });
        let ctx = egui::Context::default();
        let step = |w: &mut DmxInputWidget| {
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| w.learn_step(ui));
            });
        };
        let mut frame = [0u8; 40];
        frame[20] = 9;
        frame[19] = 9;
        frame[21] = 9;
        hub.apply_dmx(1, &frame, desk);
        step(&mut w);
        assert!(w.learning.is_some(), "it waits for the other channels of the group");
        std::thread::sleep(LEARN_SETTLE + Duration::from_millis(50));
        step(&mut w);
        assert!(w.learning.is_none());
        let e = &w.config().entries[0];
        assert_eq!((e.universe, e.address), (1, 20), "channel 19 (zero based) is the first of 19, 20, 21");
    }
}
