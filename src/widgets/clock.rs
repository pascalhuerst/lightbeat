use std::any::Any;

use egui::{self, Color32, Ui};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::beat_clock::{BeatInfo, BeatListener};
use crate::widgets::nodes::{NodeId, NodeWidget, PortDef, PortType};

const BEAT_FLASH_DURATION_MS: u128 = 80;

pub struct ClockState {
    pub tempo: f64,
    pub playing: bool,
    pub num_peers: usize,
    pub last_beat_time: Option<Instant>,
    /// Number of beats that arrived since last drain.
    pub pending_beats: u32,
}

impl ClockState {
    pub fn new() -> Self {
        Self {
            tempo: 0.0,
            playing: false,
            num_peers: 0,
            last_beat_time: None,
            pending_beats: 0,
        }
    }
}

impl BeatListener for ClockState {
    fn on_beat(&mut self, info: &BeatInfo) {
        self.tempo = info.tempo;
        self.last_beat_time = Some(Instant::now());
        self.pending_beats += 1;
    }

    fn on_transport_change(&mut self, playing: bool) {
        self.playing = playing;
        if !playing {
            self.last_beat_time = None;
        }
    }
}

pub struct ClockNode {
    id: NodeId,
    pub state: Arc<Mutex<ClockState>>,
    outputs: Vec<PortDef>,
}

impl ClockNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            state: Arc::new(Mutex::new(ClockState::new())),
            outputs: vec![
                PortDef::new("beat", PortType::Trigger),
                PortDef::new("play", PortType::Value),
            ],
        }
    }
}

impl NodeWidget for ClockNode {
    fn node_id(&self) -> NodeId {
        self.id
    }

    fn title(&self) -> &str {
        "Clock"
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

        ui.horizontal(|ui| {
            let link_color = if state.num_peers > 0 {
                Color32::from_rgb(80, 240, 120)
            } else {
                Color32::from_gray(100)
            };
            ui.colored_label(link_color, "LINK");
            ui.colored_label(Color32::from_gray(140), format!("{} peers", state.num_peers));
        });

        ui.add_space(pad);

        ui.vertical_centered(|ui| {
            ui.colored_label(Color32::WHITE, egui::RichText::new(format!("{:.1}", state.tempo)).monospace().size(20.0));
            ui.colored_label(Color32::from_gray(100), egui::RichText::new("BPM").monospace().size(9.0));
        });

        ui.add_space(pad);

        ui.horizontal(|ui| {
            let led_radius = 5.0;

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

    fn drain_trigger_outputs(&mut self) -> Vec<usize> {
        let mut state = self.state.lock().unwrap();
        let count = state.pending_beats;
        state.pending_beats = 0;
        if count > 0 {
            // Output port 0 = "beat"
            vec![0; count as usize]
        } else {
            vec![]
        }
    }

    fn read_value_output(&self, port_index: usize) -> f32 {
        match port_index {
            // Output port 1 = "play"
            1 => {
                let state = self.state.lock().unwrap();
                if state.playing { 1.0 } else { 0.0 }
            }
            _ => 0.0,
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
