use std::any::Any;

use egui::{self, Ui};

use crate::engine::nodes::io::midi_input::InMode;
use crate::engine::nodes::io::midi_output::{MidiOutputConfig, OutEntry, OutMode, input_ports};
use crate::engine::nodes::io::status::status_port;
use crate::engine::types::*;
use crate::midi_io::{MidiHub, MsgKind};
use crate::theme;
use crate::widgets::nodes::io::midi_common::{LearnedControl, MAX_ENTRIES, SharedLearned, device_picker, next_id};
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

/// Point an output at the control a MIDI Input has learned.
pub fn copy_from(entry: &mut OutEntry, control: &LearnedControl) {
    entry.binding = control.binding;
    entry.mode = match (control.binding.kind, control.mode) {
        (MsgKind::Note, InMode::Velocity) => OutMode::Velocity,
        (MsgKind::Note, _) => OutMode::Gate,
        _ => OutMode::Value,
    };
}

pub struct MidiOutputWidget {
    id: NodeId,
    shared: SharedState,
    hub: MidiHub,
    learned: SharedLearned,
    config: MidiOutputConfig,
    /// Set when an entry is removed, for the graph to move the wires after it.
    removed: Option<usize>,
}

impl MidiOutputWidget {
    pub fn new(id: NodeId, shared: SharedState, hub: MidiHub, learned: SharedLearned) -> Self {
        Self { id, shared, hub, learned, config: MidiOutputConfig::default(), removed: None }
    }

    #[cfg(test)]
    pub fn config(&self) -> &MidiOutputConfig {
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
        if let Ok(config) = serde_json::from_value::<MidiOutputConfig>(data.clone()) {
            self.config = config;
        }
    }

    /// Take entry `i` out of the list; the graph moves the wires after it.
    pub fn remove_entry(&mut self, i: usize) {
        self.config.entries.remove(i);
        self.removed = Some(i);
        self.push_config();
    }

    fn connected(&self) -> bool {
        self.shared.lock().unwrap().outputs.first().copied().unwrap_or(0.0) >= 0.5
    }
}

