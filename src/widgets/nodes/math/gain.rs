use std::any::Any;

use egui::{self, Pos2, Sense, Stroke, Ui};

use crate::engine::nodes::math::gain::GainDisplay;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

/// Knob sweep: 0 at bottom-left, 1 at bottom-right, going the long way
/// around (~270°). Standard audio-knob layout.
const KNOB_START_ANGLE: f32 = std::f32::consts::PI * 0.75;     // bottom-left
const KNOB_SWEEP: f32 = std::f32::consts::PI * 1.5;            // 270°
/// Pixels of vertical drag for a full 0→1 sweep. Small so fine gain
/// adjustments are easy even on a small knob.
const DRAG_PIXELS_FOR_FULL_SWEEP: f32 = 120.0;

pub struct GainWidget {
    id: NodeId,
    shared: SharedState,
    gain: f32,
    output: f32,
}

impl GainWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, gain: 1.0, output: 0.0 }
    }
}

impl NodeWidget for GainWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Gain" }
    fn description(&self) -> &'static str {
        "Multiplies the input by a 0..1 scalar. Drag the knob to adjust; \
         double-click to reset to unity (1.0)."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("in", PortType::Untyped))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", PortType::Untyped))]
    }

    fn min_width(&self) -> f32 { 90.0 }
    fn min_content_height(&self) -> f32 { 70.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        // Sync from engine.
        {
            let shared = self.shared.lock().unwrap();
            if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<GainDisplay>()) {
                self.gain = d.gain;
                self.output = d.output;
            }
        }

        let avail = ui.available_size();
        if avail.x <= 0.0 || avail.y <= 0.0 { return; }
        let (rect, resp) = ui.allocate_exact_size(avail, Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        // Leave a bit of space below the knob for the value text.
        let text_h = 14.0 * zoom;
        let knob_area = egui::Rect::from_min_max(
            rect.min,
            Pos2::new(rect.max.x, (rect.max.y - text_h).max(rect.min.y)),
        );
        let center = knob_area.center();
        let r = (knob_area.width().min(knob_area.height()) * 0.5 - 2.0 * zoom).max(6.0 * zoom);

        // Track arc (empty part — full sweep as a dim ring).
        painter.circle_stroke(center, r, Stroke::new(2.0 * zoom.max(0.5), theme::STROKE));

        // Filled arc from 0 up to current value.
        let v = self.gain.clamp(0.0, 1.0);
        let end_angle = KNOB_START_ANGLE + KNOB_SWEEP * v;
        let segments = 24;
        let mut pts: Vec<Pos2> = Vec::with_capacity(segments + 1);
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let a = KNOB_START_ANGLE + (end_angle - KNOB_START_ANGLE) * t;
            pts.push(Pos2::new(center.x + a.cos() * r, center.y + a.sin() * r));
        }
        for w in pts.windows(2) {
            painter.line_segment([w[0], w[1]],
                Stroke::new(2.5 * zoom.max(0.5), theme::SEM_PRIMARY));
        }

        // Indicator needle from centre to the arc at the current value.
        let needle_inner = r * 0.25;
        let needle_outer = r * 0.9;
        let ia = Pos2::new(
            center.x + end_angle.cos() * needle_inner,
            center.y + end_angle.sin() * needle_inner,
        );
        let oa = Pos2::new(
            center.x + end_angle.cos() * needle_outer,
            center.y + end_angle.sin() * needle_outer,
        );
        painter.line_segment([ia, oa], Stroke::new(2.0 * zoom.max(0.5), theme::TEXT_BRIGHT));

        // Centre cap.
        painter.circle_filled(center, (r * 0.15).max(1.5 * zoom), theme::BG_HIGH);
        painter.circle_stroke(center, (r * 0.15).max(1.5 * zoom),
            Stroke::new(1.0_f32, theme::STROKE));

        // Value text below the knob.
        painter.text(
            Pos2::new(rect.center().x, rect.max.y - text_h * 0.5),
            egui::Align2::CENTER_CENTER,
            format!("{:.2}", self.gain),
            egui::FontId::monospace(10.0 * zoom.max(0.5)),
            theme::TEXT_MUTED,
        );

        // Interaction: vertical drag changes value (up = louder).
        if resp.double_clicked() {
            self.shared.lock().unwrap()
                .pending_params.push((0, ParamValue::Float(1.0)));
        } else if resp.dragged() {
            let dy = resp.drag_delta().y;
            if dy.abs() > 0.0 {
                let delta_v = -dy / (DRAG_PIXELS_FOR_FULL_SWEEP * zoom.max(0.5));
                let new_v = (self.gain + delta_v).clamp(0.0, 1.0);
                self.shared.lock().unwrap()
                    .pending_params.push((0, ParamValue::Float(new_v)));
            }
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
