use egui::{Ui, Vec2};

use crate::objects::gradient_preset::GradientPreset;
use crate::theme;
use crate::widgets::gradient_editor;
use crate::widgets::setup_ui::{self, ItemHeader};

pub struct GradientPresetManager {
    pub presets: Vec<GradientPreset>,
    next_id: u32,
}

impl GradientPresetManager {
    pub fn new() -> Self {
        Self { presets: Vec::new(), next_id: 1 }
    }

    pub fn from_presets(presets: Vec<GradientPreset>) -> Self {
        let next_id = presets.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        Self { presets, next_id }
    }

    /// Reserve and return a fresh id for a preset added externally
    /// (e.g. via "Save current as preset" in the Gradient Source widget).
    pub fn next_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// A copy right after the original, with a fresh id.
    fn duplicate(&mut self, id: u32) {
        let Some(pos) = self.presets.iter().position(|p| p.id == id) else { return };
        let mut copy = self.presets[pos].clone();
        copy.id = self.next_id();
        copy.name = setup_ui::copy_name(&copy.name);
        self.presets.insert(pos + 1, copy);
    }

    pub fn show(&mut self, ui: &mut Ui) {
        ui.colored_label(
            theme::TEXT_DIM,
            "Reusable gradients. Edit the stops here, or load one into a Gradient Source node and use \"Save preset\" or \"Update preset\" there.",
        );
        ui.separator();

        if self.presets.is_empty() {
            setup_ui::empty_state(ui, "No gradients yet. Add one below.");
        }

        let open = setup_ui::open_by_default(self.presets.len());
        let mut remove_id = None;
        let mut duplicate_id = None;
        for preset in &mut self.presets {
            let title = preset.name.clone();
            let mut header = ItemHeader::new(&title, "Delete gradient");
            header.duplicate = Some("Duplicate gradient");
            let actions = setup_ui::item_header(ui, preset.id, open, header, |ui| {
                setup_ui::name_field(ui, &mut preset.name);
                gradient_editor::draw_strip(ui, Vec2::new(ui.available_width().min(360.0), 22.0), &preset.stops);
                ui.add_space(4.0);
                gradient_editor::show_stops(ui, &mut preset.stops);
                ui.add_space(4.0);
            });
            if actions.delete {
                remove_id = Some(preset.id);
            }
            if actions.duplicate {
                duplicate_id = Some(preset.id);
            }
        }
        if let Some(id) = remove_id {
            self.presets.retain(|p| p.id != id);
        }
        if let Some(id) = duplicate_id {
            self.duplicate(id);
        }

        ui.separator();
        if setup_ui::add_button(ui, "Add gradient") {
            let id = self.next_id();
            self.presets.push(GradientPreset::new(id, format!("Gradient {}", id)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_gets_a_fresh_id_and_the_same_stops() {
        let mut m = GradientPresetManager::from_presets(vec![GradientPreset::new(7, "Fire")]);
        m.duplicate(7);
        assert_eq!(m.presets[1].id, 8);
        assert_eq!(m.presets[1].name, "Fire copy");
        assert_eq!(m.presets[1].stops.len(), m.presets[0].stops.len());
        assert_eq!(m.next_id(), 9);
    }
}
