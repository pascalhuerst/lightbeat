use std::any::Any;

use egui::{self, Color32, Pos2, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::color::Gradient;
use crate::engine::nodes::display::color_display::{
    ColorDisplayData, MODE_COLOR, MODE_GRADIENT, MODE_NEUTRAL, MODE_PALETTE,
};
use crate::engine::types::*;
use crate::objects::color_palette::PALETTE_SIZE;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct ColorDisplayWidget {
    id: NodeId,
    shared: SharedState,
    mode: usize,
}

impl ColorDisplayWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, mode: MODE_NEUTRAL }
    }

    fn input_defs(&self) -> Vec<PortDef> {
        match self.mode {
            MODE_PALETTE => vec![PortDef::new("palette", PortType::Palette)],
            MODE_COLOR => vec![PortDef::new("color", PortType::Color)],
            MODE_GRADIENT => vec![PortDef::new("gradient", PortType::Gradient)],
            _ => vec![PortDef::new("?", PortType::Any)],
        }
    }

    fn push_mode(&self, mode: usize) {
        let mut s = self.shared.lock().unwrap();
        s.pending_params.push((0, ParamValue::Choice(mode)));
    }
}

impl NodeWidget for ColorDisplayWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color Display" }
    fn title(&self) -> &str { "Color Display" }
    fn description(&self) -> &'static str {
        "Shows a color swatch, palette (4-color set), or gradient preview. Mode auto-detected from the first connected wire."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.input_defs().iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 60.0 }
    fn resizable(&self) -> bool { true }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn on_ui_connect(&mut self, _input_port: usize, source_type: PortType) {
        if self.mode != MODE_NEUTRAL { return; }
        let new_mode = match source_type {
            PortType::Palette => MODE_PALETTE,
            PortType::Color => MODE_COLOR,
            PortType::Gradient => MODE_GRADIENT,
            _ => return,
        };
        self.mode = new_mode;
        self.push_mode(new_mode);
    }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<ColorDisplayData>());

        let (mode, channels) = if let Some(d) = display {
            (d.mode, d.channels)
        } else {
            (MODE_NEUTRAL, [0.0; GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS])
        };
        drop(shared);

        self.mode = mode;

        let w = ui.available_width();
        let h = ui.available_height().max(4.0);

        if mode == MODE_NEUTRAL {
            ui.colored_label(Color32::from_gray(120), "Connect a color, palette, or gradient");
            return;
        }

        match mode {
            MODE_PALETTE => {
                let (response, painter) = ui.allocate_painter(Vec2::new(w, h), Sense::hover());
                let rect = response.rect;
                let bar_h = rect.height() / PALETTE_SIZE as f32;

                for i in 0..PALETTE_SIZE {
                    let base = i * 3;
                    let r = channels[base];
                    let g = channels[base + 1];
                    let b = channels[base + 2];
                    let color = Color32::from_rgb(
                        (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8,
                    );
                    let bar_rect = egui::Rect::from_min_size(
                        egui::pos2(rect.min.x, rect.min.y + i as f32 * bar_h),
                        Vec2::new(rect.width(), bar_h),
                    );
                    painter.rect_filled(bar_rect, 0.0, color);
                }
            }
            MODE_GRADIENT => {
                let (response, painter) = ui.allocate_painter(Vec2::new(w, h), Sense::hover());
                let rect = response.rect;

                draw_checker(&painter, rect);

                let gradient = Gradient::from_channels(&channels);
                let stops = gradient.stops();
                if stops.is_empty() {
                    painter.rect_filled(rect, 2.0, Color32::from_gray(40));
                } else {
                    let samples = (rect.width() as usize).max(16).min(512);
                    for i in 0..samples {
                        let t = i as f32 / (samples - 1).max(1) as f32;
                        let x = rect.min.x + (i as f32 / samples as f32) * rect.width();
                        let (rgb, alpha) = gradient.sample_with_alpha(t);
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

                    for s in stops {
                        let x = rect.min.x + s.position.clamp(0.0, 1.0) * rect.width();
                        let y = rect.min.y;
                        let tri = [
                            Pos2::new(x, y),
                            Pos2::new(x - 3.0, y - 5.0),
                            Pos2::new(x + 3.0, y - 5.0),
                        ];
                        let marker_col = Color32::from_rgb(
                            (s.color.r.clamp(0.0, 1.0) * 255.0) as u8,
                            (s.color.g.clamp(0.0, 1.0) * 255.0) as u8,
                            (s.color.b.clamp(0.0, 1.0) * 255.0) as u8,
                        );
                        painter.add(egui::Shape::convex_polygon(
                            tri.to_vec(), marker_col, Stroke::new(0.5, Color32::BLACK),
                        ));
                    }
                }

                painter.rect_stroke(rect, 2.0, Stroke::new(1.0, Color32::from_gray(80)), StrokeKind::Inside);
            }
            _ => {
                // Single color mode.
                let r = channels[0];
                let g = channels[1];
                let b = channels[2];
                let color = Color32::from_rgb(
                    (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8,
                );

                let (response, painter) = ui.allocate_painter(Vec2::new(w, h), Sense::hover());
                let rect = response.rect;
                painter.rect_filled(rect, 4.0, color);

                let luma = r * 0.299 + g * 0.587 + b * 0.114;
                let text_color = if luma > 0.5 { Color32::BLACK } else { Color32::WHITE };
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("#{:02X}{:02X}{:02X}", (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8),
                    egui::FontId::monospace(11.0 * zoom),
                    text_color,
                );
            }
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

fn draw_checker(painter: &egui::Painter, rect: egui::Rect) {
    let cell = 6.0;
    let cols = (rect.width() / cell).ceil() as i32;
    let rows = (rect.height() / cell).ceil() as i32;
    let c1 = Color32::from_gray(40);
    let c2 = Color32::from_gray(70);
    for y in 0..rows {
        for x in 0..cols {
            let color = if (x + y) % 2 == 0 { c1 } else { c2 };
            let cell_rect = egui::Rect::from_min_size(
                Pos2::new(rect.min.x + x as f32 * cell, rect.min.y + y as f32 * cell),
                Vec2::splat(cell),
            ).intersect(rect);
            painter.rect_filled(cell_rect, 0.0, color);
        }
    }
}
