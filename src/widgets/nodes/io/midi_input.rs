use std::any::Any;

use egui::{self, Ui};

use crate::engine::nodes::io::midi_input::{InEntry, InMode, MidiInputConfig, output_ports};
use crate::engine::types::*;
use crate::midi_io::{Binding, MidiHub, Msg};
use crate::theme;
use crate::widgets::nodes::io::midi_common::{LearnedControl, MAX_ENTRIES, SharedLearned, device_picker, next_id};
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

/// An entry waiting for the next control to be moved.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Learning {
    entry: u32,
    /// Messages up to this number came before and do not count.
    since: u64,
}

/// What a message teaches an entry.
#[derive(Debug, Clone, PartialEq)]
pub enum Learned {
    /// Nothing usable yet.
    Nothing,
    Bound(Binding),
    /// The control is already another entry's; the name of that entry.
    Taken(String),
}

/// The first control among `messages` that can be learned: a CC, a note being
/// pressed or the pitch wheel. Releases are ignored, so pressing a button
/// teaches the press and not the release that follows. A control that another
/// entry already has is refused.
pub fn learn_from(messages: &[Msg], entries: &[InEntry], learning: u32) -> Learned {
    for msg in messages {
        if matches!(msg, Msg::NoteOff { .. }) {
            continue;
        }
        let binding = msg.binding();
        if let Some(other) = entries.iter().find(|e| e.id != learning && e.binding == Some(binding)) {
            return Learned::Taken(other.name.clone());
        }
        return Learned::Bound(binding);
    }
    Learned::Nothing
}

pub struct MidiInputWidget {
    id: NodeId,
    shared: SharedState,
    hub: MidiHub,
    learned: SharedLearned,
    config: MidiInputConfig,
    learning: Option<Learning>,
    /// A line to show under the list, for example why a control was refused.
    notice: Option<String>,
    /// Set when an entry is removed, for the graph to move the wires after it.
    removed: Option<usize>,
}

impl MidiInputWidget {
    pub fn new(id: NodeId, shared: SharedState, hub: MidiHub, learned: SharedLearned) -> Self {
        Self { id, shared, hub, learned, config: MidiInputConfig::default(), learning: None, notice: None, removed: None }
    }

    #[cfg(test)]
    pub fn config(&self) -> &MidiInputConfig {
        &self.config
    }

    fn push_config(&self) {
        if let Ok(value) = serde_json::to_value(&self.config) {
            self.shared.lock().unwrap().pending_config = Some(value);
        }
        self.publish();
    }

    /// Tell the MIDI Output nodes what this one has learned.
    fn publish(&self) {
        let label = if self.config.port.is_empty() { "MIDI Input".to_string() } else { format!("MIDI Input ({})", self.config.port) };
        let controls = self
            .config
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                e.binding.map(|binding| LearnedControl { name: e.port_name(i), binding, mode: e.mode })
            })
            .collect();
        self.learned.lock().unwrap().insert(self.id.0, (label, controls));
    }

    /// Restore the saved list before wires are attached, so wires to its
    /// ports still have ports to land on.
    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Ok(config) = serde_json::from_value::<MidiInputConfig>(data.clone()) {
            self.config = config;
            self.publish();
        }
    }

    fn set_binding(&mut self, entry: u32, binding: Binding) {
        if let Some(e) = self.config.entries.iter_mut().find(|e| e.id == entry) {
            e.binding = Some(binding);
            if !InMode::modes_for(binding.kind).contains(&e.mode) {
                e.mode = InMode::modes_for(binding.kind)[0];
            }
        }
    }

    /// Look at what arrived since learning started.
    fn learn_step(&mut self, ui: &Ui) {
        let Some(l) = self.learning else { return };
        let (messages, latest) = self.hub.messages_since(&self.config.port, l.since);
        match learn_from(&messages, &self.config.entries, l.entry) {
            Learned::Nothing => {
                self.learning = Some(Learning { since: latest, ..l });
            }
            Learned::Bound(binding) => {
                self.set_binding(l.entry, binding);
                self.learning = None;
                self.notice = None;
                self.push_config();
            }
            Learned::Taken(other) => {
                let who = if other.is_empty() { "another output".to_string() } else { other };
                self.notice = Some(format!("That control is already {who}'s. Move another one."));
                self.learning = Some(Learning { since: latest, ..l });
            }
        }
        // New messages arrive without any input to this window.
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
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

    fn value_of(&self, index: usize) -> f32 {
        self.shared.lock().unwrap().outputs.get(index + 1).copied().unwrap_or(0.0)
    }

    fn connected(&self) -> bool {
        self.shared.lock().unwrap().outputs.first().copied().unwrap_or(0.0) >= 0.5
    }
}

