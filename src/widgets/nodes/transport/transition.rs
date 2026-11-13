use egui::{self, Pos2, Sense, Stroke, Ui, Vec2};

use crate::engine::nodes::transport::easing::EasingCurve;
use crate::engine::nodes::transport::transition::{ports_for, TransitionDisplay, TransitionMode};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeWidget, param_usize, node_identity};
use crate::widgets::nodes::types::UiPortDef;

pub struct TransitionWidget {
    id: NodeId,
    shared: SharedState,
    mode: TransitionMode,
}

impl TransitionWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, mode: TransitionMode::Neutral }
    }

    fn push_mode(&self, mode: TransitionMode) {
        let mut s = self.shared.lock();
        s.pending_params.push((0, ParamValue::Choice(mode.to_index())));
    }

    /// Map a port type seen on the value input or out output to a mode.
    fn infer_mode(t: PortType) -> Option<TransitionMode> {
        match t {
            PortType::Untyped | PortType::Phase | PortType::Logic => Some(TransitionMode::Float),
            PortType::Color => Some(TransitionMode::Color),
            PortType::Palette => Some(TransitionMode::Palette),
            PortType::Position => Some(TransitionMode::Position),
            PortType::Gradient => Some(TransitionMode::Gradient),
            _ => None,
        }
    }
}

const CURVE_SEGMENTS: usize = 30;

impl NodeWidget for TransitionWidget {
    fn restore_from_save(&mut self, _data: Option<&serde_json::Value>, params: &[(usize, ParamValue)]) {
        self.mode = param_usize(params, 0).map(TransitionMode::from_index).unwrap_or(TransitionMode::Neutral);
    }

    node_identity!("Transition");
    fn description(&self) -> &'static str { "Smoothly interpolates between values using an easing curve when triggered; the phase input moves it along. With Type set to Phase out there is no value: the output is the eased phase itself, a curve shaper that a trigger starts. For a time instead of a phase, drive it with Time to Phase." }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&ports_for(self.mode).0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&ports_for(self.mode).1) }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 50.0 }
    fn resizable(&self) -> bool { true }


    fn on_ui_connect(&mut self, input_port: usize, source_type: PortType) {
        // Only the `value` port (index 0) determines mode.
        if input_port != 0 || self.mode != TransitionMode::Neutral { return; }
        if let Some(new_mode) = Self::infer_mode(source_type) {
            self.mode = new_mode;
            self.push_mode(new_mode);
        }
    }
    fn on_ui_output_connect(&mut self, output_port: usize, dest_type: PortType) {
        if output_port != 0 || self.mode != TransitionMode::Neutral { return; }
        if let Some(new_mode) = Self::infer_mode(dest_type) {
            self.mode = new_mode;
            self.push_mode(new_mode);
        }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<TransitionDisplay>());

        let (_mode, curve, phase, active) = if let Some(d) = display {
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

        painter.rect_filled(rect, 2.0, theme::PLOT_BG);

        // Draw curve.
        let curve_color = theme::CURVE;
        let mut prev: Option<Pos2> = None;
        for i in 0..=CURVE_SEGMENTS {
            let t = i as f32 / CURVE_SEGMENTS as f32;
            let v = curve.apply(t);
            let x = rect.min.x + t * rect.width();
            let y = rect.max.y - v.clamp(0.0, 1.0) * rect.height();
            let pos = Pos2::new(x, y);
            if let Some(p) = prev {
                painter.line_segment([p, pos], Stroke::new(1.5_f32, curve_color));
            }
            prev = Some(pos);
        }

        // Draw progress indicator.
        if active {
            let x = rect.min.x + phase * rect.width();
            let v = curve.apply(phase);
            let y = rect.max.y - v.clamp(0.0, 1.0) * rect.height();
            painter.circle_filled(Pos2::new(x, y), 4.0, theme::TEXT_BRIGHT);
        }
    }
}
