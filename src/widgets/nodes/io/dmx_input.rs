use std::any::Any;
use std::time::{Duration, Instant};

use egui::{self, Sense, Ui, Vec2};

use crate::dmx_input::{Change, DmxInputHub, format_address};
use crate::engine::nodes::io::dmx_input::{ColorModel, DmxInputNodeConfig, DmxKind, DmxOutput, Source, output_ports};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

/// The most outputs a node has.
pub const MAX_OUTPUTS: usize = 64;
/// After the first channel moves, wait this long for the rest of a group
/// (the three of a colour move together) before deciding which is the first.
const LEARN_SETTLE: Duration = Duration::from_millis(250);

/// A source waiting for a channel to move on the desk.
#[derive(Debug, Clone, Copy)]
struct Learning {
    output: u32,
    source: usize,
    /// Changes up to this number came before and do not count.
    since: u64,
    first_seen: Option<Instant>,
}

/// Which channel to learn from what moved: the lowest one of the first
/// universe, so that the three channels of a colour fader give the first.
pub fn learn_pick(changes: &[Change], universes: &[u16]) -> Option<(u16, usize)> {
    changes
        .iter()
        .map(|c| (universes.iter().position(|u| *u == c.universe).unwrap_or(usize::MAX), c.universe, c.channel))
        .min()
        .map(|(_, universe, channel)| (universe, channel))
}

/// The channel after the last one any source uses in `universe`, or the first.
pub fn next_free(outputs: &[DmxOutput], universe: u16) -> u16 {
    let last = outputs
        .iter()
        .flat_map(|o| &o.sources)
        .filter(|s| s.universe == universe)
        .flat_map(|s| std::iter::once(s.coarse_index()).chain(s.fine_index()))
        .max();
    last.map_or(1, |l| (l as u16 + 2).min(512))
}

/// Every channel used, as text, for patching the desk to match.
pub fn patch_text(outputs: &[DmxOutput]) -> String {
    let mut rows: Vec<(u16, usize, String)> = Vec::new();
    for (i, o) in outputs.iter().enumerate() {
        let labels = o.source_labels();
        for (k, s) in o.sources.iter().enumerate() {
            let what = format!("{} {}", o.port_name(i), labels.get(k).copied().unwrap_or("").to_lowercase());
            rows.push((s.universe, s.coarse_index(), format!("universe {}  channel {}: {}", format_address(s.universe), s.coarse_index() + 1, what.trim())));
            if let Some(f) = s.fine_index() {
                rows.push((s.universe, f, format!("universe {}  channel {}: {} (fine)", format_address(s.universe), f + 1, what.trim())));
            }
        }
    }
    rows.sort();
    rows.into_iter().map(|(_, _, line)| line).collect::<Vec<_>>().join("\n")
}

/// A level between 0 and 1 as the 0 to 255 a DMX channel counts in.
fn dmx_level(ui: &mut Ui, value: &mut f32, prefix: &str) -> bool {
    ui.add(
        egui::DragValue::new(value)
            .range(0.0..=1.0)
            .speed(0.004)
            .prefix(prefix)
            .custom_formatter(|v, _| format!("{:.0}", v * 255.0))
            .custom_parser(|s| s.trim().parse::<f64>().ok().map(|x| (x / 255.0).clamp(0.0, 1.0))),
    )
    .changed()
}

