use std::any::Any;

use egui::{self, Color32, Rect, Sense, StrokeKind, Ui, Vec2};

use crate::engine::nodes::transport::step_sequencer::{StepMode, StepSequencerDisplay};
use crate::engine::types::*;
use crate::widgets::fader::{self, FaderStyle, Orientation};
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

/// Custom shared data for step value edits from UI.
#[allow(dead_code)]
pub struct StepValueEdits {
    pub edits: Vec<(usize, f32)>,
}

pub struct StepSequencerWidget {
    id: NodeId,
    shared: SharedState,
    /// Mirror of engine mode — drives `ui_inputs` so the input port matches.
    mode: StepMode,
    outputs: Vec<PortDef>,
}

impl StepSequencerWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            mode: StepMode::Phase,
            outputs: vec![
                PortDef::new("trigger", PortType::Logic),
                PortDef::new("value", PortType::Untyped),
                PortDef::new("step", PortType::Untyped),
            ],
        }
    }
}

impl NodeWidget for StepSequencerWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Step Sequencer" }
    fn description(&self) -> &'static str { "N-step pattern advanced by phase; outputs current step value, index, and per-step trigger." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        match self.mode {
            StepMode::Phase => vec![
                UiPortDef::from_def(&PortDef::new("phase", PortType::Phase)),
            ],
            StepMode::Trigger => vec![
                UiPortDef::from_def(&PortDef::new("trigger", PortType::Logic)),
                UiPortDef::from_def(&PortDef::new("reset", PortType::Logic)),
            ],
        }
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.outputs.iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 160.0 }
    fn min_content_height(&self) -> f32 { 80.0 }
    fn resizable(&self) -> bool { true }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<StepSequencerDisplay>());

        let (values, current_step, active, mode) = if let Some(d) = display {
            (d.values.clone(), d.current_step, d.active, d.mode)
        } else {
            (vec![0.0; 8], 0, false, StepMode::Phase)
        };
        drop(shared);
        self.mode = mode;

        let num_steps = values.len();
        let available_width = ui.available_width();
        let step_width = available_width / num_steps as f32;
        let height = ui.available_height().max(60.0);

        let (response, painter) =
            ui.allocate_painter(Vec2::new(available_width, height), Sense::click_and_drag());
        let rect = response.rect;

        let style = FaderStyle {
            // Border off; we paint a single outer stroke below to avoid double-draws.
            border: None,
            ..FaderStyle::default()
        };
        let active_fill = style.fill_active;
        let line_color = Color32::from_gray(90);

        // Use rounded pixel-aligned x boundaries so separator lines render
        // crisply for any step count (sub-pixel `line_segment` lines were
        // disappearing for some columns at large step counts).
        let x_at = |i: usize| -> f32 {
            (rect.min.x + i as f32 * step_width).round()
        };

        for i in 0..num_steps {
            let x_min = x_at(i);
            let x_max = x_at(i + 1);
            let cell = Rect::from_min_max(
                egui::pos2(x_min, rect.min.y),
                egui::pos2(x_max, rect.max.y),
            );
            let is_active = i == current_step && active;
            fader::draw_fader(&painter, cell, values[i], Orientation::Vertical, &style, is_active);
        }
        // Draw separator lines as 1px-wide filled rects at integer x —
        // independent of cell drawing so no separator can be hidden by a
        // bar fill, and they're guaranteed pixel-aligned.
        for i in 1..num_steps {
            let x = x_at(i);
            painter.rect_filled(
                Rect::from_min_max(
                    egui::pos2(x, rect.min.y),
                    egui::pos2(x + 1.0, rect.max.y),
                ),
                0.0,
                line_color,
            );
        }

        painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, line_color), StrokeKind::Inside);

        if active {
            let i = current_step;
            let step_rect = Rect::from_min_max(
                egui::pos2(x_at(i), rect.min.y),
                egui::pos2(x_at(i + 1), rect.max.y),
            );
            painter.rect_stroke(step_rect, 0.0, egui::Stroke::new(2.0, active_fill), StrokeKind::Inside);
        }

        // Handle fader interaction — send value edits to engine via shared state.
        // Uses the shared fader gesture conventions: double-click to reset,
        // shift+drag for fine-grained (delta-based) adjustment, else absolute.
        if let Some(pos) = response.interact_pointer_pos()
            && rect.contains(pos) {
                let step_index = ((pos.x - rect.min.x) / step_width).floor() as usize;
                let step_index = step_index.min(num_steps - 1);

                let shift = ui.input(|i| i.modifiers.shift);
                if response.double_clicked() {
                    let mut shared = self.shared.lock().unwrap();
                    shared.pending_params.push((100 + step_index, ParamValue::Float(0.0)));
                } else if response.dragged() {
                    if shift {
                        let delta_y = response.drag_delta().y;
                        let norm_delta = -delta_y / height.max(1.0) * 0.1;
                        let current = values.get(step_index).copied().unwrap_or(0.0);
                        let new_val = (current + norm_delta).clamp(0.0, 1.0);
                        let mut shared = self.shared.lock().unwrap();
                        shared.pending_params.push((100 + step_index, ParamValue::Float(new_val)));
                    } else {
                        let value = 1.0 - ((pos.y - rect.min.y) / height).clamp(0.0, 1.0);
                        let mut shared = self.shared.lock().unwrap();
                        shared.pending_params.push((100 + step_index, ParamValue::Float(value)));
                    }
                } else if response.clicked() && shift {
                    let value = 1.0 - ((pos.y - rect.min.y) / height).clamp(0.0, 1.0);
                    let mut shared = self.shared.lock().unwrap();
                    shared.pending_params.push((100 + step_index, ParamValue::Float(value)));
                }
            }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
