use std::any::Any;
use std::collections::VecDeque;

use egui::{self, Color32, Pos2, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::widgets::nodes::{NodeId, NodeWidget, ParamDef, ParamValue, PortDef, PortType};

const MAX_SAMPLES: usize = 512;
const WAVE_COLORS: [Color32; 2] = [
    Color32::from_rgb(80, 240, 120),
    Color32::from_rgb(240, 140, 80),
];

pub struct ScopeNode {
    id: NodeId,
    buffers: [VecDeque<f32>; 2],
    input_values: [f32; 2],
    trigger_threshold: f32,
    width_samples: usize,
    range_min: f32,
    range_max: f32,
    inputs: Vec<PortDef>,
    connected_types: [Option<PortType>; 2],
}

impl ScopeNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            buffers: [
                VecDeque::with_capacity(MAX_SAMPLES),
                VecDeque::with_capacity(MAX_SAMPLES),
            ],
            input_values: [0.0; 2],
            trigger_threshold: 0.5,
            width_samples: 200,
            range_min: 0.0,
            range_max: 1.0,
            inputs: vec![
                PortDef::new("in 1", PortType::Any),
                PortDef::new("in 2", PortType::Any),
            ],
            connected_types: [None, None],
        }
    }
}

impl NodeWidget for ScopeNode {
    fn node_id(&self) -> NodeId {
        self.id
    }

    fn type_name(&self) -> &'static str {
        "Scope"
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

    fn resizable(&self) -> bool {
        true
    }

    fn read_input(&self, port_index: usize) -> f32 {
        if port_index < 2 { self.input_values[port_index] } else { 0.0 }
    }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if port_index < 2 {
            self.input_values[port_index] = value;
        }
    }

    fn process(&mut self) {
        for ch in 0..2 {
            self.buffers[ch].push_back(self.input_values[ch]);
            while self.buffers[ch].len() > MAX_SAMPLES {
                self.buffers[ch].pop_front();
            }
        }
    }

    fn on_connect(&mut self, input_port: usize, source_type: PortType) {
        if input_port < 2 {
            self.connected_types[input_port] = Some(source_type);
            self.inputs[input_port] = PortDef::new(format!("in {}", input_port + 1), source_type)
                .with_fill(WAVE_COLORS[input_port]);
            // Auto-set range from the first connected input.
            if self.connected_types.iter().filter(|t| t.is_some()).count() == 1 {
                let (lo, hi) = source_type.default_range();
                self.range_min = lo;
                self.range_max = hi;
            }
        }
    }

    fn on_disconnect(&mut self, input_port: usize) {
        if input_port < 2 {
            self.connected_types[input_port] = None;
            self.inputs[input_port] = PortDef::new(format!("in {}", input_port + 1), PortType::Any);
            // If the other input is still connected, use its range.
            let other = 1 - input_port;
            if let Some(t) = self.connected_types[other] {
                let (lo, hi) = t.default_range();
                self.range_min = lo;
                self.range_max = hi;
            }
        }
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
        let size = Vec2::new(ui.available_width(), ui.available_height().max(60.0));
        draw_scope(ui, &self.buffers, &self.connected_types, self.width_samples, self.range_min, self.range_max, self.trigger_threshold, size);
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        ui.heading("Waveform");
        let size = Vec2::new(ui.available_width(), 200.0);
        draw_scope(ui, &self.buffers, &self.connected_types, self.width_samples, self.range_min, self.range_max, self.trigger_threshold, size);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn draw_scope(
    ui: &mut Ui,
    buffers: &[VecDeque<f32>; 2],
    connected: &[Option<PortType>; 2],
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

    // Draw waveforms
    for ch in 0..2 {
        if connected[ch].is_none() {
            continue;
        }
        let buffer = &buffers[ch];
        let num_visible = width_samples.min(buffer.len());
        if num_visible < 2 {
            continue;
        }

        let start = buffer.len().saturating_sub(num_visible);
        let dx = rect.width() / (num_visible - 1) as f32;
        let color = WAVE_COLORS[ch];

        let mut prev: Option<Pos2> = None;
        for i in 0..num_visible {
            let sample = buffer[start + i];
            let t = (sample - range_min) / range;
            let x = rect.min.x + i as f32 * dx;
            let y = rect.max.y - t * rect.height();
            let y = y.clamp(rect.min.y, rect.max.y);
            let pos = Pos2::new(x, y);

            if let Some(p) = prev {
                painter.line_segment([p, pos], Stroke::new(1.5, color));
            }
            prev = Some(pos);
        }
    }
}