pub struct DmxInputWidget {
    id: NodeId,
    shared: SharedState,
    hub: DmxInputHub,
    config: DmxInputNodeConfig,
    learning: Option<Learning>,
    /// Set when an output is removed, for the graph to move the wires after it.
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
        if let Ok(mut config) = serde_json::from_value::<DmxInputNodeConfig>(data.clone()) {
            for o in &mut config.outputs {
                o.fit_sources();
            }
            self.config = config;
        }
    }

    pub fn add_output(&mut self, kind: DmxKind) {
        let universe = self.hub.input_universes(self.config.input).first().copied().unwrap_or(0);
        let id = self.config.outputs.iter().map(|o| o.id).max().map_or(1, |m| m + 1);
        self.config.outputs.push(DmxOutput::new(id, kind, universe, next_free(&self.config.outputs, universe)));
        self.push_config();
    }

    /// Take output `i` out of the list; the graph moves the wires after it.
    pub fn remove_output(&mut self, i: usize) {
        let id = self.config.outputs.remove(i).id;
        if self.learning.is_some_and(|l| l.output == id) {
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
            if let Some(s) = self.config.outputs.iter_mut().find(|o| o.id == l.output).and_then(|o| o.sources.get_mut(l.source)) {
                s.universe = universe;
                s.address = channel as u16 + 1;
                if s.fine.is_some() {
                    s.fine = Some((s.address + 1).min(512));
                }
            }
            self.learning = None;
            self.push_config();
        }
        ui.ctx().request_repaint_after(Duration::from_millis(50));
    }

    fn live(&self) -> bool {
        self.shared.lock().unwrap().outputs.first().copied().unwrap_or(0.0) >= 0.5
    }

    /// The first output channel of each output.
    fn bases(&self) -> Vec<usize> {
        let mut base = 1;
        self.config
            .outputs
            .iter()
            .map(|o| {
                let b = base;
                base += o.kind.port_type().channel_count();
                b
            })
            .collect()
    }

    /// What each source reads now, 0..1, for the bars in the editor.
    fn source_values(&self) -> Vec<Vec<f32>> {
        let requests: Vec<(u16, usize, usize)> = self
            .config
            .outputs
            .iter()
            .flat_map(|o| &o.sources)
            .flat_map(|s| std::iter::once((s.universe, s.coarse_index(), 1)).chain(s.fine_index().map(|f| (s.universe, f, 1))))
            .collect();
        let mut raw = Vec::new();
        self.hub.read_channels(&requests, &mut raw);
        let mut at = 0;
        self.config
            .outputs
            .iter()
            .map(|o| {
                o.sources
                    .iter()
                    .map(|s| {
                        let coarse = raw.get(at).copied().unwrap_or(0);
                        at += 1;
                        let fine = s.fine.map(|_| {
                            at += 1;
                            raw.get(at - 1).copied().unwrap_or(0)
                        });
                        s.value(coarse, fine)
                    })
                    .collect()
            })
            .collect()
    }
}

/// One source of an output: where it is read and how. Returns whether it
/// changed and whether Learn was pressed.
#[allow(clippy::too_many_arguments)]
fn source_row(
    ui: &mut Ui,
    salt: (u32, usize),
    label: &str,
    source: &mut Source,
    universes: &[u16],
    learning: bool,
    value: f32,
) -> (bool, bool) {
    let mut changed = false;
    let mut learn = false;
    ui.horizontal_wrapped(|ui| {
        ui.add_sized([62.0, 18.0], egui::Label::new(label));
        if universes.len() > 1 || !universes.contains(&source.universe) {
            egui::ComboBox::from_id_salt(("dmx_src_universe", salt))
                .selected_text(format_address(source.universe))
                .width(60.0)
                .show_ui(ui, |ui| {
                    for u in universes {
                        if ui.selectable_label(source.universe == *u, format_address(*u)).clicked() && source.universe != *u {
                            source.universe = *u;
                            changed = true;
                        }
                    }
                });
        }
        changed |= ui.add(egui::DragValue::new(&mut source.address).range(1..=512).prefix("ch ")).changed();
        let mut sixteen = source.fine.is_some();
        if ui.checkbox(&mut sixteen, "16 bit").on_hover_text("Add a fine channel").changed() {
            source.fine = sixteen.then(|| (source.address + 1).min(512));
            changed = true;
        }
        if let Some(fine) = &mut source.fine {
            changed |= ui.add(egui::DragValue::new(fine).range(1..=512).prefix("fine ch ")).changed();
        }
        let text = if learning { "Move it on the desk... (cancel)" } else { "Learn" };
        if ui.selectable_label(learning, text).on_hover_text("Then move the fader or press the button on the desk").clicked() {
            learn = true;
        }
        let more = ui.id().with(("dmx_more", salt));
        let mut open = ui.data(|d| d.get_temp::<bool>(more)).unwrap_or(false);
        if ui.selectable_label(open, egui_phosphor::regular::SLIDERS).on_hover_text("Invert and range").clicked() {
            open = !open;
            ui.data_mut(|d| d.insert_temp(more, open));
        }
        ui.add(egui::ProgressBar::new(value.clamp(0.0, 1.0)).desired_width(50.0).desired_height(6.0));
        if open {
            ui.end_row();
            ui.add_space(62.0);
            changed |= ui.checkbox(&mut source.invert, "invert").changed();
            ui.label("counts from");
            changed |= dmx_level(ui, &mut source.low, "");
            ui.label("to");
            changed |= dmx_level(ui, &mut source.high, "");
        }
    });
    (changed, learn)
}

