//! What the MIDI Input and MIDI Output nodes have in common: picking a device
//! and sharing what the inputs have learned with the outputs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::{self, Ui};

use crate::engine::nodes::io::midi_input::InMode;
use crate::input_controller::midi::without_address;
use crate::midi_io::{Binding, MidiHub};

/// The most inputs or outputs one node has.
pub const MAX_ENTRIES: usize = 32;

/// One control a MIDI Input node has learned.
#[derive(Debug, Clone, PartialEq)]
pub struct LearnedControl {
    pub name: String,
    pub binding: Binding,
    pub mode: InMode,
}

/// What each MIDI Input node has learned, by node id with its label, so a
/// MIDI Output can copy a control's message from it.
pub type SharedLearned = Arc<Mutex<HashMap<u64, (String, Vec<LearnedControl>)>>>;

pub fn new_shared_learned() -> SharedLearned {
    Arc::new(Mutex::new(HashMap::new()))
}

/// The device dropdown with its Rescan button. True when another device was picked.
pub fn device_picker(ui: &mut Ui, salt: impl std::hash::Hash, port: &mut String, available: &[String], hub: &MidiHub) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Device:");
        let shown = if port.is_empty() {
            "(none)".to_string()
        } else if available.iter().any(|a| a == port || without_address(a) == without_address(port)) {
            port.clone()
        } else {
            format!("{port} (not found)")
        };
        egui::ComboBox::from_id_salt(salt).selected_text(shown).width(180.0).show_ui(ui, |ui| {
            if ui.selectable_label(port.is_empty(), "(none)").clicked() && !port.is_empty() {
                port.clear();
                changed = true;
            }
            for name in available {
                let same = !port.is_empty() && (name == port || without_address(name) == without_address(port));
                if ui.selectable_label(same, name).clicked() && *port != *name {
                    port.clone_from(name);
                    changed = true;
                }
            }
        });
        if ui.small_button("Rescan").on_hover_text("Look for devices again").clicked() {
            hub.rescan();
        }
    });
    changed
}

/// The next id for a list whose entries have these ids.
pub fn next_id(ids: impl Iterator<Item = u32>) -> u32 {
    ids.max().map_or(1, |m| m + 1)
}
