//! Drawing and stop editing for gradients that are shared by the Gradient
//! Source node and the Gradient presets tab.

use egui::{Color32, Pos2, Sense, Stroke, StrokeKind, Ui, Vec2};
use egui_phosphor::regular as icon;

use crate::color::{Gradient, GradientStop, Rgb};
use crate::engine::types::GRADIENT_STOP_COUNT;
use crate::theme;

fn to_color32(rgb: Rgb, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (rgb.r.clamp(0.0, 1.0) * 255.0) as u8,
        (rgb.g.clamp(0.0, 1.0) * 255.0) as u8,
        (rgb.b.clamp(0.0, 1.0) * 255.0) as u8,
        (alpha.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

/// A strip of the gradient, drawn over a checkerboard. The stops need not be
/// sorted.
pub fn draw_strip(ui: &mut Ui, size: Vec2, stops: &[GradientStop]) {
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;
    theme::draw_checkerboard(&painter, rect, 5.0);

    if !stops.is_empty() {
        let g = Gradient::new(stops.to_vec());
        let samples = (rect.width() as usize).clamp(16, 256);
        for i in 0..samples {
            let t = i as f32 / (samples - 1).max(1) as f32;
            let x = rect.min.x + (i as f32 / samples as f32) * rect.width();
            let (rgb, alpha) = g.sample_with_alpha(t);
            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(rect.width() / samples as f32 + 0.5, to_color32(rgb, alpha)),
            );
        }
    }
    painter.rect_stroke(rect, 2.0, Stroke::new(1.0_f32, theme::STROKE), StrokeKind::Inside);
}

/// A new stop in the middle of the widest gap, coloured from the gradient as
/// it is there so the picture does not jump. None when the list is full.
pub fn new_stop(stops: &[GradientStop]) -> Option<GradientStop> {
    if stops.len() >= GRADIENT_STOP_COUNT {
        return None;
    }
    if stops.is_empty() {
        return Some(GradientStop::opaque(0.0, Rgb::WHITE));
    }
    let mut pos: Vec<f32> = stops.iter().map(|s| s.position.clamp(0.0, 1.0)).collect();
    pos.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // The ends count as gaps too, so a single stop still gets a neighbour.
    let mut edges = vec![0.0];
    edges.extend(pos);
    edges.push(1.0);
    let (lo, hi) = edges
        .windows(2)
        .map(|w| (w[0], w[1]))
        .max_by(|a, b| (a.1 - a.0).partial_cmp(&(b.1 - b.0)).unwrap_or(std::cmp::Ordering::Equal))?;
    let at = (lo + hi) * 0.5;
    let (color, alpha) = Gradient::new(stops.to_vec()).sample_with_alpha(at);
    Some(GradientStop { position: at, color, alpha })
}

/// One row per stop (colour, position, alpha, delete) and an "Add stop"
/// button. Returns true when anything changed.
pub fn show_stops(ui: &mut Ui, stops: &mut Vec<GradientStop>) -> bool {
    let mut changed = false;
    let mut remove = None;
    let can_remove = stops.len() > 1;
    for (i, s) in stops.iter_mut().enumerate() {
        ui.push_id(("gradient_stop", i), |ui| {
            ui.horizontal(|ui| {
                let mut col = [s.color.r, s.color.g, s.color.b];
                if ui.color_edit_button_rgb(&mut col).changed() {
                    s.color = Rgb::new(col[0], col[1], col[2]);
                    changed = true;
                }
                ui.label("pos");
                changed |= ui.add(egui::Slider::new(&mut s.position, 0.0..=1.0).step_by(0.01)).changed();
                ui.label("alpha");
                changed |= ui.add(egui::Slider::new(&mut s.alpha, 0.0..=1.0).step_by(0.01)).changed();
                if ui
                    .add_enabled(can_remove, egui::Button::new(icon::X).small())
                    .on_hover_text("Delete stop")
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        stops.remove(i);
        changed = true;
    }
    let full = stops.len() >= GRADIENT_STOP_COUNT;
    let add = ui
        .add_enabled(!full, egui::Button::new(format!("{} Add stop", icon::PLUS)))
        .on_disabled_hover_text(format!("A gradient has at most {GRADIENT_STOP_COUNT} stops"));
    if add.clicked()
        && let Some(stop) = new_stop(stops)
    {
        stops.push(stop);
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(p: f32) -> GradientStop {
        GradientStop::opaque(p, Rgb::WHITE)
    }

    #[test]
    fn new_stop_goes_into_the_widest_gap() {
        let s = new_stop(&[stop(0.0), stop(0.2), stop(1.0)]).unwrap();
        assert!((s.position - 0.6).abs() < 1e-6);
    }

    #[test]
    fn new_stop_next_to_a_single_stop() {
        let s = new_stop(&[stop(0.0)]).unwrap();
        assert!((s.position - 0.5).abs() < 1e-6);
    }

    #[test]
    fn new_stop_is_refused_when_full() {
        let full: Vec<GradientStop> = (0..GRADIENT_STOP_COUNT).map(|i| stop(i as f32 / 8.0)).collect();
        assert!(new_stop(&full).is_none());
    }

    #[test]
    fn new_stop_takes_the_gradient_colour_there() {
        let a = GradientStop::opaque(0.0, Rgb::BLACK);
        let b = GradientStop::opaque(1.0, Rgb::WHITE);
        let s = new_stop(&[a, b]).unwrap();
        assert!((s.color.r - 0.5).abs() < 1e-3);
    }
}