impl NodeWidget for DmxInputWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "DMX Input" }
    fn description(&self) -> &'static str {
        "What a lighting desk sends us over Art-Net, as outputs. Add an Art-Net input in Setup, then add the outputs you need here in the inspector (a colour, a value, a gate, ...) and pick the channels each is made of, or press Learn and move the fader on the desk."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        output_ports(&self.config.outputs).iter().map(UiPortDef::from_def).collect()
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
        for (i, output) in self.config.outputs.iter().enumerate() {
            let value = |k: usize| shared.outputs.get(bases[i] + k).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            ui.horizontal(|ui| {
                ui.colored_label(theme::TEXT_DIM, output.port_name(i));
                preview(ui, output.kind, &value);
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
        ui.label(egui::RichText::new("Outputs").strong());
        let bases = self.bases();
        let live = self.source_values();
        let shared_outputs = self.shared.lock().unwrap().outputs.clone();
        let mut remove: Option<usize> = None;
        let mut learn: Option<(u32, usize)> = None;
        let learning = self.learning;

        for (i, o) in self.config.outputs.iter_mut().enumerate() {
            let state_id = ui.make_persistent_id(("dmx_out", o.id));
            let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), state_id, true);
            state
                .show_header(ui, |ui| {
                    let hint = o.port_name(i);
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut o.name).hint_text(hint).desired_width(100.0))
                        .changed();
                    ui.colored_label(theme::TEXT_DIM, o.kind.short());
                    let value = |k: usize| shared_outputs.get(bases[i] + k).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                    preview(ui, o.kind, &value);
                    if ui.small_button(egui_phosphor::regular::X).on_hover_text("Remove this output").clicked() {
                        remove = Some(i);
                    }
                })
                .body(|ui| {
                    match o.kind {
                        DmxKind::Color => {
                            ui.horizontal(|ui| {
                                for (m, label) in [(ColorModel::Rgb, "Red, green, blue"), (ColorModel::Hsv, "Hue, saturation, value")] {
                                    if ui.selectable_label(o.model == m, label).clicked() && o.model != m {
                                        o.model = m;
                                        changed = true;
                                    }
                                }
                                changed |= ui.checkbox(&mut o.dimmer, "with a dimmer").changed();
                            });
                            o.fit_sources();
                        }
                        DmxKind::Gate | DmxKind::Trigger => {
                            ui.horizontal_wrapped(|ui| {
                                changed |= dmx_level(ui, &mut o.on_above, "goes high above ");
                                changed |= dmx_level(ui, &mut o.off_below, if o.kind == DmxKind::Gate { "low below " } else { "ready again below " });
                            });
                        }
                        DmxKind::Index => {
                            ui.horizontal(|ui| {
                                changed |= ui.add(egui::DragValue::new(&mut o.bins).range(1..=256).prefix("ranges: ")).changed();
                                ui.colored_label(theme::TEXT_DIM, "the output is the number of the range, from 0");
                            });
                        }
                        DmxKind::Value | DmxKind::Position => {}
                    }
                    let labels = o.source_labels();
                    for (k, source) in o.sources.iter_mut().enumerate() {
                        let listening = learning.is_some_and(|l| l.output == o.id && l.source == k);
                        let value = live.get(i).and_then(|v| v.get(k)).copied().unwrap_or(0.0);
                        let (c, l) = source_row(ui, (o.id, k), labels.get(k).copied().unwrap_or(""), source, &universes, listening, value);
                        changed |= c;
                        if l {
                            learn = Some((o.id, k));
                        }
                    }
                });
        }
        if let Some((output, source)) = learn {
            self.learning = if learning.is_some_and(|l| l.output == output && l.source == source) {
                None
            } else {
                Some(Learning { output, source, since: self.hub.latest_seq(), first_seen: None })
            };
        }
        if let Some(i) = remove {
            self.remove_output(i);
            changed = false;
        }
        if changed {
            self.push_config();
        }

        if self.config.outputs.len() < MAX_OUTPUTS {
            let mut add: Option<DmxKind> = None;
            ui.menu_button(format!("{} Add output", egui_phosphor::regular::PLUS), |ui| {
                for kind in DmxKind::ALL {
                    if ui.button(kind.label()).clicked() {
                        add = Some(kind);
                        ui.close_menu();
                    }
                }
            });
            if let Some(kind) = add {
                self.add_output(kind);
            }
        }

        if self.config.outputs.iter().any(|o| !o.sources.is_empty()) {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Channels used, for patching the desk").strong());
            let text = patch_text(&self.config.outputs);
            ui.colored_label(theme::TEXT_DIM, &text);
            if ui.small_button("Copy").clicked() {
                ui.ctx().copy_text(text);
            }
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

/// A small picture of what an output reads now: a bar, a colour, a lamp.
fn preview(ui: &mut Ui, kind: DmxKind, value: &dyn Fn(usize) -> f32) {
    match kind {
        DmxKind::Color => {
            let (r, p) = ui.allocate_painter(Vec2::new(24.0, 8.0), Sense::hover());
            let rgb = |v: f32| (v * 255.0) as u8;
            p.rect_filled(r.rect, 1.0, egui::Color32::from_rgb(rgb(value(0)), rgb(value(1)), rgb(value(2))));
        }
        DmxKind::Gate | DmxKind::Trigger => {
            let (r, p) = ui.allocate_painter(Vec2::splat(10.0), Sense::hover());
            p.circle_filled(r.rect.center(), 4.0, if value(0) >= 0.5 { theme::SEM_SUCCESS } else { theme::STROKE });
        }
        DmxKind::Position => {
            let (r, p) = ui.allocate_painter(Vec2::splat(14.0), Sense::hover());
            p.rect_filled(r.rect, 1.0, theme::BG_DEEP);
            let at = egui::pos2(r.rect.min.x + value(0) * r.rect.width(), r.rect.min.y + value(1) * r.rect.height());
            p.circle_filled(at, 2.0, theme::SEM_PRIMARY);
        }
        DmxKind::Index => {
            ui.colored_label(theme::TEXT, format!("{:.0}", value(0) * 1.0));
        }
        DmxKind::Value => {
            ui.add(egui::ProgressBar::new(value(0)).desired_width(40.0).desired_height(6.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmx_input::{DmxInputConfig, DmxInputKind};
    use std::net::Ipv4Addr;

    fn change(universe: u16, channel: usize) -> Change {
        Change { seq: 1, universe, channel, value: 1 }
    }

    #[test]
    fn learning_picks_the_lowest_channel_of_the_first_universe_that_moved() {
        let universes = [4, 5];
        assert_eq!(learn_pick(&[], &universes), None);
        assert_eq!(learn_pick(&[change(4, 12), change(4, 11), change(4, 10)], &universes), Some((4, 10)));
        assert_eq!(learn_pick(&[change(5, 0), change(4, 300)], &[5, 4]), Some((5, 0)));
        assert_eq!(learn_pick(&[change(5, 7), change(4, 300)], &universes), Some((4, 300)));
    }

    #[test]
    fn the_next_free_channel_is_after_every_coarse_and_fine_channel_in_use() {
        let mut a = DmxOutput::new(1, DmxKind::Color, 0, 1);
        a.sources[2].fine = Some(40);
        let b = DmxOutput::new(2, DmxKind::Value, 1, 5);
        assert_eq!(next_free(&[a.clone(), b.clone()], 0), 41);
        assert_eq!(next_free(&[a.clone(), b], 1), 6);
        assert_eq!(next_free(&[a], 7), 1);
        let end = DmxOutput::new(3, DmxKind::Value, 0, 512);
        assert_eq!(next_free(&[end], 0), 512, "it stops at the end of the universe");
    }

    #[test]
    fn the_patch_text_lists_every_channel_in_order_for_the_desk() {
        let mut lamp = DmxOutput::new(1, DmxKind::Color, 0, 2);
        lamp.name = "Lamp".into();
        lamp.sources[1].fine = Some(9);
        let dim = DmxOutput::new(2, DmxKind::Value, 0, 1);
        let text = patch_text(&[lamp, dim]);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 5);
        assert!(lines[0].contains("channel 1:") && lines[0].contains("value 2"));
        assert!(lines[1].contains("channel 2:") && lines[1].contains("Lamp red"));
        assert!(lines[3].contains("channel 4:") && lines[3].contains("Lamp blue"));
        assert!(lines[4].contains("channel 9:") && lines[4].contains("green (fine)"));
    }

    fn widget() -> (DmxInputWidget, DmxInputHub) {
        let hub = DmxInputHub::idle();
        let mut input = DmxInputConfig::new_art_net(1, "Desk");
        input.kind = DmxInputKind::ArtNet { universes: vec![0, 1], interface: String::new() };
        hub.set_inputs(&[input]);
        (DmxInputWidget::new(NodeId(3), new_shared_state(0, 200), hub.clone()), hub)
    }

    #[test]
    fn adding_outputs_lays_their_channels_out_one_after_the_other() {
        let (mut w, _) = widget();
        w.restore_from_save_data(&serde_json::json!({ "input": 1, "outputs": [] }));
        w.add_output(DmxKind::Color);
        w.add_output(DmxKind::Gate);
        w.add_output(DmxKind::Position);
        let addresses: Vec<Vec<u16>> = w.config().outputs.iter().map(|o| o.sources.iter().map(|s| s.address).collect()).collect();
        assert_eq!(addresses, vec![vec![1, 2, 3], vec![4], vec![5, 6]]);
        let ports: Vec<(String, PortType)> = w.ui_outputs().into_iter().map(|p| (p.def.name, p.def.port_type)).collect();
        assert_eq!(ports[1].1, PortType::Color);
        assert_eq!(ports[2].1, PortType::Logic);
        assert_eq!(ports[3].1, PortType::Position);
        assert_eq!(w.bases(), vec![1, 4, 5], "a colour takes three output channels");
        assert!(w.shared.lock().unwrap().pending_config.is_some(), "the engine is told");
    }

    #[test]
    fn the_list_comes_back_from_the_saved_form_and_removal_names_the_port_that_went() {
        let (mut w, _) = widget();
        w.restore_from_save_data(&serde_json::json!({ "input": 1, "outputs": [
            {"id": 1, "name": "Dim", "kind": "value", "sources": [{"address": 1}]},
            {"id": 2, "kind": "color", "dimmer": true, "sources": [{"address": 2}]}]}));
        assert_eq!(w.config().outputs[1].sources.len(), 4, "missing sources are filled in");
        w.remove_output(0);
        assert_eq!(w.take_removed_port(), Some((PortDir::Output, 1)));
        assert_eq!(w.take_removed_port(), None);
        assert_eq!(w.ui_outputs().len(), 2);
    }

    #[test]
    fn a_moved_channel_is_learned_into_the_source_after_the_group_has_settled() {
        let (mut w, hub) = widget();
        w.restore_from_save_data(&serde_json::json!({ "input": 1, "outputs": [
            {"id": 1, "kind": "color", "sources": [{"address": 100}, {"address": 101}, {"address": 102}]}]}));
        let desk = Ipv4Addr::new(2, 0, 0, 1);
        hub.apply_dmx(1, &[0; 40], desk);
        w.learning = Some(Learning { output: 1, source: 1, since: hub.latest_seq(), first_seen: None });
        let ctx = egui::Context::default();
        let step = |w: &mut DmxInputWidget| {
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| w.learn_step(ui));
            });
        };
        let mut frame = [0u8; 40];
        frame[19] = 9;
        frame[20] = 9;
        hub.apply_dmx(1, &frame, desk);
        step(&mut w);
        assert!(w.learning.is_some(), "it waits for the other channels of the group");
        std::thread::sleep(LEARN_SETTLE + Duration::from_millis(50));
        step(&mut w);
        assert!(w.learning.is_none());
        let sources = &w.config().outputs[0].sources;
        assert_eq!((sources[1].universe, sources[1].address), (1, 20), "the lowest that moved, into the one learned");
        assert_eq!(sources[0].address, 100, "the others are not touched");
    }

    #[test]
    fn the_live_values_follow_each_sources_range_and_inversion() {
        let (mut w, hub) = widget();
        w.restore_from_save_data(&serde_json::json!({ "input": 1, "outputs": [
            {"id": 1, "kind": "value", "sources": [{"address": 1, "invert": true}]},
            {"id": 2, "kind": "position", "sources": [{"address": 2, "fine": 3}, {"address": 4}]}]}));
        hub.apply_dmx(0, &[255, 0xFF, 0xFF, 51], Ipv4Addr::new(2, 0, 0, 1));
        let v = w.source_values();
        assert!(v[0][0].abs() < 1e-6, "inverted full is empty");
        assert!((v[1][0] - 1.0).abs() < 1e-6 && (v[1][1] - 0.2).abs() < 1e-6);
    }
}
