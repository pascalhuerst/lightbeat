use egui::{self, Color32, Ui};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::beat_clock::{BeatInfo, BeatListener};
use crate::widgets::nodes::{NodeId, NodeWidget, PortDef, PortType};

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

pub struct LinkStatusNode {
    id: NodeId,
    pub state: Arc<Mutex<LinkStatusState>>,
    outputs: Vec<PortDef>,
}

impl LinkStatusNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            state: Arc::new(Mutex::new(LinkStatusState::new())),
            outputs: vec![
                PortDef::new("beat", PortType::Trigger),
                PortDef::new("play", PortType::Value),
            ],
        }
    }
}

impl NodeWidget for LinkStatusNode {
    fn node_id(&self) -> NodeId {
        self.id
    }

    fn title(&self) -> &str {
        "Ableton Link"
    }

    fn inputs(&self) -> &[PortDef] {
        &[]
    }

    fn outputs(&self) -> &[PortDef] {
        &self.outputs
    }

    fn min_width(&self) -> f32 {
        150.0
    }

    fn min_content_height(&self) -> f32 {
        90.0
    }

    fn show_content(&mut self, ui: &mut Ui) {
        let state = self.state.lock().unwrap();

        let pad = 4.0;

        // -- LINK + peers row --
        ui.horizontal(|ui| {
            let link_color = if state.num_peers > 0 {
                Color32::from_rgb(80, 240, 120)
            } else {
                Color32::from_gray(100)
            };
            ui.colored_label(link_color, "LINK");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.colored_label(Color32::from_gray(140), format!("{} peers", state.num_peers));
            });
        });

        ui.add_space(pad);

        // -- BPM --
        ui.vertical_centered(|ui| {
            ui.colored_label(Color32::WHITE, egui::RichText::new(format!("{:.1}", state.tempo)).monospace().size(20.0));
            ui.colored_label(Color32::from_gray(100), egui::RichText::new("BPM").monospace().size(9.0));
        });

        ui.add_space(pad);

        // -- Play/Stop LED + Beat flash --
        ui.horizontal(|ui| {
            let led_radius = 5.0;

            // Play LED
            let play_color = if state.playing {
                Color32::from_rgb(80, 240, 120)
            } else {
                Color32::from_gray(60)
            };
            let (play_resp, play_painter) = ui.allocate_painter(
                egui::Vec2::new(led_radius * 2.0, led_radius * 2.0),
                egui::Sense::hover(),
            );
            play_painter.circle_filled(play_resp.rect.center(), led_radius, play_color);

            ui.colored_label(
                Color32::from_gray(140),
                if state.playing { "PLAY" } else { "STOP" },
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Beat flash LED
                let beat_on = state.last_beat_time.is_some_and(|t| {
                    t.elapsed().as_millis() < BEAT_FLASH_DURATION_MS
                });
                let beat_color = if beat_on {
                    Color32::from_rgb(240, 200, 40)
                } else {
                    Color32::from_gray(40)
                };
                let (beat_resp, beat_painter) = ui.allocate_painter(
                    egui::Vec2::new(led_radius * 2.0, led_radius * 2.0),
                    egui::Sense::hover(),
                );
                beat_painter.circle_filled(beat_resp.rect.center(), led_radius, beat_color);
            });
        });
    }
}
