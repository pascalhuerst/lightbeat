//! Four draggable stop handles over a colour strip, for nodes that place
//! palette colours on a 0..1 axis (Palette to Gradient).
//!
//! The caller owns the positions; this draws them and reports what the user
//! did, so a position that a wire drives can be shown (`locked`) without
//! being editable.

use egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::theme;
use crate::widgets::fader;

pub const STRIP_HEIGHT: f32 = 22.0;
pub const HANDLE_TRACK_HEIGHT: f32 = 18.0;
const HANDLE_RADIUS: f32 = 6.0;
const HANDLE_HIT_RADIUS: f32 = 10.0;
const GAP: f32 = 4.0;

/// The height the handles and the strip need together.
pub const MIN_HEIGHT: f32 = STRIP_HEIGHT + HANDLE_TRACK_HEIGHT + GAP;

pub const COUNT: usize = 4;

/// What happened this frame.
#[derive(Default)]
pub struct StopsResponse {
    /// A handle was dragged: its index and new position.
    pub moved: Option<(usize, f32)>,
    /// A handle was double-clicked: the caller knows what it resets to.
    pub reset: Option<usize>,
}

/// `drag` remembers the handle being dragged between frames.
pub fn show(
    ui: &mut Ui,
    drag: &mut Option<usize>,
    positions: &[f32; COUNT],
    colors: &[Color32; COUNT],
    locked: &[bool; COUNT],
) -> StopsResponse {
    let avail = ui.available_size();
    let (resp, painter) = ui.allocate_painter(Vec2::new(avail.x, MIN_HEIGHT.max(avail.y)), Sense::click_and_drag());
    let rect = resp.rect;
    let mut out = StopsResponse::default();

    let handle_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), HANDLE_TRACK_HEIGHT));
    let strip_rect = Rect::from_min_size(
        Pos2::new(rect.min.x, rect.min.y + HANDLE_TRACK_HEIGHT + GAP),
        Vec2::new(rect.width(), STRIP_HEIGHT),
    );
    let x_of = |pos: f32| rect.min.x + pos.clamp(0.0, 1.0) * rect.width();

    let mut stops: Vec<(f32, Color32)> =
        positions.iter().map(|p| p.clamp(0.0, 1.0)).zip(colors.iter().copied()).collect();
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let samples = strip_rect.width().max(1.0) as usize;
    for i in 0..samples {
        let t = i as f32 / (samples - 1).max(1) as f32;
        let x = strip_rect.min.x + t * strip_rect.width();
        painter.line_segment(
            [Pos2::new(x, strip_rect.min.y), Pos2::new(x, strip_rect.max.y)],
            Stroke::new(1.0_f32, sample_stops(&stops, t)),
        );
    }
    painter.rect_stroke(strip_rect, 2.0, Stroke::new(1.0_f32, theme::STROKE), StrokeKind::Inside);

    let ptr = resp.interact_pointer_pos();
    let nearest = |p: Pos2, within: Rect| -> Option<usize> {
        if p.y < within.min.y || p.y > within.max.y {
            return None;
        }
        (0..COUNT)
            .filter(|&i| !locked[i])
            .map(|i| (i, (p.x - x_of(positions[i])).abs()))
            .filter(|(_, d)| *d < HANDLE_HIT_RADIUS)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    let grab_area = Rect::from_min_max(handle_rect.min, Pos2::new(rect.max.x, strip_rect.max.y));

    if resp.drag_started()
        && let Some(p) = ptr
    {
        *drag = nearest(p, grab_area);
    }
    if !resp.dragged() && !resp.drag_started() {
        *drag = None;
    }
    if resp.double_clicked()
        && let Some(p) = ptr
    {
        out.reset = nearest(p, grab_area);
    }
    if let Some(i) = *drag
        && let Some(p) = ptr
    {
        let new_pos = if ui.input(|inp| inp.modifiers.shift) {
            (positions[i] + resp.drag_delta().x * fader::FINE_FACTOR / rect.width().max(1.0)).clamp(0.0, 1.0)
        } else {
            ((p.x - rect.min.x) / rect.width().max(1.0)).clamp(0.0, 1.0)
        };
        if (new_pos - positions[i]).abs() > 1e-4 {
            out.moved = Some((i, new_pos));
        }
    }

    // The positions the handles are drawn at include this frame's drag, so the
    // handle follows the pointer without waiting for the engine to echo it.
    let shown = |i: usize| match out.moved {
        Some((m, v)) if m == i => v,
        _ => positions[i],
    };

    let rail_y = handle_rect.center().y;
    painter.line_segment(
        [Pos2::new(rect.min.x + GAP, rail_y), Pos2::new(rect.max.x - GAP, rail_y)],
        Stroke::new(1.0_f32, theme::PLOT_BORDER),
    );
    for (i, (&is_locked, &color)) in locked.iter().zip(colors).enumerate() {
        let hx = x_of(shown(i));
        let line_color = if is_locked { theme::PLOT_BORDER } else { theme::TEXT_DIM };
        painter.line_segment(
            [Pos2::new(hx, handle_rect.min.y), Pos2::new(hx, strip_rect.max.y)],
            Stroke::new(1.0_f32, line_color),
        );
        let centre = Pos2::new(hx, rail_y);
        let fill = if is_locked { color.gamma_multiply(0.55) } else { color };
        painter.circle_filled(centre, HANDLE_RADIUS, fill);
        // A handle a wire drives gets a lighter ring; it cannot be grabbed.
        let ring = if is_locked { theme::TEXT_DIM } else { theme::PLOT_BG };
        painter.circle_stroke(centre, HANDLE_RADIUS, Stroke::new(1.5_f32, ring));
        painter.text(
            centre,
            egui::Align2::CENTER_CENTER,
            format!("{}", i + 1),
            egui::FontId::proportional(9.0),
            text_color_for(fill),
        );
    }
    out
}

fn sample_stops(stops: &[(f32, Color32)], t: f32) -> Color32 {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else { return theme::BG };
    if t <= first.0 {
        return first.1;
    }
    if t >= last.0 {
        return last.1;
    }
    for pair in stops.windows(2) {
        let (t0, c0) = pair[0];
        let (t1, c1) = pair[1];
        if t >= t0 && t <= t1 {
            let k = ((t - t0) / (t1 - t0).max(1e-6)).clamp(0.0, 1.0);
            return lerp_color(c0, c1, k);
        }
    }
    last.1
}

fn lerp_color(a: Color32, b: Color32, k: f32) -> Color32 {
    let lerp = |x: u8, y: u8| ((1.0 - k) * x as f32 + k * y as f32).round() as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}

/// Black or white for the number on a handle, by the handle colour's
/// luminance, so the digit reads on light and dark palette entries.
fn text_color_for(bg: Color32) -> Color32 {
    let y = 0.2126 * bg.r() as f32 + 0.7152 * bg.g() as f32 + 0.0722 * bg.b() as f32;
    if y > 140.0 { theme::ON_LIGHT } else { theme::TEXT_BRIGHT }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_blends_between_the_two_stops_around_a_point() {
        let stops = [(0.0, Color32::from_rgb(0, 0, 0)), (1.0, Color32::from_rgb(200, 100, 0))];
        assert_eq!(sample_stops(&stops, 0.5), Color32::from_rgb(100, 50, 0));
        assert_eq!(sample_stops(&stops, -1.0), stops[0].1);
        assert_eq!(sample_stops(&stops, 2.0), stops[1].1);
    }
}
