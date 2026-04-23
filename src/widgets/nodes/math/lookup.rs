//! Lookup table widget.
//!
//! The widget is the source of truth for columns, rows, and cell data.
//! Any edit pushes a full config JSON via `pending_config`; the engine's
//! `load_data` replaces its state on the next tick. The engine's display
//! feeds back the `current_row` for the body preview.

use std::any::Any;

use egui::{self, Color32, Pos2, Ui, Vec2, Sense, Stroke, StrokeKind};
use egui_extras::{Column, TableBuilder};

use crate::color::{Gradient, GradientStop};
use crate::engine::nodes::math::lookup::{LookupColumn, LookupDisplay, port_type_to_str};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::math::gradient_source::SharedGradientLibrary;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const MAX_COLUMNS: usize = 8;
const MIN_ROWS: usize = 1;
const MAX_ROWS: usize = 64;

/// `PortType`s available as Lookup columns. Gradient cells are populated
/// from the shared gradient library — the cell editor opens a popup with
/// the available presets; in-place stop editing is intentionally left to
/// the dedicated Gradient Source node.
const COLUMN_TYPES: &[PortType] = &[
    PortType::Untyped,
    PortType::Logic,
    PortType::Phase,
    PortType::Color,
    PortType::Position,
    PortType::Palette,
    PortType::Gradient,
];

fn column_type_label(pt: PortType) -> &'static str {
    match pt {
        PortType::Untyped => "Untyped",
        PortType::Logic => "Logic",
        PortType::Phase => "Phase",
        PortType::Color => "Color",
        PortType::Position => "Position",
        PortType::Palette => "Palette",
        PortType::Gradient => "Gradient",
        _ => "—",
    }
}

pub struct LookupWidget {
    id: NodeId,
    shared: SharedState,
    /// Shared gradient library — read for Gradient cell pickers.
    library: SharedGradientLibrary,
    /// Authoritative table state. Every edit pushes a full config to the
    /// engine so inputs() / outputs() on the engine side stay in lockstep.
    columns: Vec<LookupColumn>,
    data: Vec<f32>,
    row_count: usize,
    /// Mirrored from engine display so the body can highlight the live row.
    current_row: usize,
}

