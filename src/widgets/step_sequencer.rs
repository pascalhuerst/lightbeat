use egui::{self, Color32, Rect, Sense, StrokeKind, Ui, Vec2};
use std::sync::{Arc, Mutex};

use crate::beat_clock::{BeatInfo, BeatListener};

const DEFAULT_STEPS: usize = 8;

/// Inner state shared between the UI and the beat clock thread.
pub struct StepSequencerState {
    /// Value per step, 0.0..=1.0
    pub values: Vec<f32>,
    /// Which step is currently active (-1 or wrapping index)
    pub current_step: usize,
    /// Whether the sequencer is running
    pub playing: bool,
}

impl StepSequencerState {
    pub fn new(num_steps: usize) -> Self {
        Self {
            values: vec![0.0; num_steps],
            current_step: 0,
            playing: false,
        }
    }

    fn num_steps(&self) -> usize {
        self.values.len()
    }

    fn advance(&mut self) {
        self.current_step = (self.current_step + 1) % self.num_steps();
        let value = self.values[self.current_step];
        println!(
            "step {} -> trigger (value: {:.2})",
            self.current_step, value
        );
    }
}

impl BeatListener for StepSequencerState {
    fn on_beat(&mut self, _info: &BeatInfo) {
        self.advance();
    }

    fn on_transport_change(&mut self, playing: bool) {
        self.playing = playing;
        if playing {
            self.current_step = 0;
        }
    }
}

/// Step sequencer UI widget. Holds an Arc to shared state so the beat
/// clock thread can advance the step while the UI renders.
pub struct StepSequencer {
    pub state: Arc<Mutex<StepSequencerState>>,
}

impl StepSequencer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(StepSequencerState::new(DEFAULT_STEPS))),
        }
    }

    pub fn show(&self, ui: &mut Ui) {
        let mut state = self.state.lock().unwrap();
        let num_steps = state.num_steps();

        let available_width = ui.available_width();
        let step_width = available_width / num_steps as f32;
        let height = 200.0;

        let (response, painter) =
            ui.allocate_painter(Vec2::new(available_width, height), Sense::click_and_drag());
        let rect = response.rect;

        let bg_color = Color32::from_gray(30);
        let fill_color = Color32::from_rgb(80, 180, 240);
        let active_fill = Color32::from_rgb(240, 160, 40);
        let line_color = Color32::from_gray(60);

        // Background
        painter.rect_filled(rect, 2.0, bg_color);

        // Draw each step
        for i in 0..num_steps {
            let x_min = rect.min.x + i as f32 * step_width;
            let x_max = x_min + step_width;
            let step_rect = Rect::from_min_max(
                egui::pos2(x_min, rect.min.y),
                egui::pos2(x_max, rect.max.y),
            );

            // Filled portion (from bottom)
            let fill_height = state.values[i] * height;
            let fill_rect = Rect::from_min_max(
                egui::pos2(x_min, rect.max.y - fill_height),
                egui::pos2(x_max, rect.max.y),
            );

            let color = if i == state.current_step && state.playing {
                active_fill
            } else {
                fill_color
            };
            painter.rect_filled(fill_rect, 0.0, color);

            // Divider line (except after last)
            if i < num_steps - 1 {
                painter.line_segment(
                    [egui::pos2(x_max, rect.min.y), egui::pos2(x_max, rect.max.y)],
                    egui::Stroke::new(1.0, line_color),
                );
            }

        }

        // Outline
        painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, line_color), StrokeKind::Inside);

        // Active step highlight border (drawn last so it's on top of everything)
        if state.playing {
            let i = state.current_step;
            let x_min = rect.min.x + i as f32 * step_width;
            let x_max = x_min + step_width;
            let step_rect = Rect::from_min_max(
                egui::pos2(x_min, rect.min.y),
                egui::pos2(x_max, rect.max.y),
            );
            painter.rect_stroke(step_rect, 0.0, egui::Stroke::new(2.0, active_fill), StrokeKind::Inside);
        }

        // Handle drag interaction
        if response.dragged() || response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if rect.contains(pos) {
                    let step_index =
                        ((pos.x - rect.min.x) / step_width).floor() as usize;
                    let step_index = step_index.min(num_steps - 1);
                    let value = 1.0 - ((pos.y - rect.min.y) / height).clamp(0.0, 1.0);
                    state.values[step_index] = value;
                }
            }
        }
    }
}
