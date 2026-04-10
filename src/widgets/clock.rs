use std::any::Any;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use egui::{self, Color32, Ui};

use crate::beat_clock::{BeatInfo, BeatListener, LinkSnapshot};
use crate::widgets::nodes::{NodeId, NodeWidget, PortDef, PortType};

const BEAT_FLASH_MS: u128 = 80;

/// Shared state updated by the beat clock thread (for beat flash + pending count).
pub struct ClockState {
    pub last_beat_time: Option<Instant>,
    pub last_beat_is_downbeat: bool,
    pub pending_beats: u32,
}

impl ClockState {
    pub fn new() -> Self {
        Self {
            last_beat_time: None,
            last_beat_is_downbeat: false,
            pending_beats: 0,
        }
    }
}

impl BeatListener for ClockState {
    fn on_beat(&mut self, info: &BeatInfo) {
        self.last_beat_time = Some(Instant::now());
        // Beat 0, 4, 8, ... are downbeats (quantum = 4).
        self.last_beat_is_downbeat = info.beat % 4 == 0;
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
    beat_output: f32,
    phase_output: f32,
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
            phase_output: 0.0,
        }
    }
}

impl NodeWidget for ClockNode {
    fn node_id(&self) -> NodeId {
        self.id
    }

    fn type_name(&self) -> &'static str {
        "Clock"
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
        let snap = self.snapshot.lock().unwrap();
        let mut cs = self.state.lock().unwrap();

        // Pulse beat output high for one frame per beat.
        if cs.pending_beats > 0 {
            cs.pending_beats = 0;
            self.beat_output = 1.0;
        } else {
            self.beat_output = 0.0;
        }

        // Only update phase while playing; freeze when stopped.
        if snap.playing {
            self.phase_output = snap.phase as f32;
        }
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.beat_output,
            1 => {
                let snap = self.snapshot.lock().unwrap();
                if snap.playing { 1.0 } else { 0.0 }
            }
            2 => self.phase_output,
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
                let beat_on = cs
                    .last_beat_time
                    .is_some_and(|t| t.elapsed().as_millis() < BEAT_FLASH_MS);
                let beat_color = if beat_on {
                    if cs.last_beat_is_downbeat {
                        Color32::from_rgb(255, 255, 255)
                    } else {
                        Color32::from_rgb(240, 200, 40)
                    }
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
