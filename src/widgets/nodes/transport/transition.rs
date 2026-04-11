use std::any::Any;
use egui::{self, Color32, Pos2, Sense, Stroke, Ui, Vec2};

use crate::engine::nodes::transport::easing::EasingCurve;
use crate::engine::nodes::transport::transition::{TransitionDisplay, TransitionMode};
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct TransitionWidget {
    id: NodeId,
    shared: SharedState,
    mode: TransitionMode,
}

impl TransitionWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, mode: TransitionMode::Color }
    }

    fn build_inputs(mode: TransitionMode) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("trigger", PortType::Logic)),
            UiPortDef::from_def(&PortDef::new("phase", PortType::Phase)),
            UiPortDef::from_def(&PortDef::new("value", mode.value_type())),
        ]
    }

    fn build_outputs(mode: TransitionMode) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", mode.value_type()))]
    }
}

const CURVE_SEGMENTS: usize = 30;

impl NodeWidget for TransitionWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Transition" }
    fn title(&self) -> &str { "Transition" }

    fn ui_inputs(&self) -> Vec<UiPortDef> { Self::build_inputs(self.mode) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { Self::build_outputs(self.mode) }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 50.0 }
    fn resizable(&self) -> bool { true }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<TransitionDisplay>());

        let (mode, curve, phase, active) = if let Some(d) = display {
            self.mode = d.mode;
            (d.mode, d.curve, d.phase, d.active)
        } else {
            (self.mode, EasingCurve::Linear, 0.0, false)
        };
        drop(shared);

        // Draw the easing curve.
        let w = ui.available_width();
        let h = ui.available_height().max(30.0);
        let (response, painter) = ui.allocate_painter(Vec2::new(w, h), Sense::hover());
        let rect = response.rect;

        painter.rect_filled(rect, 2.0, Color32::from_gray(25));

        // Draw curve.
        let curve_color = Color32::from_rgb(80, 200, 160);
        let mut prev: Option<Pos2> = None;
        for i in 0..=CURVE_SEGMENTS {
            let t = i as f32 / CURVE_SEGMENTS as f32;
            let v = curve.apply(t);
            let x = rect.min.x + t * rect.width();
            let y = rect.max.y - v.clamp(0.0, 1.0) * rect.height();
            let pos = Pos2::new(x, y);
            if let Some(p) = prev {
                painter.line_segment([p, pos], Stroke::new(1.5, curve_color));
            }
            prev = Some(pos);
        }

        // Draw progress indicator.
        if active {
            let x = rect.min.x + phase * rect.width();
            let v = curve.apply(phase);
            let y = rect.max.y - v.clamp(0.0, 1.0) * rect.height();
            painter.circle_filled(Pos2::new(x, y), 4.0, Color32::WHITE);
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
