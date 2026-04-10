use std::any::Any;
use std::collections::VecDeque;

use egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::widgets::nodes::{NodeId, NodeWidget, ParamDef, ParamValue, PortDef, PortType};

const MAX_SAMPLES: usize = 512;

pub struct ScopeNode {
    id: NodeId,
    buffer: VecDeque<f32>,
    input_value: f32,
    trigger_threshold: f32,
    /// Number of samples visible in the scope width.
    width_samples: usize,
    /// Min display value.
    range_min: f32,
    /// Max display value.
    range_max: f32,
    inputs: Vec<PortDef>,
    /// The type of the currently connected signal (None = disconnected).
    connected_type: Option<PortType>,
}

impl ScopeNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            buffer: VecDeque::with_capacity(MAX_SAMPLES),
            input_value: 0.0,
            trigger_threshold: 0.5,
            width_samples: 200,
            range_min: 0.0,
            range_max: 1.0,
            inputs: vec![PortDef::new("in", PortType::Any)],
            connected_type: None,
        }
    }
}

impl NodeWidget for ScopeNode {
    fn node_id(&self) -> NodeId {
        self.id
    }

    fn title(&self) -> &str {
        "Scope"
    }

    fn inputs(&self) -> &[PortDef] {
        &self.inputs
    }

    fn outputs(&self) -> &[PortDef] {
        &[]
    }

    fn min_width(&self) -> f32 {
        180.0
    }

    fn min_content_height(&self) -> f32 {
        80.0
    }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if port_index == 0 {
            self.input_value = value;
        }
    }

    fn process(&mut self) {
        self.buffer.push_back(self.input_value);
        while self.buffer.len() > MAX_SAMPLES {
            self.buffer.pop_front();
        }
    }

    fn on_connect(&mut self, _input_port: usize, source_type: PortType) {
        self.connected_type = Some(source_type);
        let (lo, hi) = source_type.default_range();
        self.range_min = lo;
        self.range_max = hi;
        // Update the displayed port color to match the connected signal.
        self.inputs[0] = PortDef::new("in", source_type);
    }

    fn on_disconnect(&mut self, _input_port: usize) {
        self.connected_type = None;
        self.inputs[0] = PortDef::new("in", PortType::Any);
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Float {
                name: "Threshold".into(),
                value: self.trigger_threshold,
                min: 0.0,
                max: 1.0,
                step: 0.01,
                unit: "",
            },
            ParamDef::Int {
                name: "Width".into(),
                value: self.width_samples as i64,
                min: 50,
                max: MAX_SAMPLES as i64,
            },
            ParamDef::Float {
                name: "Range Min".into(),
                value: self.range_min,
                min: -2.0,
                max: 2.0,
                step: 0.05,
                unit: "",
            },
            ParamDef::Float {
                name: "Range Max".into(),
                value: self.range_max,
                min: -2.0,
                max: 2.0,
                step: 0.05,
                unit: "",
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match (index, value) {
            (0, ParamValue::Float(v)) => self.trigger_threshold = v,
            (1, ParamValue::Int(v)) => self.width_samples = v as usize,
            (2, ParamValue::Float(v)) => self.range_min = v,
            (3, ParamValue::Float(v)) => self.range_max = v,
            _ => {}
        }
    }

    fn show_content(&mut self, ui: &mut Ui) {
        draw_scope(ui, &self.buffer, self.width_samples, self.range_min, self.range_max, self.trigger_threshold);
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        ui.heading("Waveform");
        let size = Vec2::new(ui.available_width(), 200.0);
        draw_scope_sized(ui, &self.buffer, self.width_samples, self.range_min, self.range_max, self.trigger_threshold, size);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn draw_scope(
    ui: &mut Ui,
    buffer: &VecDeque<f32>,
    width_samples: usize,
    range_min: f32,
    range_max: f32,
    threshold: f32,
) {
    let size = Vec2::new(ui.available_width(), 60.0);
    draw_scope_sized(ui, buffer, width_samples, range_min, range_max, threshold, size);
}

fn draw_scope_sized(
    ui: &mut Ui,
    buffer: &VecDeque<f32>,
    width_samples: usize,
    range_min: f32,
    range_max: f32,
    threshold: f32,
    size: Vec2,
) {
    let (response, painter) = ui.allocate_painter(size, Sense::hover());
    let rect = response.rect;

    let bg = Color32::from_gray(20);
    let grid_color = Color32::from_gray(40);
    let wave_color = Color32::from_rgb(80, 240, 120);
    let threshold_color = Color32::from_rgb(240, 200, 40).linear_multiply(0.4);

    painter.rect_filled(rect, 2.0, bg);
    painter.rect_stroke(rect, 2.0, Stroke::new(1.0, Color32::from_gray(50)), StrokeKind::Inside);

    let range = range_max - range_min;
    if range.abs() < 1e-6 {
        return;
    }

    // Threshold line
    let threshold_y = rect.max.y - ((threshold - range_min) / range) * rect.height();
    if threshold_y > rect.min.y && threshold_y < rect.max.y {
        painter.line_segment(
            [Pos2::new(rect.min.x, threshold_y), Pos2::new(rect.max.x, threshold_y)],
            Stroke::new(1.0, threshold_color),
        );
    }

    // Center line (at 0 if in range)
    if range_min < 0.0 && range_max > 0.0 {
        let zero_y = rect.max.y - ((-range_min) / range) * rect.height();
        painter.line_segment(
            [Pos2::new(rect.min.x, zero_y), Pos2::new(rect.max.x, zero_y)],
            Stroke::new(1.0, grid_color),
        );
    }

    // Draw waveform
    let num_visible = width_samples.min(buffer.len());
    if num_visible < 2 {
        return;
    }

    let start = buffer.len().saturating_sub(num_visible);
    let dx = rect.width() / (num_visible - 1) as f32;

    let mut prev: Option<Pos2> = None;
    for i in 0..num_visible {
        let sample = buffer[start + i];
        let t = (sample - range_min) / range;
        let x = rect.min.x + i as f32 * dx;
        let y = rect.max.y - t * rect.height();
        let y = y.clamp(rect.min.y, rect.max.y);
        let pos = Pos2::new(x, y);

        if let Some(p) = prev {
            painter.line_segment([p, pos], Stroke::new(1.5, wave_color));
        }
        prev = Some(pos);
    }
}
