use std::any::Any;
use egui::{self, Color32, Ui, Vec2, Sense};

use crate::engine::nodes::math::lookup::{LookupDisplay, LookupMode};
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct LookupWidget {
    id: NodeId,
    shared: SharedState,
    mode: LookupMode,
}

impl LookupWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, mode: LookupMode::Float }
    }
}

impl NodeWidget for LookupWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Lookup" }
    fn title(&self) -> &str { "Lookup" }
    fn description(&self) -> &'static str { "Maps an index to a value or color from an editable lookup table." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("index", PortType::Untyped))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", self.mode.output_type()))]
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 30.0 }
    fn resizable(&self) -> bool { true }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<LookupDisplay>());

        let (mode, table, current, count) = if let Some(d) = display {
            self.mode = d.mode;
            (d.mode, d.table.clone(), d.current_index, d.entry_count)
        } else {
            (self.mode, vec![], 0, 0)
        };
        drop(shared);

        if count == 0 {
            ui.label("Empty");
            return;
        }

        let cpe = mode.channels_per_entry();
        let w = ui.available_width();
        let entry_w = (w / count as f32).max(8.0);
        let h = ui.available_height().max(20.0);

        let (response, painter) = ui.allocate_painter(Vec2::new(w, h), Sense::hover());
        let rect = response.rect;

        for i in 0..count {
            let x = rect.min.x + i as f32 * entry_w;
            let entry_rect = egui::Rect::from_min_size(
                egui::pos2(x, rect.min.y),
                Vec2::new(entry_w, h),
            );

            let base = i * cpe;
            let color = match mode {
                LookupMode::Float => {
                    let v = table.get(base).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                    Color32::from_rgb(
                        (v * 200.0 + 40.0) as u8,
                        (v * 200.0 + 40.0) as u8,
                        255,
                    )
                }
                LookupMode::Color => {
                    let r = table.get(base).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                    let g = table.get(base + 1).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                    let b = table.get(base + 2).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
                }
            };

            let is_current = i == current;
            let fill = if is_current { color } else { color.linear_multiply(0.5) };
            painter.rect_filled(entry_rect, 1.0, fill);

            if is_current {
                painter.rect_stroke(
                    entry_rect, 1.0,
                    egui::Stroke::new(2.0, Color32::WHITE),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<LookupDisplay>());

        let (mode, table, count) = if let Some(d) = display {
            (d.mode, d.table.clone(), d.entry_count)
        } else {
            return;
        };
        drop(shared);

        ui.label(egui::RichText::new("Table").strong());

        let cpe = mode.channels_per_entry();

        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for i in 0..count {
                let base = i * cpe;
                ui.horizontal(|ui| {
                    ui.label(format!("{}:", i));
                    match mode {
                        LookupMode::Float => {
                            let mut v = table.get(base).copied().unwrap_or(0.0);
                            if ui.add(egui::DragValue::new(&mut v).range(0.0..=1.0).speed(0.01)).changed() {
                                self.shared.lock().unwrap()
                                    .pending_params.push((100 + base, ParamValue::Float(v)));
                            }
                        }
                        LookupMode::Color => {
                            let r = table.get(base).copied().unwrap_or(0.0);
                            let g = table.get(base + 1).copied().unwrap_or(0.0);
                            let b = table.get(base + 2).copied().unwrap_or(0.0);

                            let mut color = [r, g, b];
                            if ui.color_edit_button_rgb(&mut color).changed() {
                                let mut shared = self.shared.lock().unwrap();
                                shared.pending_params.push((100 + base, ParamValue::Float(color[0])));
                                shared.pending_params.push((100 + base + 1, ParamValue::Float(color[1])));
                                shared.pending_params.push((100 + base + 2, ParamValue::Float(color[2])));
                            }
                        }
                    }
                });
            }
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
