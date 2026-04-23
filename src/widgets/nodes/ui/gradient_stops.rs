use std::any::Any;

use egui::{self, Color32, Pos2, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::engine::nodes::ui::gradient_stops::GradientStopsDisplay;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const STRIP_HEIGHT: f32 = 22.0;
const HANDLE_TRACK_HEIGHT: f32 = 18.0;
const HANDLE_RADIUS: f32 = 6.0;
const HANDLE_HIT_RADIUS: f32 = 10.0;

pub struct GradientStopsWidget {
    id: NodeId,
    shared: SharedState,
    name: String,
    positions: [f32; 4],
    palette: [f32; 12],
    /// Which handle index is currently being dragged, if any.
    drag_index: Option<usize>,
}

impl GradientStopsWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id, shared,
            name: String::new(),
            positions: [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0],
            palette: [0.0; 12],
            drag_index: None,
        }
    }

    fn sync_from_display(&mut self) {
        let s = self.shared.lock().unwrap();
        if let Some(d) = s.display.as_ref().and_then(|d| d.downcast_ref::<GradientStopsDisplay>()) {
            self.name = d.name.clone();
            self.positions = d.positions;
            self.palette = d.palette;
        }
    }

    fn push_position(&self, idx: usize, v: f32) {
        let mut s = self.shared.lock().unwrap();
        s.pending_params.push((idx, ParamValue::Float(v)));
    }

    fn push_name(&self) {
        let mut s = self.shared.lock().unwrap();
        s.pending_config = Some(serde_json::json!({ "name": self.name }));
    }

    fn palette_color(&self, slot: usize) -> Color32 {
        let base = slot * 3;
        let r = (self.palette[base].clamp(0.0, 1.0) * 255.0) as u8;
        let g = (self.palette[base + 1].clamp(0.0, 1.0) * 255.0) as u8;
        let b = (self.palette[base + 2].clamp(0.0, 1.0) * 255.0) as u8;
        Color32::from_rgb(r, g, b)
    }
}

