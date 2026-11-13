use egui::{self, Ui};

use crate::engine::nodes::ui::color_mixer::{ColorMixerDisplay, MixerMode, MIXER_MODE_NAMES};
use crate::engine::types::*;
use crate::widgets::nodes::node::{NodeLayout, NodeWidget, node_identity, with_display};
use crate::widgets::nodes::types::UiPortDef;

pub struct ColorMixerWidget {
    id: NodeId,
    shared: SharedState,
    name: String,
    mode: MixerMode,
    values: [f32; 3],
}

impl ColorMixerWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, name: String::new(), mode: MixerMode::Rgb, values: [1.0; 3] }
    }

    fn sync_from_display(&mut self) {
        with_display::<ColorMixerDisplay, _>(&self.shared, |d| {
            self.name = d.name.clone();
            self.mode = d.mode;
            self.values = d.values;
        });
    }

    fn push_config(&self) {
        let mut s = self.shared.lock().unwrap();
        s.pending_config = Some(serde_json::json!({
            "name": self.name,
            "mode": self.mode.to_index(),
            "values": self.values,
        }));
    }

    fn set_mode(&mut self, mode: MixerMode) {
        if mode == self.mode { return; }
        let c = self.mode.to_rgb(self.values);
        self.values = mode.from_rgb(c);
        self.mode = mode;
        self.push_config();
    }
}

impl NodeWidget for ColorMixerWidget {
    node_identity!("Color Mixer");
    fn title(&self) -> &str { &self.name }
    fn description(&self) -> &'static str {
        "Three sliders that mix a color in RGB, HSV or CMY. Switching mode keeps the color."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::ui::color_mixer::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::ui::color_mixer::ports().1) }

    fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
        self.push_config();
    }

    fn layout(&self) -> NodeLayout { NodeLayout::compact().with_side_inset(8.0) }
    fn min_width(&self) -> f32 { 150.0 }
    fn min_content_height(&self) -> f32 { 84.0 }
    fn resizable(&self) -> bool { true }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.sync_from_display();
        let names = self.mode.channel_names();
        let mut changed = false;
        ui.spacing_mut().slider_width = (ui.available_width() - 60.0).max(40.0);
        for (name, v) in names.iter().zip(self.values.iter_mut()) {
            changed |= ui
                .add(egui::Slider::new(v, 0.0..=1.0).text(*name).show_value(false))
                .changed();
        }
        let c = self.mode.to_rgb(self.values);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), ui.available_height().max(10.0)),
            egui::Sense::hover(),
        );
        let (r, g, b) = c.to_u8();
        ui.painter().rect_filled(rect, 3.0, egui::Color32::from_rgb(r, g, b));
        if changed {
            self.push_config();
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Name");
            if ui.text_edit_singleline(&mut self.name).changed() {
                self.push_config();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Mode");
            let mut mode = self.mode;
            egui::ComboBox::from_id_salt(("mixer_mode", self.id))
                .selected_text(MIXER_MODE_NAMES[mode.to_index()])
                .show_ui(ui, |ui| {
                    for (i, n) in MIXER_MODE_NAMES.iter().enumerate() {
                        ui.selectable_value(&mut mode, MixerMode::from_index(i), *n);
                    }
                });
            self.set_mode(mode);
        });
    }
}
