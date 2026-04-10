use egui::{self, Color32, Sense, StrokeKind, Ui, Vec2};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::beat_clock::{BeatInfo, BeatListener};

const BEAT_FLASH_DURATION_MS: u128 = 80;

pub struct LinkStatusState {
    pub tempo: f64,
    pub playing: bool,
    pub num_peers: usize,
    pub last_beat_time: Option<Instant>,
}

impl LinkStatusState {
    pub fn new() -> Self {
        Self {
            tempo: 0.0,
            playing: false,
            num_peers: 0,
            last_beat_time: None,
        }
    }
}

impl BeatListener for LinkStatusState {
    fn on_beat(&mut self, info: &BeatInfo) {
        self.tempo = info.tempo;
        self.last_beat_time = Some(Instant::now());
    }

    fn on_transport_change(&mut self, playing: bool) {
        self.playing = playing;
        if !playing {
            self.last_beat_time = None;
        }
    }
}

pub struct LinkStatus {
    pub state: Arc<Mutex<LinkStatusState>>,
}

impl LinkStatus {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(LinkStatusState::new())),
        }
    }

    pub fn show(&self, ui: &mut Ui) {
        let state = self.state.lock().unwrap();

        let size = 120.0;
        let (response, painter) =
            ui.allocate_painter(Vec2::splat(size), Sense::hover());
        let rect = response.rect;

        // Background
        painter.rect_filled(rect, 4.0, Color32::from_gray(25));
        painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, Color32::from_gray(50)), StrokeKind::Inside);

        let pad = 8.0;

        // -- "LINK" label top-left --
        let link_color = if state.num_peers > 0 {
            Color32::from_rgb(80, 240, 120)
        } else {
            Color32::from_gray(100)
        };
        painter.text(
            egui::pos2(rect.min.x + pad, rect.min.y + pad),
            egui::Align2::LEFT_TOP,
            "LINK",
            egui::FontId::monospace(11.0),
            link_color,
        );

        // -- Peer count top-right --
        let peers_text = format!("{}", state.num_peers);
        painter.text(
            egui::pos2(rect.max.x - pad, rect.min.y + pad),
            egui::Align2::RIGHT_TOP,
            &peers_text,
            egui::FontId::monospace(11.0),
            Color32::from_gray(140),
        );

        // -- BPM center --
        let bpm_text = format!("{:.1}", state.tempo);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &bpm_text,
            egui::FontId::monospace(22.0),
            Color32::WHITE,
        );
        painter.text(
            egui::pos2(rect.center().x, rect.center().y + 16.0),
            egui::Align2::CENTER_TOP,
            "BPM",
            egui::FontId::monospace(9.0),
            Color32::from_gray(100),
        );

        // -- Play/Stop LED bottom-left --
        let led_radius = 5.0;
        let play_led_center = egui::pos2(rect.min.x + pad + led_radius, rect.max.y - pad - led_radius);
        let play_color = if state.playing {
            Color32::from_rgb(80, 240, 120)
        } else {
            Color32::from_gray(60)
        };
        painter.circle_filled(play_led_center, led_radius, play_color);
        painter.text(
            egui::pos2(play_led_center.x + led_radius + 4.0, play_led_center.y),
            egui::Align2::LEFT_CENTER,
            if state.playing { "PLAY" } else { "STOP" },
            egui::FontId::monospace(9.0),
            Color32::from_gray(140),
        );

        // -- Beat flash LED bottom-right --
        let beat_led_center = egui::pos2(rect.max.x - pad - led_radius, rect.max.y - pad - led_radius);
        let beat_on = state.last_beat_time.is_some_and(|t| {
            t.elapsed().as_millis() < BEAT_FLASH_DURATION_MS
        });
        let beat_color = if beat_on {
            Color32::from_rgb(240, 200, 40)
        } else {
            Color32::from_gray(40)
        };
        painter.circle_filled(beat_led_center, led_radius, beat_color);
    }
}
