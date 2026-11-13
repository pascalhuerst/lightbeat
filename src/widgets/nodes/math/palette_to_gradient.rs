use std::any::Any;

use egui::{self, Color32, Pos2, Sense, Stroke, StrokeKind, Ui};

use crate::color::Gradient;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const N: usize = 4;

pub struct PaletteToGradientWidget {
    id: NodeId,
    shared: SharedState,
}

impl PaletteToGradientWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for PaletteToGradientWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Palette to Gradient" }
    fn description(&self) -> &'static str {
        "Builds a 4-stop gradient from a palette and one position per palette colour. \
         Wire `pos1..pos4` to drive stop positions from the graph, or set them in the inspector."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::palette_to_gradient::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::palette_to_gradient::ports().1) }

    fn min_width(&self) -> f32 { 150.0 }
    fn min_content_height(&self) -> f32 { 40.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn overridden_param_indices(&self) -> Vec<usize> {
        // Inspector params are Pos 1..Pos 4 at indices 0..=3; each is
        // overridden by its matching input port at logical index 1..=4.
        let s = self.shared.lock().unwrap();
        let mut hidden = Vec::new();
        for i in 0..N {
            if s.inputs_connected.get(1 + i).copied().unwrap_or(false) {
                hidden.push(i);
            }
        }
        hidden
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        // Read the current gradient output straight from shared outputs
        // and render a compact preview bar.
        let chans: Vec<f32> = {
            let s = self.shared.lock().unwrap();
            s.outputs.iter().copied().take(GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS).collect()
        };

        let avail = ui.available_size();
        let (resp, painter) = ui.allocate_painter(avail, Sense::hover());
        let rect = resp.rect;

        // Checkerboard for alpha (palette-to-gradient stops are alpha=1 but
        // keep the same visual vocabulary as other gradient previews).
        theme::draw_checkerboard(&painter, rect, 5.0);

        let g = Gradient::from_channels(&chans);
        if !g.stops().is_empty() {
            let samples = (rect.width() as usize).max(16).min(320);
            for i in 0..samples {
                let t = i as f32 / (samples - 1).max(1) as f32;
                let x = rect.min.x + (i as f32 / samples as f32) * rect.width();
                let (rgb, alpha) = g.sample_with_alpha(t);
                let c = Color32::from_rgba_unmultiplied(
                    (rgb.r.clamp(0.0, 1.0) * 255.0) as u8,
                    (rgb.g.clamp(0.0, 1.0) * 255.0) as u8,
                    (rgb.b.clamp(0.0, 1.0) * 255.0) as u8,
                    (alpha.clamp(0.0, 1.0) * 255.0) as u8,
                );
                painter.line_segment(
                    [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                    Stroke::new(rect.width() / samples as f32 + 0.5, c),
                );
            }
        } else {
            painter.rect_filled(rect, 2.0, theme::BG);
        }
        painter.rect_stroke(rect, 2.0, Stroke::new(1.0_f32, theme::STROKE), StrokeKind::Inside);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