impl NodeWidget for MidiOutputWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "MIDI Output" }
    fn description(&self) -> &'static str {
        "Sends its inputs to a MIDI device. Add an input in the inspector and choose the message it sends: a CC, a note or the pitch wheel. Only wired inputs are sent, and only when they change. Copy from fills in a control that a MIDI Input has learned, for example to drive the motor of a fader you read."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        input_ports(&self.config.entries).iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&status_port())]
    }

    fn min_width(&self) -> f32 { 160.0 }
    fn min_content_height(&self) -> f32 { 24.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn take_removed_port(&mut self) -> Option<(PortDir, usize)> {
        self.removed.take().map(|i| (PortDir::Input, i))
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        if self.config.port.is_empty() {
            ui.colored_label(theme::TEXT_DIM, "No device. Pick one in the inspector.");
        } else if self.connected() {
            ui.colored_label(theme::SEM_SUCCESS, &self.config.port);
        } else {
            ui.colored_label(theme::SEM_WARNING, format!("{} (waiting)", self.config.port));
        }
        for entry in &self.config.entries {
            ui.colored_label(theme::TEXT_DIM, entry.binding.label());
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        self.hub.ensure_scanned();
        let available = self.hub.available_outputs();
        let mut port = self.config.port.clone();
        if device_picker(ui, ("midi_out_device", self.id.0), &mut port, &available, &self.hub) {
            self.config.port = port;
            self.push_config();
        }
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Inputs").strong());

        let learned: Vec<(String, LearnedControl)> = {
            let map = self.learned.lock().unwrap();
            let mut nodes: Vec<_> = map.iter().collect();
            nodes.sort_by_key(|(id, _)| **id);
            nodes
                .into_iter()
                .flat_map(|(_, (label, controls))| controls.iter().map(move |c| (label.clone(), c.clone())))
                .collect()
        };

        let mut changed = false;
        let mut remove: Option<usize> = None;
        for (i, entry) in self.config.entries.iter_mut().enumerate() {
            ui.horizontal_wrapped(|ui| {
                let hint = format!("in {}", i + 1);
                changed |= ui.add(egui::TextEdit::singleline(&mut entry.name).hint_text(hint).desired_width(90.0)).changed();
                egui::ComboBox::from_id_salt(("midi_out_kind", entry.id))
                    .selected_text(entry.binding.kind.label())
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        for kind in MsgKind::ALL {
                            if ui.selectable_label(entry.binding.kind == kind, kind.label()).clicked() && entry.binding.kind != kind {
                                entry.binding.kind = kind;
                                if kind == MsgKind::PitchBend {
                                    entry.binding.number = 0;
                                }
                                if !OutMode::modes_for(kind).contains(&entry.mode) {
                                    entry.mode = OutMode::default_for(kind);
                                }
                                changed = true;
                            }
                        }
                    });
                changed |= ui.add(egui::DragValue::new(&mut entry.binding.channel).range(1..=16).prefix("ch ")).changed();
                if entry.binding.kind != MsgKind::PitchBend {
                    changed |= ui.add(egui::DragValue::new(&mut entry.binding.number).range(0..=127).prefix("# ")).changed();
                }
                let modes = OutMode::modes_for(entry.binding.kind);
                if modes.len() > 1 {
                    egui::ComboBox::from_id_salt(("midi_out_mode", entry.id)).selected_text(entry.mode.label()).show_ui(ui, |ui| {
                        for m in modes {
                            if ui.selectable_label(entry.mode == *m, m.label()).clicked() {
                                entry.mode = *m;
                                changed = true;
                            }
                        }
                    });
                }
                if !learned.is_empty() {
                    egui::ComboBox::from_id_salt(("midi_out_copy", entry.id)).selected_text("Copy from...").show_ui(ui, |ui| {
                        for (label, control) in &learned {
                            if ui.selectable_label(false, format!("{label}: {} ({})", control.name, control.binding.label())).clicked() {
                                copy_from(entry, control);
                                changed = true;
                            }
                        }
                    });
                }
                if ui.small_button(egui_phosphor::regular::X).on_hover_text("Remove this input").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.remove_entry(i);
            changed = false;
        }
        if self.config.entries.len() < MAX_ENTRIES && ui.button(format!("{} Add input", egui_phosphor::regular::PLUS)).clicked() {
            let id = next_id(self.config.entries.iter().map(|e| e.id));
            self.config.entries.push(OutEntry::new(id));
            changed = true;
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
    use crate::midi_io::Binding;

    fn control(kind: MsgKind, number: u8, mode: InMode) -> LearnedControl {
        LearnedControl { name: "c".into(), binding: Binding { kind, channel: 3, number }, mode }
    }

    #[test]
    fn copying_a_learned_control_takes_its_message_and_a_fitting_mode() {
        let mut e = OutEntry::new(1);
        copy_from(&mut e, &control(MsgKind::Cc, 7, InMode::Relative));
        assert_eq!((e.binding.kind, e.binding.channel, e.binding.number, e.mode), (MsgKind::Cc, 3, 7, OutMode::Value));
        copy_from(&mut e, &control(MsgKind::Note, 36, InMode::TriggerPress));
        assert_eq!((e.binding.kind, e.binding.number, e.mode), (MsgKind::Note, 36, OutMode::Gate));
        copy_from(&mut e, &control(MsgKind::Note, 37, InMode::Velocity));
        assert_eq!(e.mode, OutMode::Velocity, "a pad that reads velocity is lit by velocity");
        copy_from(&mut e, &control(MsgKind::PitchBend, 0, InMode::Value));
        assert_eq!((e.binding.kind, e.mode), (MsgKind::PitchBend, OutMode::Value));
    }

    #[test]
    fn the_list_comes_back_from_the_saved_form_with_its_ports() {
        let mut w = MidiOutputWidget::new(
            NodeId(5),
            new_shared_state(32, 1),
            MidiHub::idle(),
            crate::widgets::nodes::io::midi_common::new_shared_learned(),
        );
        w.restore_from_save_data(&serde_json::json!({
            "port": "Dev",
            "entries": [{ "id": 1, "name": "Fader", "binding": {"kind": "cc", "channel": 1, "number": 5}, "mode": "value" },
                        { "id": 2, "binding": {"kind": "note", "channel": 1, "number": 60}, "mode": "gate" }]
        }));
        let names: Vec<String> = w.ui_inputs().into_iter().map(|p| p.def.name).collect();
        assert_eq!(names, ["Fader", "in 2"]);
        assert_eq!(w.ui_inputs()[1].def.port_type, PortType::Logic);
        assert_eq!(w.ui_outputs().len(), 1);
        assert_eq!(w.config().port, "Dev");
        w.removed = Some(0);
        assert_eq!(w.take_removed_port(), Some((PortDir::Input, 0)));
    }
}
