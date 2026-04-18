use std::any::Any;

use egui::{self, Color32, Pos2, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::engine::nodes::math::gradient_source::GradientSourceDisplay;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const MAX_STOPS: usize = GRADIENT_STOP_COUNT;

pub struct GradientSourceWidget {
    id: NodeId,
    shared: SharedState,
    /// `(used, position, r, g, b, alpha)` per slot. The widget is the
    /// authoritative source of truth for edits; on change it pushes to
    /// pending_config which the engine's load_data consumes.
    stops: [StopEdit; MAX_STOPS],
    /// Mirror of the engine's latest active stops — used by the node
    /// preview so the visual matches what's being emitted.
    preview_stops: Vec<(f32, egui::Color32, f32)>,
}

#[derive(Clone, Copy)]
struct StopEdit {
    used: bool,
    position: f32,
    r: f32,
    g: f32,
    b: f32,
    alpha: f32,
}

impl Default for StopEdit {
    fn default() -> Self {
        Self { used: false, position: 0.0, r: 0.0, g: 0.0, b: 0.0, alpha: 1.0 }
    }
}

impl GradientSourceWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        // Default: black → white two-stop gradient (matches the engine's default).
        let mut stops = [StopEdit::default(); MAX_STOPS];
        stops[0] = StopEdit { used: true, position: 0.0, r: 0.0, g: 0.0, b: 0.0, alpha: 1.0 };
        stops[1] = StopEdit { used: true, position: 1.0, r: 1.0, g: 1.0, b: 1.0, alpha: 1.0 };
        Self { id, shared, stops, preview_stops: Vec::new() }
    }

    fn push_config(&self) {
        let stops: Vec<serde_json::Value> = self.stops.iter().map(|s| {
            serde_json::json!({
                "used": s.used,
                "position": s.position,
                "r": s.r, "g": s.g, "b": s.b,
                "alpha": s.alpha,
            })
        }).collect();
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({ "stops": stops }));
    }

    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Some(arr) = data.get("stops").and_then(|v| v.as_array()) {
            for (i, entry) in arr.iter().take(MAX_STOPS).enumerate() {
                let used = entry.get("used").and_then(|v| v.as_bool()).unwrap_or(false);
                if !used { self.stops[i] = StopEdit::default(); continue; }
                self.stops[i] = StopEdit {
                    used: true,
                    position: entry.get("position").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    r: entry.get("r").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    g: entry.get("g").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    b: entry.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    alpha: entry.get("alpha").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                };
            }
        }
    }
}