impl LookupWidget {
    pub fn new(id: NodeId, shared: SharedState, library: SharedGradientLibrary) -> Self {
        let columns = vec![LookupColumn {
            name: "value".into(),
            port_type: PortType::Untyped,
        }];
        let row_count = 4;
        let data = vec![0.0, 0.25, 0.5, 1.0];
        Self { id, shared, library, columns, data, row_count, current_row: 0 }
    }

    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Some(arr) = data.get("columns").and_then(|v| v.as_array()) {
            let mut cols = Vec::with_capacity(arr.len());
            for entry in arr {
                let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("col").to_string();
                let port_type = entry.get("type")
                    .and_then(|v| v.as_str())
                    .and_then(crate::engine::nodes::math::lookup::port_type_from_str)
                    .unwrap_or(PortType::Untyped);
                cols.push(LookupColumn { name, port_type });
            }
            self.columns = cols;
        }
        if let Some(n) = data.get("row_count").and_then(|v| v.as_u64()) {
            self.row_count = n as usize;
        }
        if let Some(arr) = data.get("data").and_then(|v| v.as_array()) {
            self.data = arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect();
        }
        self.fix_data_size();
    }

    fn row_stride(&self) -> usize {
        self.columns.iter().map(|c| c.port_type.channel_count()).sum()
    }

    fn column_channel_offset(&self, col_idx: usize) -> usize {
        self.columns.iter().take(col_idx).map(|c| c.port_type.channel_count()).sum()
    }

    fn fix_data_size(&mut self) {
        let expected = self.row_count * self.row_stride();
        if self.data.len() < expected {
            self.data.resize(expected, 0.0);
        } else if self.data.len() > expected {
            self.data.truncate(expected);
        }
    }

    fn push_config(&self) {
        let cfg = serde_json::json!({
            "columns": self.columns.iter().map(|c| serde_json::json!({
                "name": c.name,
                "type": port_type_to_str(c.port_type),
            })).collect::<Vec<_>>(),
            "row_count": self.row_count,
            "data": self.data,
        });
        let mut s = self.shared.lock().unwrap();
        s.pending_config = Some(cfg);
    }

    fn add_column(&mut self) {
        if self.columns.len() >= MAX_COLUMNS { return; }
        let new_col = LookupColumn {
            name: format!("col{}", self.columns.len() + 1),
            port_type: PortType::Untyped,
        };
        let new_cc = new_col.port_type.channel_count();
        let old_stride = self.row_stride();
        let new_stride = old_stride + new_cc;
        let mut new_data = vec![0.0f32; self.row_count * new_stride];
        for r in 0..self.row_count {
            let src = r * old_stride;
            let dst = r * new_stride;
            for k in 0..old_stride {
                new_data[dst + k] = self.data.get(src + k).copied().unwrap_or(0.0);
            }
        }
        self.columns.push(new_col);
        self.data = new_data;
    }

    fn remove_column(&mut self, col_idx: usize) {
        if col_idx >= self.columns.len() || self.columns.len() == 1 { return; }
        let off = self.column_channel_offset(col_idx);
        let cc = self.columns[col_idx].port_type.channel_count();
        let old_stride = self.row_stride();
        let new_stride = old_stride - cc;
        let mut new_data = vec![0.0f32; self.row_count * new_stride];
        for r in 0..self.row_count {
            let src = r * old_stride;
            let dst = r * new_stride;
            for k in 0..off {
                new_data[dst + k] = self.data[src + k];
            }
            for k in 0..(old_stride - off - cc) {
                new_data[dst + off + k] = self.data[src + off + cc + k];
            }
        }
        self.columns.remove(col_idx);
        self.data = new_data;
    }

    fn set_column_type(&mut self, col_idx: usize, new_type: PortType) {
        if col_idx >= self.columns.len() { return; }
        if self.columns[col_idx].port_type == new_type { return; }
        let off = self.column_channel_offset(col_idx);
        let old_cc = self.columns[col_idx].port_type.channel_count();
        let new_cc = new_type.channel_count();
        let old_stride = self.row_stride();
        let new_stride = old_stride - old_cc + new_cc;
        let mut new_data = vec![0.0f32; self.row_count * new_stride];
        for r in 0..self.row_count {
            let src = r * old_stride;
            let dst = r * new_stride;
            for k in 0..off {
                new_data[dst + k] = self.data[src + k];
            }
            // Cells for the retyped column are cleared — old values don't
            // carry meaningful semantics in the new type.
            let tail = old_stride - off - old_cc;
            for k in 0..tail {
                new_data[dst + off + new_cc + k] = self.data[src + off + old_cc + k];
            }
        }
        self.columns[col_idx].port_type = new_type;
        self.data = new_data;
    }

    fn add_row(&mut self) {
        if self.row_count >= MAX_ROWS { return; }
        self.row_count += 1;
        self.fix_data_size();
    }

    fn remove_row(&mut self, row_idx: usize) {
        if self.row_count <= MIN_ROWS || row_idx >= self.row_count { return; }
        let stride = self.row_stride();
        let start = row_idx * stride;
        self.data.drain(start..start + stride);
        self.row_count -= 1;
    }

    fn sync_current_row_from_display(&mut self) {
        let s = self.shared.lock().unwrap();
        if let Some(d) = s.display.as_ref().and_then(|d| d.downcast_ref::<LookupDisplay>()) {
            self.current_row = d.current_row;
        }
    }
}

