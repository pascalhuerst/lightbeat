use std::any::Any;

use egui::{self, Align2, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};

use crate::engine::nodes::display::meter::{MeterData, MeterKind};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

/// Seconds a level meter's peak marker stays before it falls.
const PEAK_HOLD_S: f64 = 1.0;
/// Fraction of the range per second the marker falls at after the hold.
const PEAK_FALL: f32 = 0.5;
/// Gauge sweep: from 135 degrees (bottom left) clockwise through the top to
/// 45 degrees (bottom right), in screen coordinates where y points down.
const GAUGE_START_DEG: f32 = 135.0;
const GAUGE_SWEEP_DEG: f32 = 270.0;
/// Part of the range, at the top end, that the gauge marks as a red zone.
const RED_ZONE: f32 = 0.85;

pub struct MeterWidget {
    id: NodeId,
    shared: SharedState,
    kind: MeterKind,
    name: String,
    unit: String,
    /// The engine's copy has been read once (it holds what a loaded project
    /// saved); after that this widget is the source of truth while typing.
    synced: bool,
    peak: f32,
    peak_at: f64,
}

impl MeterWidget {
    pub fn new(id: NodeId, shared: SharedState, kind: MeterKind) -> Self {
        Self { id, shared, kind, name: String::new(), unit: String::new(), synced: false, peak: 0.0, peak_at: 0.0 }
    }

    fn data(&mut self) -> Option<MeterData> {
        let shared = self.shared.lock().unwrap();
        let d = shared.display.as_ref()?.downcast_ref::<MeterData>()?;
        if !self.synced {
            self.name = d.name.clone();
            self.unit = d.unit.clone();
            self.synced = true;
        }
        Some(MeterData {
            name: d.name.clone(),
            unit: d.unit.clone(),
            value: d.value,
            min: d.min,
            max: d.max,
            vertical: d.vertical,
        })
    }

    fn push_config(&self) {
        self.shared.lock().unwrap().pending_config =
            Some(serde_json::json!({ "name": self.name, "unit": self.unit }));
    }
}

