use std::any::Any;

use egui::{self, Color32, Pos2, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::engine::nodes::display::scope::ScopeDisplay;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const WAVE_COLORS: [Color32; 2] = [
    Color32::from_rgb(80, 240, 120),
    Color32::from_rgb(240, 140, 80),
];

pub struct ScopeWidget {
    id: NodeId,
    shared: SharedState,
    ui_inputs: Vec<UiPortDef>,
}

impl ScopeWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            ui_inputs: vec![
                UiPortDef::from_def(&PortDef::new("in 1", PortType::Any)),
                UiPortDef::from_def(&PortDef::new("in 2", PortType::Any)),
            ],
        }
    }
}

impl NodeWidget for ScopeWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Scope" }
    fn title(&self) -> &str { "Scope" }

    fn ui_inputs(&self) -> Vec<UiPortDef> { self.ui_inputs.clone() }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 180.0 }
    fn min_content_height(&self) -> f32 { 80.0 }
    fn resizable(&self) -> bool { true }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn on_ui_connect(&mut self, input_port: usize, source_type: PortType) {
        if input_port < 2 {
            self.ui_inputs[input_port] = UiPortDef::from_def(
                &PortDef::new(format!("in {}", input_port + 1), source_type),
            ).with_fill(WAVE_COLORS[input_port]);
        }
    }

    fn on_ui_disconnect(&mut self, input_port: usize) {
        if input_port < 2 {
            self.ui_inputs[input_port] = UiPortDef::from_def(
                &PortDef::new(format!("in {}", input_port + 1), PortType::Any),
            );
        }
    }

    fn show_content(&mut self, ui: &mut Ui) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<ScopeDisplay>());

        let (buffers, connected) = if let Some(d) = display {
            (d.buffers.clone(), d.connected_types)
        } else {
            ([vec![], vec![]], [None, None])
        };

        let params = &shared.current_params;
        let threshold = params.iter().find_map(|p| match p {
            ParamDef::Float { name, value, .. } if name == "Threshold" => Some(*value),
            _ => None,
        }).unwrap_or(0.5);
        let width_samples = params.iter().find_map(|p| match p {
            ParamDef::Int { name, value, .. } if name == "Width" => Some(*value as usize),
            _ => None,
        }).unwrap_or(200);
        let range_min = params.iter().find_map(|p| match p {
            ParamDef::Float { name, value, .. } if name == "Range Min" => Some(*value),
            _ => None,
        }).unwrap_or(0.0);
        let range_max = params.iter().find_map(|p| match p {
            ParamDef::Float { name, value, .. } if name == "Range Max" => Some(*value),
            _ => None,
        }).unwrap_or(1.0);
        drop(shared);

        let size = Vec2::new(ui.available_width(), ui.available_height().max(60.0));
        draw_scope(ui, &buffers, &connected, width_samples, range_min, range_max, threshold, size);
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<ScopeDisplay>());

        let (buffers, connected) = if let Some(d) = display {
            (d.buffers.clone(), d.connected_types)
        } else {
            ([vec![], vec![]], [None, None])
        };

        let params = &shared.current_params;
        let threshold = params.iter().find_map(|p| match p {
            ParamDef::Float { name, value, .. } if name == "Threshold" => Some(*value),
            _ => None,
        }).unwrap_or(0.5);
        let width_samples = params.iter().find_map(|p| match p {
            ParamDef::Int { name, value, .. } if name == "Width" => Some(*value as usize),
            _ => None,
        }).unwrap_or(200);
        let range_min = params.iter().find_map(|p| match p {
            ParamDef::Float { name, value, .. } if name == "Range Min" => Some(*value),
            _ => None,
        }).unwrap_or(0.0);
        let range_max = params.iter().find_map(|p| match p {
            ParamDef::Float { name, value, .. } if name == "Range Max" => Some(*value),
            _ => None,
        }).unwrap_or(1.0);
        drop(shared);

        ui.heading("Waveform");
        let size = Vec2::new(ui.available_width(), 200.0);
        draw_scope(ui, &buffers, &connected, width_samples, range_min, range_max, threshold, size);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

fn draw_scope(
    ui: &mut Ui,
    buffers: &[Vec<f32>; 2],
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
    if range.abs() < 1e-6 { return; }

    let threshold_y = rect.max.y - ((threshold - range_min) / range) * rect.height();
    if threshold_y > rect.min.y && threshold_y < rect.max.y {
        painter.line_segment(
            [Pos2::new(rect.min.x, threshold_y), Pos2::new(rect.max.x, threshold_y)],
            Stroke::new(1.0, threshold_color),
        );
    }

    if range_min < 0.0 && range_max > 0.0 {
        let zero_y = rect.max.y - ((-range_min) / range) * rect.height();
        painter.line_segment(
            [Pos2::new(rect.min.x, zero_y), Pos2::new(rect.max.x, zero_y)],
            Stroke::new(1.0, grid_color),
        );
    }

    for ch in 0..2 {
        if connected[ch].is_none() { continue; }
        let buffer = &buffers[ch];
        let num_visible = width_samples.min(buffer.len());
        if num_visible < 2 { continue; }

        let start = buffer.len().saturating_sub(num_visible);
        let dx = rect.width() / (num_visible - 1) as f32;
        let color = WAVE_COLORS[ch];

        let mut prev: Option<Pos2> = None;
        for i in 0..num_visible {
            let sample = buffer[start + i];
            let t = (sample - range_min) / range;
            let x = rect.min.x + i as f32 * dx;
            let y = (rect.max.y - t * rect.height()).clamp(rect.min.y, rect.max.y);
            let pos = Pos2::new(x, y);
            if let Some(p) = prev {
                painter.line_segment([p, pos], Stroke::new(1.5, color));
            }
            prev = Some(pos);
        }
    }
}