impl NodeWidget for LookupWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Lookup" }
    fn title(&self) -> &str { "Lookup" }
    fn description(&self) -> &'static str {
        "Table with one row selected by the `index` input. Each column becomes its own typed output (Untyped / Logic / Phase / Color / Position / Palette)."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("index", PortType::Untyped))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        // "rows" is always at index 0 so its position is stable when columns
        // are added/removed — wires from it survive schema edits.
        std::iter::once(UiPortDef::from_def(&PortDef::new("rows", PortType::Untyped)))
            .chain(self.columns.iter()
                .map(|c| UiPortDef::from_def(&PortDef::new(c.name.clone(), c.port_type))))
            .collect()
    }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 30.0 }
    fn resizable(&self) -> bool { true }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.sync_current_row_from_display();
        let n = self.row_count;
        if n == 0 {
            ui.label("Empty");
            return;
        }

        let w = ui.available_width();
        let h = ui.available_height().max(20.0);
        let (resp, painter) = ui.allocate_painter(Vec2::new(w, h), Sense::hover());
        let rect = resp.rect;

        let row_h = (h / n as f32).max(2.0);
        let cols_n = self.columns.len().max(1);
        let stride = self.row_stride();

        for r in 0..n {
            let y = rect.min.y + r as f32 * row_h;
            let row_rect = egui::Rect::from_min_size(
                egui::pos2(rect.min.x, y),
                Vec2::new(rect.width(), row_h),
            );
            let col_w = rect.width() / cols_n as f32;
            for (ci, col) in self.columns.iter().enumerate() {
                let off = self.column_channel_offset(ci);
                let base = r * stride + off;
                let cell_color = preview_color(col.port_type, &self.data, base);
                let cell_rect = egui::Rect::from_min_size(
                    egui::pos2(row_rect.min.x + ci as f32 * col_w, row_rect.min.y),
                    Vec2::new(col_w, row_h),
                );
                painter.rect_filled(cell_rect, 0.0, cell_color);
            }
            if r == self.current_row {
                painter.rect_stroke(
                    row_rect, 1.0,
                    Stroke::new(2.0, Color32::WHITE),
                    StrokeKind::Inside,
                );
            }
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        self.sync_current_row_from_display();
        let mut changed = false;

        // -- Columns --
        ui.label(egui::RichText::new("Columns").strong());
        let mut remove_col: Option<usize> = None;
        let mut retype: Option<(usize, PortType)> = None;
        let cols_snapshot: Vec<(String, PortType)> =
            self.columns.iter().map(|c| (c.name.clone(), c.port_type)).collect();
        for (ci, (name, pt)) in cols_snapshot.iter().enumerate() {
            ui.horizontal(|ui| {
                let mut name = name.clone();
                if ui.add(
                    egui::TextEdit::singleline(&mut name)
                        .id_salt(("lookup_col_name", self.id.0, ci))
                        .desired_width(100.0),
                ).changed() {
                    self.columns[ci].name = name;
                    changed = true;
                }
                let mut new_pt = *pt;
                egui::ComboBox::from_id_salt(("lookup_col_type", self.id.0, ci))
                    .selected_text(column_type_label(*pt))
                    .show_ui(ui, |ui| {
                        for &opt in COLUMN_TYPES {
                            ui.selectable_value(&mut new_pt, opt, column_type_label(opt));
                        }
                    });
                if new_pt != *pt {
                    retype = Some((ci, new_pt));
                }
                ui.add_enabled_ui(self.columns.len() > 1, |ui| {
                    if ui.small_button(egui_phosphor::regular::X).clicked() {
                        remove_col = Some(ci);
                    }
                });
            });
        }
        if let Some((ci, pt)) = retype {
            self.set_column_type(ci, pt);
            changed = true;
        }
        if let Some(ci) = remove_col {
            self.remove_column(ci);
            changed = true;
        }
        if self.columns.len() < MAX_COLUMNS && ui.button("+ Add Column").clicked() {
            self.add_column();
            changed = true;
        }

        ui.separator();

        // -- Rows table --
        ui.label(egui::RichText::new("Rows").strong());
        let stride = self.row_stride();
        let cur = self.current_row;
        let mut remove_row: Option<usize> = None;

        let cell_w = 80.0;
        let mut table = TableBuilder::new(ui)
            .id_salt(("lookup_rows", self.id.0))
            .striped(true)
            .resizable(false)
            .vscroll(true)
            .max_scroll_height(300.0)
            .column(Column::exact(36.0)); // row index
        for col in &self.columns {
            // Palette renders 4 swatches → needs a wider cell.
            // Gradient renders a preview bar with a click-to-pick popup.
            let w = match col.port_type {
                PortType::Palette => 120.0,
                PortType::Gradient => 140.0,
                PortType::Position => 90.0,
                _ => cell_w,
            };
            table = table.column(Column::exact(w));
        }
        let table = table.column(Column::exact(24.0)); // delete

        let columns_snapshot: Vec<(String, PortType)> =
            self.columns.iter().map(|c| (c.name.clone(), c.port_type)).collect();
        table
            .header(20.0, |mut header| {
                header.col(|ui| { ui.strong("#"); });
                for (name, _pt) in &columns_snapshot {
                    header.col(|ui| {
                        ui.strong(
                            egui::RichText::new(name)
                                .color(Color32::from_gray(200))
                                .size(11.0),
                        );
                    });
                }
                header.col(|_ui| {});
            })
            .body(|mut body| {
                for r in 0..self.row_count {
                    let row_start = r * stride;
                    let is_current = r == cur;
                    body.row(22.0, |mut row| {
                        row.col(|ui| {
                            // Stationary index cell — tint when this row is active.
                            // Paint a fill behind the label so the row doesn't shift.
                            let rect = ui.available_rect_before_wrap();
                            if is_current {
                                ui.painter().rect_filled(
                                    rect, 2.0,
                                    Color32::from_rgba_unmultiplied(80, 200, 240, 60),
                                );
                            }
                            let txt = egui::RichText::new(format!("{:>3}", r))
                                .monospace()
                                .color(if is_current {
                                    theme::STATUS_ACTIVE
                                } else {
                                    theme::TEXT_MUTED
                                })
                                .strong();
                            ui.label(txt);
                        });
                        for (ci, (_, pt)) in columns_snapshot.iter().enumerate() {
                            row.col(|ui| {
                                if is_current {
                                    let rect = ui.available_rect_before_wrap();
                                    ui.painter().rect_filled(
                                        rect, 2.0,
                                        Color32::from_rgba_unmultiplied(80, 200, 240, 30),
                                    );
                                }
                                let off = self.column_channel_offset(ci);
                                let cell_base = row_start + off;
                                if edit_cell(
                                    ui, *pt, &mut self.data, cell_base,
                                    self.id.0, r, ci, &self.library,
                                ) {
                                    changed = true;
                                }
                            });
                        }
                        row.col(|ui| {
                            ui.add_enabled_ui(self.row_count > MIN_ROWS, |ui| {
                                if ui.small_button(egui_phosphor::regular::X).clicked() {
                                    remove_row = Some(r);
                                }
                            });
                        });
                    });
                }
            });

        if let Some(r) = remove_row {
            self.remove_row(r);
            changed = true;
        }
        if self.row_count < MAX_ROWS && ui.button("+ Add Row").clicked() {
            self.add_row();
            changed = true;
        }

        if changed {
            self.fix_data_size();
            self.push_config();
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

fn preview_color(pt: PortType, data: &[f32], base: usize) -> Color32 {
    match pt {
        PortType::Color => Color32::from_rgb(
            (data.get(base).copied().unwrap_or(0.0).clamp(0.0, 1.0) * 255.0) as u8,
            (data.get(base + 1).copied().unwrap_or(0.0).clamp(0.0, 1.0) * 255.0) as u8,
            (data.get(base + 2).copied().unwrap_or(0.0).clamp(0.0, 1.0) * 255.0) as u8,
        ),
        PortType::Palette => {
            let mut r = 0.0; let mut g = 0.0; let mut b = 0.0;
            for i in 0..4 {
                r += data.get(base + i * 3).copied().unwrap_or(0.0);
                g += data.get(base + i * 3 + 1).copied().unwrap_or(0.0);
                b += data.get(base + i * 3 + 2).copied().unwrap_or(0.0);
            }
            Color32::from_rgb(
                ((r * 0.25).clamp(0.0, 1.0) * 255.0) as u8,
                ((g * 0.25).clamp(0.0, 1.0) * 255.0) as u8,
                ((b * 0.25).clamp(0.0, 1.0) * 255.0) as u8,
            )
        }
        PortType::Gradient => {
            // Sample the gradient at the centre as a single representative
            // colour for the body row preview.
            let g = Gradient::from_channels(
                data.get(base..base + GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS)
                    .unwrap_or(&[]),
            );
            let (rgb, _) = g.sample_with_alpha(0.5);
            Color32::from_rgb(
                (rgb.r.clamp(0.0, 1.0) * 255.0) as u8,
                (rgb.g.clamp(0.0, 1.0) * 255.0) as u8,
                (rgb.b.clamp(0.0, 1.0) * 255.0) as u8,
            )
        }
        PortType::Position => {
            let pan = data.get(base).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            let tilt = data.get(base + 1).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            Color32::from_rgb((pan * 255.0) as u8, (tilt * 255.0) as u8, 120)
        }
        PortType::Logic => {
            let on = data.get(base).copied().unwrap_or(0.0) >= 0.5;
            if on { theme::PORT_LOGIC } else { Color32::from_gray(40) }
        }
        _ => {
            let v = data.get(base).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            let g = (v * 200.0 + 40.0) as u8;
            Color32::from_rgb(g, g, 255)
        }
    }
}

fn edit_cell(
    ui: &mut Ui,
    pt: PortType,
    data: &mut [f32],
    base: usize,
    node_id: u64,
    row: usize,
    col: usize,
    library: &SharedGradientLibrary,
) -> bool {
    let mut changed = false;
    ui.push_id(("lookup_cell", node_id, row, col), |ui| {
        match pt {
            PortType::Gradient => {
                let span = GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS;
                // Inline preview bar that doubles as the click target.
                let (resp, painter) = ui.allocate_painter(
                    Vec2::new(ui.available_width().max(60.0), 18.0),
                    Sense::click(),
                );
                let rect = resp.rect;
                draw_checker(&painter, rect, 4.0);
                let g = Gradient::from_channels(
                    data.get(base..base + span).unwrap_or(&[]),
                );
                if !g.stops().is_empty() {
                    let samples = (rect.width() as usize).max(8).min(160);
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
                }
                let stroke = if resp.hovered() {
                    Stroke::new(1.5, Color32::WHITE)
                } else {
                    Stroke::new(1.0, Color32::from_gray(80))
                };
                painter.rect_stroke(rect, 2.0, stroke, StrokeKind::Inside);

                // Click → popup with library preset list.
                let popup_id = ui.id().with(("lookup_grad_popup", node_id, row, col));
                if resp.clicked() {
                    ui.memory_mut(|m| m.open_popup(popup_id));
                }
                let presets: Vec<(u32, String, Vec<GradientStop>)> = {
                    let lib = library.lock().unwrap();
                    lib.presets.iter().map(|p|
                        (p.id, p.name.clone(), p.stops.clone())
                    ).collect()
                };
                egui::popup::popup_below_widget(
                    ui,
                    popup_id,
                    &resp,
                    egui::popup::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        ui.set_min_width(220.0);
                        if presets.is_empty() {
                            ui.colored_label(
                                Color32::from_gray(140),
                                "No gradient presets yet. Save one from a Gradient Source node.",
                            );
                            return;
                        }
                        ui.label(egui::RichText::new("Pick a preset").strong());
                        for (_id, name, stops) in &presets {
                            let row_resp = ui.horizontal(|ui| {
                                let bar = ui.allocate_response(
                                    Vec2::new(80.0, 14.0), Sense::hover(),
                                );
                                let painter = ui.painter();
                                draw_checker(painter, bar.rect, 3.0);
                                let g = Gradient::new(stops.clone());
                                let samples = 64usize;
                                for i in 0..samples {
                                    let t = i as f32 / (samples - 1).max(1) as f32;
                                    let x = bar.rect.min.x + (i as f32 / samples as f32) * bar.rect.width();
                                    let (rgb, alpha) = g.sample_with_alpha(t);
                                    let c = Color32::from_rgba_unmultiplied(
                                        (rgb.r.clamp(0.0, 1.0) * 255.0) as u8,
                                        (rgb.g.clamp(0.0, 1.0) * 255.0) as u8,
                                        (rgb.b.clamp(0.0, 1.0) * 255.0) as u8,
                                        (alpha.clamp(0.0, 1.0) * 255.0) as u8,
                                    );
                                    painter.line_segment(
                                        [Pos2::new(x, bar.rect.min.y), Pos2::new(x, bar.rect.max.y)],
                                        Stroke::new(bar.rect.width() / samples as f32 + 0.5, c),
                                    );
                                }
                                ui.button(name).clicked()
                            });
                            if row_resp.inner {
                                let g = Gradient::new(stops.clone());
                                let chans = g.to_channels();
                                for (i, v) in chans.iter().enumerate() {
                                    if base + i < data.len() {
                                        data[base + i] = *v;
                                    }
                                }
                                changed = true;
                                ui.memory_mut(|m| m.close_popup());
                            }
                        }
                        ui.separator();
                        if ui.small_button("Clear").clicked() {
                            // Mark all stops as unused (alpha = -1 sentinel).
                            for i in 0..GRADIENT_STOP_COUNT {
                                let b = base + i * GRADIENT_STOP_FLOATS;
                                if b + 4 < data.len() {
                                    data[b] = 0.0;
                                    data[b + 1] = 0.0;
                                    data[b + 2] = 0.0;
                                    data[b + 3] = -1.0;
                                    data[b + 4] = 0.0;
                                }
                            }
                            changed = true;
                            ui.memory_mut(|m| m.close_popup());
                        }
                    },
                );
            }
            PortType::Color => {
                let mut c = [
                    data.get(base).copied().unwrap_or(0.0),
                    data.get(base + 1).copied().unwrap_or(0.0),
                    data.get(base + 2).copied().unwrap_or(0.0),
                ];
                if ui.color_edit_button_rgb(&mut c).changed() {
                    data[base] = c[0];
                    data[base + 1] = c[1];
                    data[base + 2] = c[2];
                    changed = true;
                }
            }
            PortType::Palette => {
                for i in 0..4 {
                    let b = base + i * 3;
                    let mut c = [
                        data.get(b).copied().unwrap_or(0.0),
                        data.get(b + 1).copied().unwrap_or(0.0),
                        data.get(b + 2).copied().unwrap_or(0.0),
                    ];
                    ui.push_id(("palette_slot", i), |ui| {
                        if ui.color_edit_button_rgb(&mut c).changed() {
                            data[b] = c[0];
                            data[b + 1] = c[1];
                            data[b + 2] = c[2];
                            changed = true;
                        }
                    });
                }
            }
            PortType::Position => {
                let mut pan = data.get(base).copied().unwrap_or(0.0);
                let mut tilt = data.get(base + 1).copied().unwrap_or(0.0);
                if ui.add(egui::DragValue::new(&mut pan).range(0.0..=1.0).speed(0.01)).changed() {
                    data[base] = pan.clamp(0.0, 1.0);
                    changed = true;
                }
                if ui.add(egui::DragValue::new(&mut tilt).range(0.0..=1.0).speed(0.01)).changed() {
                    data[base + 1] = tilt.clamp(0.0, 1.0);
                    changed = true;
                }
            }
            PortType::Logic => {
                let mut on = data.get(base).copied().unwrap_or(0.0) >= 0.5;
                if ui.checkbox(&mut on, "").changed() {
                    data[base] = if on { 1.0 } else { 0.0 };
                    changed = true;
                }
            }
            PortType::Phase => {
                let mut v = data.get(base).copied().unwrap_or(0.0);
                if ui.add(egui::DragValue::new(&mut v).range(0.0..=1.0).speed(0.005)).changed() {
                    data[base] = v.clamp(0.0, 1.0);
                    changed = true;
                }
            }
            _ => {
                let mut v = data.get(base).copied().unwrap_or(0.0);
                if ui.add(egui::DragValue::new(&mut v).speed(0.01)).changed() {
                    data[base] = v;
                    changed = true;
                }
            }
        }
    });
    changed
}

fn draw_checker(painter: &egui::Painter, rect: egui::Rect, cell: f32) {
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
