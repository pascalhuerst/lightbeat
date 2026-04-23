use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::io::clock::ClockDisplay;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

#[allow(dead_code)]
const BEAT_FLASH_MS: u128 = 80;

pub struct ClockWidget {
    id: NodeId,
    shared: SharedState,
    outputs: Vec<PortDef>,
}

impl ClockWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            outputs: vec![
                PortDef::new("beat", PortType::Logic),
                PortDef::new("play", PortType::Logic),
                PortDef::new("phase", PortType::Phase),
            ],
        }
    }
}

impl NodeWidget for ClockWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Clock" }
    fn title(&self) -> &str { "Clock" }
    fn description(&self) -> &'static str { "Master clock synced to Ableton Link peers; outputs beat, play state, and phase." }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.outputs.iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 150.0 }
    fn min_content_height(&self) -> f32 { 90.0 }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<ClockDisplay>());

        let (tempo, playing, num_peers, beat_on, is_downbeat) = if let Some(d) = display {
            let beat_on = d.last_beat_time.is_some_and(|t| t.elapsed().as_millis() < BEAT_FLASH_MS);
            (d.tempo, d.playing, d.num_peers, beat_on, d.last_beat_is_downbeat)
        } else {
            (0.0, false, 0, false, false)
        };
        drop(shared);

        let pad = 4.0 * zoom;

        ui.horizontal(|ui| {
            let link_color = if num_peers > 0 {
                Color32::from_rgb(80, 240, 120)
            } else {
                Color32::from_gray(100)
            };
            ui.colored_label(link_color, "LINK");
            ui.colored_label(Color32::from_gray(140), format!("{} peers", num_peers));
        });

        ui.add_space(pad);

        ui.vertical_centered(|ui| {
            ui.colored_label(
                Color32::WHITE,
                egui::RichText::new(format!("{:.1}", tempo)).monospace().size(20.0 * zoom),
            );
            ui.colored_label(
                Color32::from_gray(100),
                egui::RichText::new("BPM").monospace().size(9.0 * zoom),
            );
        });

        ui.add_space(pad);

        // Play/beat LEDs row — use painter for precise control at all zoom levels.
        let row_h = (12.0 * zoom).max(4.0);
        let (row_resp, row_painter) = ui.allocate_painter(
            egui::Vec2::new(ui.available_width(), row_h),
            egui::Sense::hover(),
        );
        let row_rect = row_resp.rect;
        let led_r = (row_h * 0.4).max(2.0);

        // Play LED (left).
        let play_color = if playing { Color32::from_rgb(80, 240, 120) } else { Color32::from_gray(60) };
        let play_center = egui::pos2(row_rect.min.x + led_r + 2.0, row_rect.center().y);
        row_painter.circle_filled(play_center, led_r, play_color);

        // Play/Stop text.
        let text_x = play_center.x + led_r + 3.0;
        row_painter.text(
            egui::pos2(text_x, row_rect.center().y),
            egui::Align2::LEFT_CENTER,
            if playing { "PLAY" } else { "STOP" },
            egui::FontId::monospace(9.0 * zoom),
            Color32::from_gray(140),
        );

        // Beat LED (right).
        let beat_color = if beat_on {
            if is_downbeat { Color32::from_rgb(255, 255, 255) }
            else { theme::TYPE_LOGIC }
        } else {
            Color32::from_gray(40)
        };
        let beat_center = egui::pos2(row_rect.max.x - led_r - 2.0, row_rect.center().y);
        row_painter.circle_filled(beat_center, led_r, beat_color);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