impl NodeWidget for GradientStopsWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Gradient Stops" }
    fn title(&self) -> &str { &self.name }
    fn description(&self) -> &'static str {
        "Four draggable stop positions (0..1) for the Palette → Gradient node. \
         Wire a palette into the input to see a live preview of the resulting \
         gradient; drag the handles to move each stop."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("palette", PortType::Palette))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("pos1", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("pos2", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("pos3", PortType::Untyped),),
            UiPortDef::from_def(&PortDef::new("pos4", PortType::Untyped)),
        ]
    }

    fn min_width(&self) -> f32 { 220.0 }
    fn min_content_height(&self) -> f32 { STRIP_HEIGHT + HANDLE_TRACK_HEIGHT + 4.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.sync_from_display();

        let avail = ui.available_size();
        let total_h = (STRIP_HEIGHT + HANDLE_TRACK_HEIGHT + 4.0).max(avail.y);
        let (resp, painter) = ui.allocate_painter(
            Vec2::new(avail.x, total_h),
            Sense::click_and_drag(),
        );
        let rect = resp.rect;

        // Split: top slice = handle track, bottom slice = gradient strip.
        let handle_rect = egui::Rect::from_min_size(
            rect.min,
            Vec2::new(rect.width(), HANDLE_TRACK_HEIGHT),
        );
        let strip_rect = egui::Rect::from_min_size(
            Pos2::new(rect.min.x, rect.min.y + HANDLE_TRACK_HEIGHT + 4.0),
            Vec2::new(rect.width(), STRIP_HEIGHT),
        );

        // --- gradient strip: sample the (positions, colours) pairs the way
        // Palette → Gradient would, linearly interpolated across x.
        let mut stops: Vec<(f32, Color32)> = (0..4)
            .map(|i| (self.positions[i].clamp(0.0, 1.0), self.palette_color(i)))
            .collect();
        stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        let samples = strip_rect.width().max(1.0) as usize;
        for i in 0..samples {
            let t = i as f32 / (samples - 1).max(1) as f32;
            let col = sample_stops(&stops, t);
            let x = strip_rect.min.x + t * strip_rect.width();
            painter.line_segment(
                [Pos2::new(x, strip_rect.min.y), Pos2::new(x, strip_rect.max.y)],
                Stroke::new(1.0, col),
            );
        }
        painter.rect_stroke(strip_rect, 2.0, Stroke::new(1.0, Color32::from_gray(80)), StrokeKind::Inside);

        // --- handles: vertical line + dot, in the palette colour.
        let ptr = resp.interact_pointer_pos();

        // Pick up the closest handle on press.
        if resp.drag_started()
            && let Some(p) = ptr {
                let mut best: Option<(usize, f32)> = None;
                for i in 0..4 {
                    let hx = rect.min.x + self.positions[i].clamp(0.0, 1.0) * rect.width();
                    let d = (p.x - hx).abs();
                    let within_y = p.y >= handle_rect.min.y && p.y <= strip_rect.max.y;
                    if within_y && d < HANDLE_HIT_RADIUS
                        && best.is_none_or(|(_, bd)| d < bd)
                    {
                        best = Some((i, d));
                    }
                }
                self.drag_index = best.map(|(i, _)| i);
            }
        if !resp.dragged() && !resp.drag_started() {
            self.drag_index = None;
        }

        if let Some(idx) = self.drag_index
            && let Some(p) = ptr {
                let new_x = ((p.x - rect.min.x) / rect.width().max(1.0)).clamp(0.0, 1.0);
                if (new_x - self.positions[idx]).abs() > 1e-4 {
                    self.positions[idx] = new_x;
                    self.push_position(idx, new_x);
                }
            }

        // Track line down the middle of the handle track so handles have a
        // visual "rail" to sit on.
        let rail_y = handle_rect.center().y;
        painter.line_segment(
            [Pos2::new(rect.min.x + 4.0, rail_y), Pos2::new(rect.max.x - 4.0, rail_y)],
            Stroke::new(1.0, Color32::from_gray(60)),
        );

        for i in 0..4 {
            let hx = rect.min.x + self.positions[i].clamp(0.0, 1.0) * rect.width();
            let col = self.palette_color(i);
            // Vertical line from handle track through the gradient strip.
            painter.line_segment(
                [Pos2::new(hx, handle_rect.min.y), Pos2::new(hx, strip_rect.max.y)],
                Stroke::new(1.0, Color32::from_gray(120)),
            );
            // Dot in the palette colour, with a dark outline so it reads on
            // any background.
            let centre = Pos2::new(hx, rail_y);
            painter.circle_filled(centre, HANDLE_RADIUS, col);
            painter.circle_stroke(
                centre, HANDLE_RADIUS,
                Stroke::new(1.5, Color32::from_gray(20)),
            );
            // Number inside the handle (1..4) for quick identification.
            painter.text(
                centre,
                egui::Align2::CENTER_CENTER,
                format!("{}", i + 1),
                egui::FontId::proportional(9.0),
                text_color_for(col),
            );
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Name:");
            if ui.text_edit_singleline(&mut self.name).changed() {
                self.push_name();
            }
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

fn sample_stops(stops: &[(f32, Color32)], t: f32) -> Color32 {
    if stops.is_empty() { return Color32::from_gray(40); }
    if t <= stops[0].0 { return stops[0].1; }
    if t >= stops[stops.len() - 1].0 { return stops[stops.len() - 1].1; }
    // Find the bracketing pair and lerp.
    for pair in stops.windows(2) {
        let (t0, c0) = pair[0];
        let (t1, c1) = pair[1];
        if t >= t0 && t <= t1 {
            let span = (t1 - t0).max(1e-6);
            let k = ((t - t0) / span).clamp(0.0, 1.0);
            return lerp_color(c0, c1, k);
        }
    }
    stops[stops.len() - 1].1
}

fn lerp_color(a: Color32, b: Color32, k: f32) -> Color32 {
    let lerp = |x: u8, y: u8| ((1.0 - k) * x as f32 + k * y as f32).round() as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}

/// Pick black or white for the handle's number label based on the handle
/// colour's luminance — keeps the digit readable on both light and dark
/// palette entries.
fn text_color_for(bg: Color32) -> Color32 {
    let y = 0.2126 * bg.r() as f32 + 0.7152 * bg.g() as f32 + 0.0722 * bg.b() as f32;
    if y > 140.0 { Color32::BLACK } else { Color32::WHITE }
}
