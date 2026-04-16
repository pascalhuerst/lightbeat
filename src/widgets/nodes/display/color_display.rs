use std::any::Any;

use egui::{self, Sense, Ui, Vec2};

use crate::engine::nodes::display::color_display::{
    ColorDisplayData, MODE_COLOR, MODE_NEUTRAL, MODE_PALETTE,
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
    fn description(&self) -> &'static str { "Shows a color swatch or a palette (4-color set) as a preview." }

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
            (MODE_NEUTRAL, [0.0; 12])
        };
        drop(shared);

        self.mode = mode;

        let w = ui.available_width();
        let h = ui.available_height().max(4.0);

        if mode == MODE_NEUTRAL {
            ui.colored_label(egui::Color32::from_gray(120), "Connect a color or palette");
            return;
        }

        match mode {
            MODE_PALETTE => {
                // Palette mode: 4 color bars vertically.
                let (response, painter) = ui.allocate_painter(Vec2::new(w, h), Sense::hover());
                let rect = response.rect;
                let bar_h = rect.height() / PALETTE_SIZE as f32;

                for i in 0..PALETTE_SIZE {
                    let base = i * 3;
                    let r = channels[base];
                    let g = channels[base + 1];
                    let b = channels[base + 2];
                    let color = egui::Color32::from_rgb(
                        (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8,
                    );
                    let bar_rect = egui::Rect::from_min_size(
                        egui::pos2(rect.min.x, rect.min.y + i as f32 * bar_h),
                        Vec2::new(rect.width(), bar_h),
                    );
                    painter.rect_filled(bar_rect, 0.0, color);
                }
            }
            _ => {
                // Single color mode.
                let r = channels[0];
                let g = channels[1];
                let b = channels[2];
                let color = egui::Color32::from_rgb(
                    (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8,
                );

                let (response, painter) = ui.allocate_painter(Vec2::new(w, h), Sense::hover());
                let rect = response.rect;
                painter.rect_filled(rect, 4.0, color);

                let luma = r * 0.299 + g * 0.587 + b * 0.114;
                let text_color = if luma > 0.5 { egui::Color32::BLACK } else { egui::Color32::WHITE };
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
