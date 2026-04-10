use std::any::Any;

use egui::{self, Color32, Rect, Sense, StrokeKind, Ui, Vec2};

use crate::widgets::nodes::{NodeId, NodeWidget, PortDef, PortType};

const DEFAULT_STEPS: usize = 8;

pub struct StepSequencerNode {
    id: NodeId,
    values: Vec<f32>,
    current_step: usize,
    playing: bool,
    /// Pending trigger outputs to fire after advancing.
    pending_triggers: Vec<usize>,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl StepSequencerNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            values: vec![0.0; DEFAULT_STEPS],
            current_step: 0,
            playing: false,
            pending_triggers: Vec::new(),
            inputs: vec![PortDef::new("clock", PortType::Trigger)],
            outputs: vec![
                PortDef::new("trigger", PortType::Trigger),
                PortDef::new("value", PortType::Value),
            ],
        }
    }

    fn num_steps(&self) -> usize {
        self.values.len()
    }

    fn advance(&mut self) {
        self.current_step = (self.current_step + 1) % self.num_steps();
        self.playing = true;
        let value = self.values[self.current_step];
        println!(
            "step {} -> trigger (value: {:.2})",
            self.current_step, value
        );
        // Fire trigger output (port 0)
        self.pending_triggers.push(0);
    }
}

impl NodeWidget for StepSequencerNode {
    fn node_id(&self) -> NodeId {
        self.id
    }

    fn title(&self) -> &str {
        "Step Sequencer"
    }

    fn inputs(&self) -> &[PortDef] {
        &self.inputs
    }

    fn outputs(&self) -> &[PortDef] {
        &self.outputs
    }

    fn min_width(&self) -> f32 {
        260.0
    }

    fn min_content_height(&self) -> f32 {
        120.0
    }

    fn on_trigger_input(&mut self, port_index: usize) {
        match port_index {
            // Port 0 = "clock"
            0 => self.advance(),
            _ => {}
        }
    }

    fn drain_trigger_outputs(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.pending_triggers)
    }

    fn read_value_output(&self, port_index: usize) -> f32 {
        match port_index {
            // Port 1 = "value" — current step's value
            1 => self.values[self.current_step],
            _ => 0.0,
        }
    }

    fn show_content(&mut self, ui: &mut Ui) {
        let num_steps = self.num_steps();

        let available_width = ui.available_width();
        let step_width = available_width / num_steps as f32;
        let height = 100.0;

        let (response, painter) =
            ui.allocate_painter(Vec2::new(available_width, height), Sense::click_and_drag());
        let rect = response.rect;

        let bg_color = Color32::from_gray(30);
        let fill_color = Color32::from_rgb(80, 180, 240);
        let active_fill = Color32::from_rgb(240, 160, 40);
        let line_color = Color32::from_gray(60);

        painter.rect_filled(rect, 2.0, bg_color);

        for i in 0..num_steps {
            let x_min = rect.min.x + i as f32 * step_width;
            let x_max = x_min + step_width;

            let fill_height = self.values[i] * height;
            let fill_rect = Rect::from_min_max(
                egui::pos2(x_min, rect.max.y - fill_height),
                egui::pos2(x_max, rect.max.y),
            );

            let color = if i == self.current_step && self.playing {
                active_fill
            } else {
                fill_color
            };
            painter.rect_filled(fill_rect, 0.0, color);

            if i < num_steps - 1 {
                painter.line_segment(
                    [egui::pos2(x_max, rect.min.y), egui::pos2(x_max, rect.max.y)],
                    egui::Stroke::new(1.0, line_color),
                );
            }
        }

        painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, line_color), StrokeKind::Inside);

        if self.playing {
            let i = self.current_step;
            let x_min = rect.min.x + i as f32 * step_width;
            let x_max = x_min + step_width;
            let step_rect = Rect::from_min_max(
                egui::pos2(x_min, rect.min.y),
                egui::pos2(x_max, rect.max.y),
            );
            painter.rect_stroke(step_rect, 0.0, egui::Stroke::new(2.0, active_fill), StrokeKind::Inside);
        }

        if response.dragged() || response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if rect.contains(pos) {
                    let step_index =
                        ((pos.x - rect.min.x) / step_width).floor() as usize;
                    let step_index = step_index.min(num_steps - 1);
                    let value = 1.0 - ((pos.y - rect.min.y) / height).clamp(0.0, 1.0);
                    self.values[step_index] = value;
                }
            }
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
