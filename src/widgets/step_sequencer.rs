use std::any::Any;

use egui::{self, Color32, Rect, Sense, StrokeKind, Ui, Vec2};

use crate::widgets::nodes::{NodeId, NodeWidget, ParamDef, ParamValue, PortDef, PortType};

const DEFAULT_STEPS: usize = 8;

pub struct StepSequencerNode {
    id: NodeId,
    values: Vec<f32>,
    current_step: usize,
    prev_step: Option<usize>,
    active: bool,
    phase_in: f32,
    trigger_out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl StepSequencerNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            values: vec![0.0; DEFAULT_STEPS],
            current_step: 0,
            prev_step: None,
            active: false,
            phase_in: 0.0,
            trigger_out: 0.0,
            inputs: vec![PortDef::new("phase", PortType::Phase)],
            outputs: vec![
                PortDef::new("trigger", PortType::Logic),
                PortDef::new("value", PortType::Untyped),
            ],
        }
    }

    fn num_steps(&self) -> usize {
        self.values.len()
    }

    fn set_num_steps(&mut self, n: usize) {
        let n = n.max(1);
        self.values.resize(n, 0.0);
        if self.current_step >= n {
            self.current_step = 0;
            self.prev_step = None;
        }
    }
}

impl NodeWidget for StepSequencerNode {
    fn node_id(&self) -> NodeId {
        self.id
    }

    fn type_name(&self) -> &'static str {
        "Step Sequencer"
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
        160.0
    }

    fn min_content_height(&self) -> f32 {
        80.0
    }

    fn resizable(&self) -> bool {
        true
    }

    fn read_input(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.phase_in,
            _ => 0.0,
        }
    }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if port_index == 0 {
            self.phase_in = value.rem_euclid(1.0);
            self.active = true;
        }
    }

    fn process(&mut self) {
        if !self.active {
            self.trigger_out = 0.0;
            return;
        }

        let step = (self.phase_in * self.num_steps() as f32).floor() as usize;
        let step = step.min(self.num_steps() - 1);

        if self.prev_step != Some(step) {
            self.current_step = step;
            self.trigger_out = 1.0;
            self.prev_step = Some(step);
        } else {
            self.trigger_out = 0.0;
        }
    }

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index {
            0 => self.trigger_out,
            1 => self.values[self.current_step],
            _ => 0.0,
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Int {
            name: "Steps".into(),
            value: self.num_steps() as i64,
            min: 1,
            max: 64,
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if let (0, ParamValue::Int(v)) = (index, value) {
            self.set_num_steps(v as usize);
        }
    }

    fn show_content(&mut self, ui: &mut Ui) {
        let num_steps = self.num_steps();

        let available_width = ui.available_width();
        let step_width = available_width / num_steps as f32;
        let height = ui.available_height().max(60.0);

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

            let color = if i == self.current_step && self.active {
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

        painter.rect_stroke(
            rect,
            2.0,
            egui::Stroke::new(1.0, line_color),
            StrokeKind::Inside,
        );

        if self.active {
            let i = self.current_step;
            let x_min = rect.min.x + i as f32 * step_width;
            let x_max = x_min + step_width;
            let step_rect = Rect::from_min_max(
                egui::pos2(x_min, rect.min.y),
                egui::pos2(x_max, rect.max.y),
            );
            painter.rect_stroke(
                step_rect,
                0.0,
                egui::Stroke::new(2.0, active_fill),
                StrokeKind::Inside,
            );
        }

        if response.dragged() || response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if rect.contains(pos) {
                    let step_index = ((pos.x - rect.min.x) / step_width).floor() as usize;
                    let step_index = step_index.min(num_steps - 1);
                    let value = 1.0 - ((pos.y - rect.min.y) / height).clamp(0.0, 1.0);
                    self.values[step_index] = value;
                }
            }
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "values": self.values }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(vals) = data.get("values").and_then(|v| v.as_array()) {
            self.values = vals
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();
            if self.values.is_empty() {
                self.values = vec![0.0; DEFAULT_STEPS];
            }
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