/// Where `value` sits between `min` and `max`, 0..1. An empty range reads 0.
pub fn fraction(value: f32, min: f32, max: f32) -> f32 {
    if max > min { ((value - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 }
}

/// The value with as many decimals as its range deserves, and its unit.
pub fn format_value(value: f32, min: f32, max: f32, unit: &str) -> String {
    let span = (max - min).abs();
    let decimals = if span >= 100.0 { 0 } else if span >= 10.0 { 1 } else { 2 };
    if unit.is_empty() { format!("{value:.decimals$}") } else { format!("{value:.decimals$} {unit}") }
}

/// Position on a circle at `fraction` of the gauge's sweep.
fn gauge_point(center: Pos2, radius: f32, fraction: f32) -> Pos2 {
    let angle = (GAUGE_START_DEG + GAUGE_SWEEP_DEG * fraction).to_radians();
    center + Vec2::new(angle.cos(), angle.sin()) * radius
}

/// Points along the gauge's arc from `from` to `to`, both fractions.
fn arc_points(center: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    let steps = (((to - from).abs() * 64.0).ceil() as usize).max(2);
    (0..=steps).map(|i| gauge_point(center, radius, from + (to - from) * i as f32 / steps as f32)).collect()
}

/// The colour of a level segment or marker at `position` (0..1) along the bar.
fn level_color(position: f32) -> egui::Color32 {
    if position < 0.7 {
        theme::SEM_SUCCESS
    } else if position < 0.9 {
        theme::SEM_WARNING_BRIGHT
    } else {
        theme::SEM_DANGER
    }
}

fn draw_level(painter: &egui::Painter, rect: Rect, data: &MeterData, frac: f32, peak: f32) {
    let length = if data.vertical { rect.height() } else { rect.width() };
    let segments = ((length / 6.0) as usize).clamp(4, 80);
    let gap = 1.5;
    let seg = (length - gap * (segments as f32 - 1.0)) / segments as f32;
    for i in 0..segments {
        let position = (i as f32 + 0.5) / segments as f32;
        let lit = position <= frac || (peak > 0.0 && (position - peak).abs() <= 0.5 / segments as f32);
        let color = if lit { level_color(position) } else { theme::BG_HIGH };
        let offset = i as f32 * (seg + gap);
        let cell = if data.vertical {
            // The first segment is the bottom one.
            let bottom = rect.max.y - offset;
            Rect::from_min_max(Pos2::new(rect.min.x, bottom - seg), Pos2::new(rect.max.x, bottom))
        } else {
            let left = rect.min.x + offset;
            Rect::from_min_max(Pos2::new(left, rect.min.y), Pos2::new(left + seg, rect.max.y))
        };
        painter.rect_filled(cell, 1.0, color);
    }
}

fn draw_gauge(painter: &egui::Painter, rect: Rect, data: &MeterData, frac: f32) {
    let radius = (rect.width().min(rect.height()) * 0.5 - 6.0).max(10.0);
    let center = rect.center() + Vec2::new(0.0, radius * 0.1);
    let thickness = (radius * 0.12).clamp(2.0, 8.0);

    let red = mix(theme::BG_HIGH, theme::SEM_DANGER, 0.55);
    painter.add(Shape::line(arc_points(center, radius, 0.0, RED_ZONE), Stroke::new(thickness, theme::BG_HIGH)));
    painter.add(Shape::line(arc_points(center, radius, RED_ZONE, 1.0), Stroke::new(thickness, red)));
    if frac > 0.0 {
        let color = if frac > RED_ZONE { theme::SEM_DANGER } else { theme::SEM_PRIMARY };
        painter.add(Shape::line(arc_points(center, radius, 0.0, frac), Stroke::new(thickness, color)));
    }

    // Ticks, with the range's ends and middle labelled.
    let font = FontId::proportional((radius * 0.16).clamp(8.0, 12.0));
    for i in 0..=10 {
        let f = i as f32 / 10.0;
        let major = i % 5 == 0;
        let outer = radius - thickness * 0.5 - 1.0;
        let inner = outer - if major { radius * 0.12 } else { radius * 0.06 };
        painter.line_segment(
            [gauge_point(center, inner, f), gauge_point(center, outer, f)],
            Stroke::new(if major { 1.5 } else { 1.0 }, theme::TEXT_DIM),
        );
        if major {
            let value = data.min + (data.max - data.min) * f;
            painter.text(
                gauge_point(center, inner - radius * 0.12, f),
                Align2::CENTER_CENTER,
                format_value(value, data.min, data.max, ""),
                font.clone(),
                theme::TEXT_MUTED,
            );
        }
    }

    // Needle and hub.
    painter.line_segment(
        [center, gauge_point(center, radius * 0.72, frac)],
        Stroke::new((radius * 0.04).clamp(1.5, 3.0), theme::TEXT_BRIGHT),
    );
    painter.circle_filled(center, (radius * 0.07).clamp(3.0, 6.0), theme::TEXT_BRIGHT);

    painter.text(
        center + Vec2::new(0.0, radius * 0.5),
        Align2::CENTER_CENTER,
        format_value(data.value, data.min, data.max, &data.unit),
        FontId::monospace((radius * 0.2).clamp(9.0, 16.0)),
        theme::TEXT,
    );
}

fn mix(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    theme::mix_color(a, b, t)
}

impl NodeWidget for MeterWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.kind.type_name() }
    fn description(&self) -> &'static str {
        match self.kind {
            MeterKind::Level => "A segmented level bar for the input, from Min to Max, with a peak marker. Vertical or horizontal.",
            MeterKind::Gauge => "A dial with a needle, like a speedometer, for the input between Min and Max. The last part of the range is marked red.",
        }
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("in", PortType::Any))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 {
        match self.kind {
            MeterKind::Level => 70.0,
            MeterKind::Gauge => 130.0,
        }
    }
    fn min_content_height(&self) -> f32 {
        match self.kind {
            MeterKind::Level => 90.0,
            MeterKind::Gauge => 110.0,
        }
    }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let Some(data) = self.data() else { return };
        let frac = fraction(data.value, data.min, data.max);
        let now = ui.input(|i| i.time);
        if frac >= self.peak {
            self.peak = frac;
            self.peak_at = now;
        } else if now - self.peak_at > PEAK_HOLD_S {
            let dt = ui.input(|i| i.stable_dt);
            self.peak = (self.peak - PEAK_FALL * dt).max(frac);
        }

        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
        let mut rect = resp.rect.shrink(2.0);
        if !data.name.is_empty() {
            painter.text(rect.center_top(), Align2::CENTER_TOP, &data.name, FontId::proportional(11.0), theme::TEXT_MUTED);
            rect.min.y += 14.0;
        }
        match self.kind {
            MeterKind::Level => {
                let text = format_value(data.value, data.min, data.max, &data.unit);
                let font = FontId::monospace(11.0);
                if data.vertical {
                    painter.text(rect.center_bottom(), Align2::CENTER_BOTTOM, text, font, theme::TEXT);
                    rect.max.y -= 16.0;
                } else {
                    painter.text(rect.right_center(), Align2::RIGHT_CENTER, &text, font.clone(), theme::TEXT);
                    let width = ui.fonts(|f| f.layout_no_wrap(text, font, theme::TEXT).size().x);
                    rect.max.x -= width + 6.0;
                }
                draw_level(&painter, rect, &data, frac, self.peak);
            }
            MeterKind::Gauge => draw_gauge(&painter, rect, &data, frac),
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        self.data();
        ui.horizontal(|ui| {
            ui.label("Name:");
            if ui.text_edit_singleline(&mut self.name).changed() {
                self.push_config();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Unit:");
            if ui.text_edit_singleline(&mut self.unit).changed() {
                self.push_config();
            }
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fraction_is_clamped_and_an_empty_range_reads_zero() {
        assert_eq!(fraction(5.0, 0.0, 10.0), 0.5);
        assert_eq!(fraction(-3.0, 0.0, 10.0), 0.0);
        assert_eq!(fraction(30.0, 0.0, 10.0), 1.0);
        assert_eq!(fraction(0.0, -10.0, 10.0), 0.5);
        assert_eq!(fraction(1.0, 5.0, 5.0), 0.0);
    }

    #[test]
    fn values_get_decimals_to_suit_their_range() {
        assert_eq!(format_value(0.5, 0.0, 1.0, ""), "0.50");
        assert_eq!(format_value(12.34, 0.0, 50.0, "V"), "12.3 V");
        assert_eq!(format_value(123.4, 0.0, 240.0, "km/h"), "123 km/h");
    }

    #[test]
    fn the_gauge_sweeps_from_bottom_left_over_the_top_to_bottom_right() {
        let c = Pos2::new(100.0, 100.0);
        let start = gauge_point(c, 10.0, 0.0);
        let top = gauge_point(c, 10.0, 0.5);
        let end = gauge_point(c, 10.0, 1.0);
        assert!(start.x < c.x && start.y > c.y, "bottom left");
        assert!((top.x - c.x).abs() < 1e-4 && top.y < c.y, "straight up at the middle");
        assert!(end.x > c.x && end.y > c.y, "bottom right");
        let arc = arc_points(c, 10.0, 0.0, 1.0);
        assert!(arc.len() > 32 && (arc[0] - start).length() < 1e-4);
    }
}
