use egui::{self, Color32, Pos2, Sense, Stroke, StrokeKind, Ui, Vec2};
use egui_extras::{Column, TableBuilder};

use crate::color::Gradient;
use crate::objects::gradient_preset::GradientPreset;

pub struct GradientPresetManager {
    pub presets: Vec<GradientPreset>,
    next_id: u32,
}

impl GradientPresetManager {
    pub fn new() -> Self {
        Self { presets: Vec::new(), next_id: 1 }
    }

    pub fn from_presets(presets: Vec<GradientPreset>) -> Self {
        let next_id = presets.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        Self { presets, next_id }
    }

    /// Reserve and return a fresh id for a preset added externally
    /// (e.g. via "Save current as preset" in the Gradient Source widget).
    pub fn next_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn show(&mut self, ui: &mut Ui) {
        ui.heading("Gradients");
        ui.colored_label(
            Color32::from_gray(140),
            "Reusable gradient presets. Author or edit them by loading into a Gradient Source node, then use \"Save preset\" there to write back here.",
        );
        ui.separator();

        if self.presets.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No gradient presets yet.");
        } else {
            let mut remove_id = None;
            TableBuilder::new(ui)
                .striped(true)
                .column(Column::remainder().clip(true).at_least(120.0)) // name
                .column(Column::exact(220.0))                            // preview
                .column(Column::exact(24.0))                             // delete
                .header(20.0, |mut header| {
                    header.col(|ui| { ui.strong("Name"); });
                    header.col(|ui| { ui.strong("Preview"); });
                    header.col(|_ui| {});
                })
                .body(|mut body| {
                    for preset in &mut self.presets {
                        body.row(28.0, |mut row| {
                            row.col(|ui| {
                                ui.add_sized(
                                    [ui.available_width(), 22.0],
                                    egui::TextEdit::singleline(&mut preset.name)
                                        .id_salt(("preset_name", preset.id)),
                                );
                            });
                            row.col(|ui| {
                                draw_preset_preview(ui, &preset.stops);
                            });
                            row.col(|ui| {
                                if ui.small_button(egui_phosphor::regular::X).clicked() {
                                    remove_id = Some(preset.id);
                                }
                            });
                        });
                    }
                });
            if let Some(id) = remove_id {
                self.presets.retain(|p| p.id != id);
            }
        }

        ui.separator();
        if ui.button("Add Empty Preset").clicked() {
            let id = self.next_id();
            self.presets.push(GradientPreset::new(id, format!("Gradient {}", id)));
        }
    }
}

fn draw_preset_preview(ui: &mut Ui, stops: &[crate::color::GradientStop]) {
    let (resp, painter) = ui.allocate_painter(Vec2::new(220.0, 22.0), Sense::hover());
    let rect = resp.rect;

    // Checkerboard so alpha is visible.
    let cell = 5.0;
    let cols = (rect.width() / cell).ceil() as i32;
    let rows = (rect.height() / cell).ceil() as i32;
    for y in 0..rows {
        for x in 0..cols {
            let c = if (x + y) % 2 == 0 {
                Color32::from_gray(40)
            } else {
                Color32::from_gray(70)
            };
            let cell_rect = egui::Rect::from_min_size(
                Pos2::new(rect.min.x + x as f32 * cell, rect.min.y + y as f32 * cell),
                Vec2::splat(cell),
            ).intersect(rect);
            painter.rect_filled(cell_rect, 0.0, c);
        }
    }

    if !stops.is_empty() {
        let g = Gradient::new(stops.to_vec());
        let samples = (rect.width() as usize).max(16).min(256);
        for i in 0..samples {
            let t = i as f32 / (samples - 1).max(1) as f32;
            let x = rect.min.x + (i as f32 / samples as f32) * rect.width();
            let (rgb, alpha) = g.sample_with_alpha(t);
            let col = Color32::from_rgba_unmultiplied(
                (rgb.r.clamp(0.0, 1.0) * 255.0) as u8,
                (rgb.g.clamp(0.0, 1.0) * 255.0) as u8,
                (rgb.b.clamp(0.0, 1.0) * 255.0) as u8,
                (alpha.clamp(0.0, 1.0) * 255.0) as u8,
            );
            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(rect.width() / samples as f32 + 0.5, col),
            );
        }
    }
    painter.rect_stroke(rect, 2.0, Stroke::new(1.0, Color32::from_gray(80)), StrokeKind::Inside);
}