impl NodeWidget for GradientSourceWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Gradient Source" }
    fn title(&self) -> &str { "Gradient Source" }
    fn description(&self) -> &'static str {
        "Authors an 8-stop gradient (color + alpha + position per stop). Output feeds Group Output and any other Gradient-accepting node."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("gradient", PortType::Gradient))]
    }

    fn min_width(&self) -> f32 { 140.0 }
    fn min_content_height(&self) -> f32 { 40.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        // Pull the active stops from the engine display so the node always
        // renders what's actually being emitted (including after load).
        let snap = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<GradientSourceDisplay>())
                .map(|d| d.stops.iter().map(|(p, c, a)| {
                    let col = egui::Color32::from_rgba_unmultiplied(
                        (c.r.clamp(0.0, 1.0) * 255.0) as u8,
                        (c.g.clamp(0.0, 1.0) * 255.0) as u8,
                        (c.b.clamp(0.0, 1.0) * 255.0) as u8,
                        (a.clamp(0.0, 1.0) * 255.0) as u8,
                    );
                    (*p, col, *a)
                }).collect::<Vec<_>>())
        };
        if let Some(s) = snap { self.preview_stops = s; }

        // Draw a continuous gradient preview across the content rect.
        let avail = ui.available_size();
        let (resp, painter) = ui.allocate_painter(avail, Sense::hover());
        let rect = resp.rect;

        // Checkerboard so alpha is visible.
        draw_checker(&painter, rect);

        // Sample the gradient at many points across width.
        // Use the preview_stops (sorted by position — we don't re-sort here
        // since the engine already does).
        let samples = (rect.width() as usize).max(16).min(512);
        if !self.preview_stops.is_empty() {
            for i in 0..samples {
                let t = i as f32 / (samples - 1).max(1) as f32;
                let x = rect.min.x + (i as f32 / samples as f32) * rect.width();
                let col = sample_preview(&self.preview_stops, t);
                painter.line_segment(
                    [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                    Stroke::new(rect.width() / samples as f32 + 0.5, col),
                );
            }
        } else {
            painter.rect_filled(rect, 2.0, Color32::from_gray(40));
        }

        painter.rect_stroke(rect, 2.0, Stroke::new(1.0, Color32::from_gray(80)), StrokeKind::Inside);

        // Stop markers: small triangles above the bar at each stop position.
        for (pos, col, _a) in &self.preview_stops {
            let x = rect.min.x + pos.clamp(0.0, 1.0) * rect.width();
            let y = rect.min.y;
            let tri = [
                Pos2::new(x, y),
                Pos2::new(x - 3.0, y - 5.0),
                Pos2::new(x + 3.0, y - 5.0),
            ];
            painter.add(egui::Shape::convex_polygon(tri.to_vec(), *col, Stroke::new(0.5, Color32::BLACK)));
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let mut changed = false;
        let mut swap: Option<(usize, usize)> = None;

        ui.label(egui::RichText::new("Stops").strong());

        let len = self.stops.len();
        for (i, s) in self.stops.iter_mut().enumerate() {
            ui.push_id(("stop", i), |ui| {
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut s.used, "").on_hover_text("Enable this stop").changed() {
                        changed = true;
                    }
                    ui.add_enabled_ui(s.used, |ui| {
                        let mut col = [s.r, s.g, s.b];
                        if ui.color_edit_button_rgb(&mut col).changed() {
                            s.r = col[0]; s.g = col[1]; s.b = col[2];
                            changed = true;
                        }
                        ui.label("pos");
                        if ui.add(egui::Slider::new(&mut s.position, 0.0..=1.0)
                            .step_by(0.01)
                            .show_value(true)
                        ).changed() { changed = true; }
                        ui.label("α");
                        if ui.add(egui::Slider::new(&mut s.alpha, 0.0..=1.0)
                            .step_by(0.01)
                            .show_value(true)
                        ).changed() { changed = true; }
                    });
                    if ui.add_enabled(i > 0, egui::Button::new(egui_phosphor::regular::ARROW_UP))
                        .on_hover_text("Move stop up")
                        .clicked()
                    {
                        swap = Some((i, i - 1));
                    }
                    if ui.add_enabled(i + 1 < len, egui::Button::new(egui_phosphor::regular::ARROW_DOWN))
                        .on_hover_text("Move stop down")
                        .clicked()
                    {
                        swap = Some((i, i + 1));
                    }
                });
            });
        }

        if let Some((a, b)) = swap {
            self.stops.swap(a, b);
            changed = true;
        }

        if changed { self.push_config(); }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

fn draw_checker(painter: &egui::Painter, rect: egui::Rect) {
    let cell = 6.0;
    let cols = (rect.width() / cell).ceil() as i32;
    let rows = (rect.height() / cell).ceil() as i32;
    let c1 = Color32::from_gray(40);
    let c2 = Color32::from_gray(70);
    for y in 0..rows {
        for x in 0..cols {
            let color = if (x + y) % 2 == 0 { c1 } else { c2 };
            let cell_rect = egui::Rect::from_min_size(
                Pos2::new(rect.min.x + x as f32 * cell, rect.min.y + y as f32 * cell),
                Vec2::splat(cell),
            ).intersect(rect);
            painter.rect_filled(cell_rect, 0.0, color);
        }
    }
}

/// Minimal preview-side sampler: linear interpolation in sRGB+alpha.
fn sample_preview(stops: &[(f32, Color32, f32)], t: f32) -> Color32 {
    if stops.is_empty() { return Color32::BLACK; }
    if t <= stops[0].0 { return stops[0].1; }
    if t >= stops.last().unwrap().0 { return stops.last().unwrap().1; }
    for i in 1..stops.len() {
        if t <= stops[i].0 {
            let a = &stops[i - 1];
            let b = &stops[i];
            let range = b.0 - a.0;
            let local = if range > 0.0 { (t - a.0) / range } else { 0.0 };
            let lerp = |x: u8, y: u8| (x as f32 * (1.0 - local) + y as f32 * local).round() as u8;
            return Color32::from_rgba_unmultiplied(
                lerp(a.1.r(), b.1.r()),
                lerp(a.1.g(), b.1.g()),
                lerp(a.1.b(), b.1.b()),
                lerp(a.1.a(), b.1.a()),
            );
        }
    }
    Color32::BLACK
}
