use egui::{self, Color32, Pos2, Sense, Stroke, StrokeKind, Ui};

use crate::color::Gradient;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeWidget, node_identity};
use crate::widgets::nodes::types::UiPortDef;

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
    node_identity!("Palette to Gradient");
    fn description(&self) -> &'static str {
        "Builds a 4-stop gradient from a palette and one position per palette colour. \
         A new node has the four positions as inputs (`Pos 1..Pos 4`); switch them off in the inspector to set them by hand."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::palette_to_gradient::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::palette_to_gradient::ports().1) }

    fn min_width(&self) -> f32 { 150.0 }
    fn min_content_height(&self) -> f32 { 40.0 }
    fn resizable(&self) -> bool { true }

    fn default_promoted(&self) -> &'static [(&'static str, PromotedKind)] {
        &[
            ("Pos 1", PromotedKind::Float),
            ("Pos 2", PromotedKind::Float),
            ("Pos 3", PromotedKind::Float),
            ("Pos 4", PromotedKind::Float),
        ]
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        // Read the current gradient output straight from shared outputs
        // and render a compact preview bar.
        let chans: Vec<f32> = {
            let s = self.shared.lock();
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
            let samples = (rect.width() as usize).clamp(16, 320);
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
}

