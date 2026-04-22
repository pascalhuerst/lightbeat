use egui::{Color32, Pos2, Rect, Response, StrokeKind, Ui};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Orientation {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy)]
pub struct FaderStyle {
    pub bg: Color32,
    pub fill: Color32,
    pub fill_active: Color32,
    pub border: Option<Color32>,
    pub corner_radius: f32,
}

impl Default for FaderStyle {
    fn default() -> Self {
        Self {
            bg: Color32::from_gray(30),
            fill: Color32::from_rgb(80, 180, 240),
            fill_active: Color32::from_rgb(240, 160, 40),
            border: Some(Color32::from_gray(60)),
            corner_radius: 2.0,
        }
    }
}

/// Map a pointer position inside `rect` to a 0..1 value along the given orientation.
pub fn value_from_pos(pos: Pos2, rect: Rect, orient: Orientation) -> f32 {
    match orient {
        Orientation::Vertical => {
            (1.0 - ((pos.y - rect.min.y) / rect.height().max(1.0))).clamp(0.0, 1.0)
        }
        Orientation::Horizontal => ((pos.x - rect.min.x) / rect.width().max(1.0)).clamp(0.0, 1.0),
    }
}

/// Paint a single fader fill inside `rect`. Does not handle interaction.
pub fn draw_fader(
    painter: &egui::Painter,
    rect: Rect,
    value: f32,
    orient: Orientation,
    style: &FaderStyle,
    active: bool,
) {
    painter.rect_filled(rect, style.corner_radius, style.bg);

    let v = value.clamp(0.0, 1.0);
    let fill_rect = match orient {
        Orientation::Vertical => Rect::from_min_max(
            egui::pos2(rect.min.x, rect.max.y - v * rect.height()),
            rect.max,
        ),
        Orientation::Horizontal => Rect::from_min_max(
            rect.min,
            egui::pos2(rect.min.x + v * rect.width(), rect.max.y),
        ),
    };
    let color = if active {
        style.fill_active
    } else {
        style.fill
    };
    painter.rect_filled(fill_rect, 0.0, color);

    if let Some(b) = style.border {
        painter.rect_stroke(
            rect,
            style.corner_radius,
            egui::Stroke::new(1.0, b),
            StrokeKind::Inside,
        );
    }
}

/// Paint a semi-transparent overlay fill at `value` position, used to show an
/// underlying input value when an override is active.
pub fn draw_fader_overlay(
    painter: &egui::Painter,
    rect: Rect,
    value: f32,
    orient: Orientation,
    color: Color32,
) {
    let v = value.clamp(0.0, 1.0);
    let fill_rect = match orient {
        Orientation::Vertical => Rect::from_min_max(
            egui::pos2(rect.min.x, rect.max.y - v * rect.height()),
            rect.max,
        ),
        Orientation::Horizontal => Rect::from_min_max(
            rect.min,
            egui::pos2(rect.min.x + v * rect.width(), rect.max.y),
        ),
    };
    painter.rect_filled(fill_rect, 0.0, color);
}

/// Bipolar fill: from center (0.5) outward to `value`. Above center for v>0.5,
/// below center for v<0.5. Used by faders in bipolar mode to show signed offset.
pub fn draw_bipolar_fill(
    painter: &egui::Painter,
    rect: Rect,
    value: f32,
    orient: Orientation,
    color: Color32,
) {
    let v = value.clamp(0.0, 1.0);
    let fill_rect = match orient {
        Orientation::Vertical => {
            let center_y = rect.min.y + 0.5 * rect.height();
            let pos_y = rect.min.y + (1.0 - v) * rect.height();
            if pos_y <= center_y {
                Rect::from_min_max(
                    egui::pos2(rect.min.x, pos_y),
                    egui::pos2(rect.max.x, center_y),
                )
            } else {
                Rect::from_min_max(
                    egui::pos2(rect.min.x, center_y),
                    egui::pos2(rect.max.x, pos_y),
                )
            }
        }
        Orientation::Horizontal => {
            let center_x = rect.min.x + 0.5 * rect.width();
            let pos_x = rect.min.x + v * rect.width();
            if pos_x >= center_x {
                Rect::from_min_max(
                    egui::pos2(center_x, rect.min.y),
                    egui::pos2(pos_x, rect.max.y),
                )
            } else {
                Rect::from_min_max(
                    egui::pos2(pos_x, rect.min.y),
                    egui::pos2(center_x, rect.max.y),
                )
            }
        }
    };
    painter.rect_filled(fill_rect, 0.0, color);
}

/// Draw the center reference line on a bipolar fader (a thin horizontal/vertical
/// line through the middle of `rect`).
pub fn draw_bipolar_center_line(
    painter: &egui::Painter,
    rect: Rect,
    orient: Orientation,
    color: Color32,
) {
    match orient {
        Orientation::Vertical => {
            let y = rect.min.y + 0.5 * rect.height();
            painter.line_segment(
                [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                egui::Stroke::new(1.0, color),
            );
        }
        Orientation::Horizontal => {
            let x = rect.min.x + 0.5 * rect.width();
            painter.line_segment(
                [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                egui::Stroke::new(1.0, color),
            );
        }
    }
}

/// Handle interaction for a single fader occupying `rect`. Updates `value` in
/// place. Returns `true` if the user interacted (clicked, dragged, or
/// double-clicked) this frame — useful for port highlight timestamps.
///
/// Gestures:
/// - Plain click: no value change (avoids surprise jumps when grabbing a fader).
/// - Shift + click: jump to cursor position (absolute).
/// - Drag: cursor-tracking (absolute).
/// - Shift + drag: delta-based fine-grained drag (10× slower).
/// - Double-click: reset to 0.
pub fn handle_fader_interaction(
    ui: &Ui,
    response: &Response,
    rect: Rect,
    orient: Orientation,
    value: &mut f32,
) -> bool {
    let mut changed = false;

    if response.double_clicked() {
        *value = 0.0;
        return true;
    }

    let shift = ui.input(|i| i.modifiers.shift);

    if response.dragged() {
        if shift {
            let delta = response.drag_delta();
            let norm_delta = match orient {
                Orientation::Vertical => -delta.y / rect.height().max(1.0),
                Orientation::Horizontal => delta.x / rect.width().max(1.0),
            };
            *value = (*value + norm_delta * 0.1).clamp(0.0, 1.0);
            changed = true;
        } else if let Some(pos) = response.interact_pointer_pos() {
            *value = value_from_pos(pos, rect, orient);
            changed = true;
        }
    } else if response.clicked() && shift
        && let Some(pos) = response.interact_pointer_pos() {
            *value = value_from_pos(pos, rect, orient);
            changed = true;
        }

    changed
}

/// Compute a linear fade-out alpha (1.0 → 0.0) given a timestamp of when an
/// event occurred and a duration. Returns 0 if `last` is None.
pub fn highlight_alpha(last: Option<f64>, now: f64, duration: f64) -> f32 {
    match last {
        None => 0.0,
        Some(t) => {
            let elapsed = now - t;
            if elapsed < 0.0 || elapsed >= duration {
                0.0
            } else {
                (1.0 - (elapsed / duration)) as f32
            }
        }
    }
}