impl Drop for MidiInputWidget {
    fn drop(&mut self) {
        self.learned.lock().unwrap().remove(&self.id.0);
    }
}

impl NodeWidget for MidiInputWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "MIDI Input" }
    fn description(&self) -> &'static str {
        "The controls of a MIDI device as outputs. Add an output in the inspector, press Learn and move a fader or press a button on the device to bind it. A fader or knob gives a value, a button a gate or a pulse, an endless encoder a value it adds up."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        output_ports(&self.config.entries).iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 160.0 }
    fn min_content_height(&self) -> f32 { 24.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn take_removed_port(&mut self) -> Option<(PortDir, usize)> {
        self.removed.take().map(|i| (PortDir::Output, i + 1))
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.learn_step(ui);
        if self.config.port.is_empty() {
            ui.colored_label(theme::TEXT_DIM, "No device. Pick one in the inspector.");
        } else if self.connected() {
            ui.colored_label(theme::SEM_SUCCESS, &self.config.port);
        } else {
            ui.colored_label(theme::SEM_WARNING, format!("{} (waiting)", self.config.port));
        }
        for (i, entry) in self.config.entries.iter().enumerate() {
            ui.horizontal(|ui| {
                let learning = self.learning.is_some_and(|l| l.entry == entry.id);
                let (text, color) = match (entry.binding, learning) {
                    (_, true) => ("listening...".to_string(), theme::SEM_PRIMARY),
                    (Some(b), _) => (b.label(), theme::TEXT_DIM),
                    (None, _) => ("not learned".to_string(), theme::SEM_WARNING),
                };
                ui.colored_label(color, text);
                ui.add(egui::ProgressBar::new(self.value_of(i).clamp(0.0, 1.0)).desired_width(40.0).desired_height(6.0));
            });
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        self.hub.ensure_scanned();
        self.learn_step(ui);
        let available = self.hub.available_inputs();
        let mut port = self.config.port.clone();
        if device_picker(ui, ("midi_in_device", self.id.0), &mut port, &available, &self.hub) {
            self.config.port = port;
            self.learning = None;
            self.push_config();
        }
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Outputs").strong());

        let mut changed = false;
        let mut remove: Option<usize> = None;
        let mut learn: Option<u32> = None;
        for (i, entry) in self.config.entries.iter_mut().enumerate() {
            ui.horizontal_wrapped(|ui| {
                let hint = format!("out {}", i + 1);
                changed |= ui.add(egui::TextEdit::singleline(&mut entry.name).hint_text(hint).desired_width(90.0)).changed();
                let listening = self.learning.is_some_and(|l| l.entry == entry.id);
                let label = if listening { "Listening... (cancel)" } else { "Learn" };
                if ui.selectable_label(listening, label).on_hover_text("Then move a control on the device").clicked() {
                    learn = Some(entry.id);
                }
                match entry.binding {
                    Some(b) => {
                        ui.colored_label(theme::TEXT, b.label());
                        let modes = InMode::modes_for(b.kind);
                        if modes.len() > 1 {
                            egui::ComboBox::from_id_salt(("midi_in_mode", entry.id))
                                .selected_text(entry.mode.label())
                                .show_ui(ui, |ui| {
                                    for m in modes {
                                        if ui.selectable_label(entry.mode == *m, m.label()).clicked() {
                                            entry.mode = *m;
                                            changed = true;
                                        }
                                    }
                                });
                        }
                    }
                    None => {
                        ui.colored_label(theme::SEM_WARNING, "not learned");
                    }
                }
                if ui.small_button(egui_phosphor::regular::X).on_hover_text("Remove this output").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(entry) = learn {
            if self.learning.is_some_and(|l| l.entry == entry) {
                self.learning = None;
            } else {
                self.learning = Some(Learning { entry, since: self.hub.latest_seq(&self.config.port) });
                self.notice = None;
            }
        }
        if let Some(i) = remove {
            self.remove_entry(i);
            changed = false;
        }
        if self.config.entries.len() < MAX_ENTRIES && ui.button(format!("{} Add output", egui_phosphor::regular::PLUS)).clicked() {
            let id = next_id(self.config.entries.iter().map(|e| e.id));
            self.config.entries.push(InEntry { id, name: String::new(), binding: None, mode: InMode::Value });
            changed = true;
        }
        if let Some(notice) = &self.notice {
            ui.colored_label(theme::SEM_WARNING, notice);
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
    use crate::midi_io::MsgKind;

    const PORT: &str = "Controller";

    fn widget() -> (MidiInputWidget, MidiHub, SharedLearned) {
        let hub = MidiHub::idle();
        let learned = crate::widgets::nodes::io::midi_common::new_shared_learned();
        (MidiInputWidget::new(NodeId(4), new_shared_state(0, 33), hub.clone(), learned.clone()), hub, learned)
    }

    fn entries() -> Vec<InEntry> {
        vec![
            InEntry { id: 1, name: "Bass".into(), binding: Some(Binding { kind: MsgKind::Cc, channel: 1, number: 7 }), mode: InMode::Value },
            InEntry { id: 2, name: "Hat".into(), binding: None, mode: InMode::Value },
        ]
    }

    #[test]
    fn the_first_control_that_moves_is_learned_and_a_release_is_not() {
        let e = entries();
        assert_eq!(learn_from(&[], &e, 2), Learned::Nothing);
        let release = Msg::NoteOff { channel: 1, note: 60 };
        let press = Msg::NoteOn { channel: 1, note: 60, velocity: 90 };
        assert_eq!(learn_from(&[release], &e, 2), Learned::Nothing);
        assert_eq!(
            learn_from(&[release, press], &e, 2),
            Learned::Bound(Binding { kind: MsgKind::Note, channel: 1, number: 60 })
        );
        assert_eq!(
            learn_from(&[Msg::Bend { channel: 3, value: 100 }], &e, 2),
            Learned::Bound(Binding { kind: MsgKind::PitchBend, channel: 3, number: 0 })
        );
    }

    #[test]
    fn a_control_another_entry_has_is_refused_but_the_entry_may_learn_its_own_again() {
        let e = entries();
        let bass = Msg::Cc { channel: 1, cc: 7, value: 3 };
        assert_eq!(learn_from(&[bass], &e, 2), Learned::Taken("Bass".into()));
        assert_eq!(
            learn_from(&[bass], &e, 1),
            Learned::Bound(Binding { kind: MsgKind::Cc, channel: 1, number: 7 })
        );
    }

    #[test]
    fn learning_binds_the_entry_picks_a_fitting_mode_and_tells_the_outputs() {
        let (mut w, hub, learned) = widget();
        w.config.port = PORT.into();
        w.config.entries = entries();
        w.learning = Some(Learning { entry: 2, since: hub.latest_seq(PORT) });
        hub.inject(PORT, Msg::NoteOn { channel: 2, note: 36, velocity: 100 });
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| w.learn_step(ui));
        });
        assert_eq!(w.learning, None);
        let hat = &w.config.entries[1];
        assert_eq!(hat.binding, Some(Binding { kind: MsgKind::Note, channel: 2, number: 36 }));
        assert_eq!(hat.mode, InMode::Gate, "a value does not fit a note, so it becomes a gate");
        let published = learned.lock().unwrap();
        let (_, controls) = published.get(&4).unwrap();
        assert_eq!(controls.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["Bass", "Hat"]);
    }

    #[test]
    fn the_list_comes_back_from_the_saved_form_and_is_forgotten_with_the_widget() {
        let (mut w, _hub, learned) = widget();
        w.restore_from_save_data(&serde_json::json!({ "port": PORT, "entries": [{ "id": 1, "name": "A", "binding": {"kind": "cc", "channel": 1, "number": 7}, "mode": "relative" }] }));
        assert_eq!(w.ui_outputs().len(), 2, "status port and one entry");
        assert_eq!(w.config().entries[0].mode, InMode::Relative);
        assert_eq!(learned.lock().unwrap().len(), 1);
        drop(w);
        assert!(learned.lock().unwrap().is_empty());
    }

    #[test]
    fn removing_an_entry_names_the_output_port_that_went() {
        let (mut w, _, _) = widget();
        w.removed = Some(1);
        assert_eq!(w.take_removed_port(), Some((PortDir::Output, 2)));
        assert_eq!(w.take_removed_port(), None);
    }
}
