use std::any::Any;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use egui::{self, Color32, Ui};

use crate::beat_clock::{BeatInfo, BeatListener, LinkSnapshot};
use crate::widgets::nodes::{NodeId, NodeWidget, PortDef, PortType};

const BEAT_FLASH_DURATION_MS: u128 = 80;

/// Shared state updated by the beat clock thread (for beat flash + pending count).
pub struct ClockState {
    pub last_beat_time: Option<Instant>,
    pub pending_beats: u32,
}

impl ClockState {
    pub fn new() -> Self {
        Self {
            last_beat_time: None,
            pending_beats: 0,
        }
    }
}

impl BeatListener for ClockState {
    fn on_beat(&mut self, _info: &BeatInfo) {
        self.last_beat_time = Some(Instant::now());
        self.pending_beats += 1;
    }

    fn on_transport_change(&mut self, playing: bool) {
        if !playing {
            self.last_beat_time = None;
        }
    }
}

pub struct ClockNode {
    id: NodeId,
    pub state: Arc<Mutex<ClockState>>,
    pub snapshot: Arc<Mutex<LinkSnapshot>>,
    outputs: Vec<PortDef>,
    /// Cached beat output: 1.0 while beat flash is active, 0.0 otherwise.
    beat_output: f32,
}

impl ClockNode {
    pub fn new(id: NodeId, snapshot: Arc<Mutex<LinkSnapshot>>) -> Self {
        Self {
            id,
            state: Arc::new(Mutex::new(ClockState::new())),
            snapshot,
            outputs: vec![
                PortDef::new("beat", PortType::Logic),
                PortDef::new("play", PortType::Logic),
                PortDef::new("phase", PortType::Phase),
            ],
            beat_output: 0.0,
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

    fn process(&mut self) {
        // Pulse beat output high for one frame per beat.
        let mut cs = self.state.lock().unwrap();
        if cs.pending_beats > 0 {
            cs.pending_beats = 0;
            self.beat_output = 1.0;
        } else {
            self.beat_output = 0.0;
        }
    }

    fn read_output(&self, port_index: usize) -> f32 {
        let snap = self.snapshot.lock().unwrap();
        match port_index {
            0 => self.beat_output,                          // beat (Logic)
            1 => if snap.playing { 1.0 } else { 0.0 },     // play (Logic)
            2 => snap.phase as f32,                         // phase (Phase)
            _ => 0.0,
        }
    }

    fn show_content(&mut self, ui: &mut Ui) {
        let snap = self.snapshot.lock().unwrap();
        let cs = self.state.lock().unwrap();

        let pad = 4.0;

        ui.horizontal(|ui| {
            let link_color = if snap.num_peers > 0 {
                Color32::from_rgb(80, 240, 120)
            } else {
                Color32::from_gray(100)
            };
            ui.colored_label(link_color, "LINK");
            ui.colored_label(Color32::from_gray(140), format!("{} peers", snap.num_peers));
        });

        ui.add_space(pad);

        ui.vertical_centered(|ui| {
            ui.colored_label(
                Color32::WHITE,
                egui::RichText::new(format!("{:.1}", snap.tempo))
                    .monospace()
                    .size(20.0),
            );
            ui.colored_label(
                Color32::from_gray(100),
                egui::RichText::new("BPM").monospace().size(9.0),
            );
        });

        ui.add_space(pad);

        ui.horizontal(|ui| {
            let led_radius = 5.0;

            let play_color = if snap.playing {
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
                if snap.playing { "PLAY" } else { "STOP" },
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let beat_on = cs.last_beat_time.is_some_and(|t| {
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

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
