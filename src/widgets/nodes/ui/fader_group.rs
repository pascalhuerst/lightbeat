use std::any::Any;

use egui::{self, Color32, Ui, Vec2};

use crate::engine::nodes::ui::common::MouseOverrideMode;
use crate::engine::nodes::ui::fader_group::FaderGroupDisplay;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::fader::{self, highlight_alpha, FaderStyle, Orientation};
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;
use crate::widgets::nodes::ui::fader::draw_override_indicator;

const HIGHLIGHT_DURATION: f64 = 0.5;
const OVERRIDE_OVERLAY_COLOR: Color32 = theme::SEM_WARNING_FILL;
const CENTER_LINE_COLOR: Color32 = Color32::from_gray(110);

pub struct FaderGroupWidget {
    id: NodeId,
    shared: SharedState,
    /// User-given label. Shown in the node title bar; empty falls back to
    /// the generic "Fader Group" text.
    name: String,
    rows: usize,
    cols: usize,
    output_values: Vec<f32>,
    input_values: Vec<f32>,
    override_active: Vec<bool>,
    override_values: Vec<f32>,
    inputs_enabled: Vec<bool>,
    outputs_enabled: Vec<bool>,
    mouse_override: Vec<MouseOverrideMode>,
    bipolar: Vec<bool>,
    any_input_enabled: bool,
    any_output_enabled: bool,
    last_edit_time: Vec<Option<f64>>,
}

impl FaderGroupWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        let rows = 1;
        let cols = 4;
        let n = rows * cols;
        Self {
            id, shared,
            name: String::new(),
            rows, cols,
            output_values: vec![0.0; n],
            input_values: vec![0.0; n],
            override_active: vec![false; n],
            override_values: vec![0.0; n],
            inputs_enabled: vec![false; n],
            outputs_enabled: vec![true; n],
            mouse_override: vec![MouseOverrideMode::No; n],
            bipolar: vec![false; n],
            any_input_enabled: false,
            any_output_enabled: true,
            last_edit_time: vec![None; n],
        }
    }

    fn cell_count(&self) -> usize { self.rows * self.cols }

    fn push_config(
        &self,
        mouse_values: Option<&[f32]>,
        overrides: Option<&[Option<f32>]>,
    ) {
        let mut shared = self.shared.lock().unwrap();
        let mo_strs: Vec<&str> = self.mouse_override.iter().map(|m| m.as_str()).collect();
        let mut cfg = serde_json::json!({
            "name": self.name,
            "rows": self.rows,
            "cols": self.cols,
            "inputs_enabled": self.inputs_enabled,
            "outputs_enabled": self.outputs_enabled,
            "mouse_override": mo_strs,
            "bipolar": self.bipolar,
        });
        if let Some(vs) = mouse_values {
            cfg["mouse_values"] = serde_json::json!(vs);
        }
        if let Some(ovs) = overrides {
            let arr: Vec<serde_json::Value> = ovs.iter().map(|o| match o {
                Some(v) => serde_json::json!(v),
                None => serde_json::Value::Null,
            }).collect();
            cfg["override_values"] = serde_json::json!(arr);
        }
        shared.pending_config = Some(cfg);
    }

    fn push_settings(&self) {
        self.push_config(None, None);
    }

    fn resize(&mut self) {
        let n = self.cell_count();
        self.output_values.resize(n, 0.0);
        self.input_values.resize(n, 0.0);
        self.override_active.resize(n, false);
        self.override_values.resize(n, 0.0);
        self.inputs_enabled.resize(n, false);
        self.outputs_enabled.resize(n, true);
        self.mouse_override.resize(n, MouseOverrideMode::No);
        self.bipolar.resize(n, false);
        self.last_edit_time.resize(n, None);
    }

    /// Restore port-affecting state directly from save_data. Called by
    /// project.rs *before* connections are loaded, so that `ui_inputs` /
    /// `ui_outputs` already reflect the saved port layout when
    /// `cleanup_stale_connections` runs on the first frame.
    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
            self.name = n.to_string();
        }
        if let Some(r) = data.get("rows").and_then(|v| v.as_u64()) {
            self.rows = (r as usize).clamp(1, 16);
        }
        if let Some(c) = data.get("cols").and_then(|v| v.as_u64()) {
            self.cols = (c as usize).clamp(1, 16);
        }
        self.resize();

        if let Some(arr) = data.get("inputs_enabled").and_then(|v| v.as_array()) {
            for (i, v) in arr.iter().enumerate() {
                if let (Some(b), Some(slot)) = (v.as_bool(), self.inputs_enabled.get_mut(i)) {
                    *slot = b;
                }
            }
        }
        if let Some(arr) = data.get("outputs_enabled").and_then(|v| v.as_array()) {
            for (i, v) in arr.iter().enumerate() {
                if let (Some(b), Some(slot)) = (v.as_bool(), self.outputs_enabled.get_mut(i)) {
                    *slot = b;
                }
            }
        }
        self.any_input_enabled = self.inputs_enabled.iter().any(|&b| b);
        self.any_output_enabled = self.outputs_enabled.iter().any(|&b| b);
    }
}

impl NodeWidget for FaderGroupWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Fader Group" }
    fn title(&self) -> &str {
        if self.name.is_empty() { "Fader Group" } else { self.name.as_str() }
    }
    fn description(&self) -> &'static str {
        "Grid of faders. Each cell has its own input enable, mouse override mode, and bipolar setting."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        if !self.any_input_enabled { return vec![]; }
        (0..self.rows).flat_map(|r| (0..self.cols).map(move |c| {
            let i = r * self.cols + c;
            let enabled = self.inputs_enabled.get(i).copied().unwrap_or(false);
            UiPortDef::from_def(&PortDef::new(format!("{},{}", r, c), PortType::Untyped))
                .with_disabled(!enabled)
        })).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        if !self.any_output_enabled { return vec![]; }
        (0..self.rows).flat_map(|r| (0..self.cols).map(move |c| {
            let i = r * self.cols + c;
            let enabled = self.outputs_enabled.get(i).copied().unwrap_or(false);
            UiPortDef::from_def(&PortDef::new(format!("{},{}", r, c), PortType::Untyped))
                .with_disabled(!enabled)
        })).collect()
    }

    fn min_width(&self) -> f32 { (self.cols as f32 * 28.0).max(120.0) }
    fn min_content_height(&self) -> f32 { (self.rows as f32 * 60.0).max(80.0) }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn output_highlight(&self, port_idx: usize, now: f64) -> f32 {
        let last = self.last_edit_time.get(port_idx).copied().flatten();
        highlight_alpha(last, now, HIGHLIGHT_DURATION)
    }
    fn input_highlight(&self, port_idx: usize, now: f64) -> f32 {
        let last = self.last_edit_time.get(port_idx).copied().flatten();
        highlight_alpha(last, now, HIGHLIGHT_DURATION)
    }

    fn inspector_hides_default_ports(&self) -> bool { true }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        // Sync from engine display.
        let snapshot = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<FaderGroupDisplay>())
                .map(|d| (
                    d.name.clone(),
                    d.rows, d.cols,
                    d.outputs.clone(), d.inputs.clone(),
                    d.override_active.clone(), d.override_values.clone(),
                    d.inputs_enabled.clone(), d.outputs_enabled.clone(),
                    d.mouse_override.clone(),
                    d.bipolar.clone(), d.any_input_enabled, d.any_output_enabled,
                ))
        };
        if let Some((name, rows, cols, outs, ins, ovsa, ovsv, ie, oe, mo, bp, anyi, anyo)) = snapshot {
            self.name = name;
            let resized = rows != self.rows || cols != self.cols;
            self.rows = rows;
            self.cols = cols;
            if resized { self.resize(); }
            let n = self.cell_count();
            if outs.len() == n { self.output_values = outs; }
            if ins.len() == n { self.input_values = ins; }
            if ovsa.len() == n { self.override_active = ovsa; }
            if ovsv.len() == n { self.override_values = ovsv; }
            if ie.len() == n { self.inputs_enabled = ie; }
            if oe.len() == n { self.outputs_enabled = oe; }
            if mo.len() == n { self.mouse_override = mo; }
            if bp.len() == n { self.bipolar = bp; }
            self.any_input_enabled = anyi;
            self.any_output_enabled = anyo;
        }

        let avail = ui.available_size();
        if avail.x <= 0.0 || avail.y <= 0.0 { return; }
        let rows = self.rows.max(1);
        let cols = self.cols.max(1);
        let cw = avail.x / cols as f32;
        let rh = avail.y / rows as f32;
        let (rect, _resp) = ui.allocate_exact_size(avail, egui::Sense::hover());
        let now = ui.ctx().input(|i| i.time);
        let style = FaderStyle::default();

        let mut new_mouse = self.output_values.clone();
        let mut new_overrides: Vec<Option<f32>> = self.override_active.iter().enumerate()
            .map(|(i, &a)| if a { Some(self.override_values[i]) } else { None })
            .collect();
        let mut mouse_changed = false;
        let mut override_changed = false;

        for r in 0..rows {
            for c in 0..cols {
                let idx = r * cols + c;
                let inputs_enabled = self.inputs_enabled[idx];
                let mode = self.mouse_override[idx];
                let bipolar = self.bipolar[idx];
                let mouse_interactive = !inputs_enabled || mode.allows_override();
                let cell_min = egui::pos2(rect.min.x + c as f32 * cw, rect.min.y + r as f32 * rh);
                let cell_rect = egui::Rect::from_min_size(cell_min, Vec2::new(cw, rh)).shrink(1.5);
                let id = ui.id().with((self.id, r, c));
                let sense = if mouse_interactive {
                    egui::Sense::click_and_drag()
                } else {
                    egui::Sense::hover()
                };
                let resp = ui.interact(cell_rect, id, sense);
                let painter = ui.painter_at(cell_rect);

                // Main fill = input value (when input enabled) or output (legacy mouse).
                let main_value = if inputs_enabled { self.input_values[idx] } else { self.output_values[idx] };
                if bipolar {
                    painter.rect_filled(cell_rect, style.corner_radius, style.bg);
                    fader::draw_bipolar_fill(&painter, cell_rect, main_value, Orientation::Vertical, style.fill);
                    fader::draw_bipolar_center_line(&painter, cell_rect, Orientation::Vertical, CENTER_LINE_COLOR);
                    if let Some(b) = style.border {
                        painter.rect_stroke(cell_rect, style.corner_radius,
                            egui::Stroke::new(1.0, b), egui::StrokeKind::Inside);
                    }
                } else {
                    fader::draw_fader(&painter, cell_rect, main_value, Orientation::Vertical, &style, false);
                }

                // Override overlay.
                if inputs_enabled && self.override_active[idx] {
                    if bipolar {
                        fader::draw_bipolar_fill(&painter, cell_rect, self.override_values[idx],
                            Orientation::Vertical, OVERRIDE_OVERLAY_COLOR);
                    } else {
                        fader::draw_fader_overlay(&painter, cell_rect, self.override_values[idx],
                            Orientation::Vertical, OVERRIDE_OVERLAY_COLOR);
                    }
                    draw_override_indicator(&painter, cell_rect, mode, 1.0);
                }

                if mouse_interactive {
                    let mut v = if inputs_enabled && mode.allows_override() {
                        if self.override_active[idx] { self.override_values[idx] }
                        else { self.input_values[idx] }
                    } else {
                        self.output_values[idx]
                    };
                    let before = v;
                    let touched = fader::handle_fader_interaction(
                        ui, &resp, cell_rect, Orientation::Vertical, &mut v,
                    );
                    if resp.double_clicked() {
                        if let Some(t) = self.last_edit_time.get_mut(idx) { *t = Some(now); }
                        if inputs_enabled && mode.allows_override() {
                            new_overrides[idx] = None;
                            override_changed = true;
                        } else {
                            new_mouse[idx] = if bipolar { 0.5 } else { 0.0 };
                            mouse_changed = true;
                        }
                    } else if touched || (v - before).abs() > f32::EPSILON {
                        if let Some(t) = self.last_edit_time.get_mut(idx) { *t = Some(now); }
                        if inputs_enabled && mode.allows_override() {
                            new_overrides[idx] = Some(v);
                            override_changed = true;
                        } else {
                            new_mouse[idx] = v;
                            mouse_changed = true;
                        }
                    }
                }
            }
        }

        if override_changed {
            self.push_config(None, Some(&new_overrides));
        } else if mouse_changed {
            self.push_config(Some(&new_mouse), None);
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Name:");
            if ui.text_edit_singleline(&mut self.name).changed() {
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Rows:");
            let mut r = self.rows as i32;
            if ui.add(egui::DragValue::new(&mut r).range(1..=16)).changed() {
                self.rows = r as usize;
                self.resize();
                changed = true;
            }
            ui.label("Cols:");
            let mut c = self.cols as i32;
            if ui.add(egui::DragValue::new(&mut c).range(1..=16)).changed() {
                self.cols = c as usize;
                self.resize();
                changed = true;
            }
        });

        ui.separator();
        ui.label(egui::RichText::new("Outputs").strong());

        egui::ScrollArea::horizontal().show(ui, |ui| {
            egui::Grid::new(("fg_table", self.id))
                .striped(true)
                .num_columns(8)
                .spacing([8.0, 4.0])
                .min_col_width(40.0)
                .show(ui, |ui| {
                    // Header row.
                    ui.label(egui::RichText::new("Cell").strong().size(11.0));
                    ui.label(egui::RichText::new("In").strong().size(11.0));
                    ui.label(egui::RichText::new("In val").strong().size(11.0));
                    ui.label(egui::RichText::new("Mouse override").strong().size(11.0));
                    ui.label(egui::RichText::new("Ovr val").strong().size(11.0));
                    ui.label(egui::RichText::new("Out").strong().size(11.0));
                    ui.label(egui::RichText::new("Out val").strong().size(11.0));
                    ui.label(egui::RichText::new("Bipolar").strong().size(11.0));
                    ui.end_row();

                    for r in 0..self.rows.max(1) {
                        for c in 0..self.cols.max(1) {
                            let idx = r * self.cols + c;
                            let salt = (self.id.0 << 16) | ((r as u64) << 8) | c as u64;

                            ui.label(format!("{},{}", r, c));

                            if ui.checkbox(&mut self.inputs_enabled[idx], "").changed() {
                                changed = true;
                            }

                            if self.inputs_enabled[idx] {
                                ui.colored_label(
                                    Color32::from_gray(180),
                                    format!("{:.3}", self.input_values[idx]),
                                );
                            } else {
                                ui.colored_label(Color32::from_gray(80), "—");
                            }

                            let mut mode = self.mouse_override[idx];
                            egui::ComboBox::from_id_salt(("mo", salt))
                                .selected_text(mode.label())
                                .show_ui(ui, |ui| {
                                    for m in [
                                        MouseOverrideMode::No,
                                        MouseOverrideMode::ClearOnReset,
                                        MouseOverrideMode::PickupDecrease,
                                        MouseOverrideMode::PickupIncrease,
                                    ] {
                                        if ui.selectable_label(mode == m, m.label()).clicked() {
                                            mode = m;
                                        }
                                    }
                                });
                            if mode != self.mouse_override[idx] {
                                self.mouse_override[idx] = mode;
                                changed = true;
                            }

                            if self.override_active[idx] {
                                ui.colored_label(
                                    theme::SEM_WARNING,
                                    format!("{:.3}", self.override_values[idx]),
                                );
                            } else {
                                ui.colored_label(Color32::from_gray(80), "—");
                            }

                            if ui.checkbox(&mut self.outputs_enabled[idx], "").changed() {
                                changed = true;
                            }

                            if self.outputs_enabled[idx] {
                                ui.colored_label(
                                    Color32::from_gray(220),
                                    format!("{:.3}", self.output_values[idx]),
                                );
                            } else {
                                ui.colored_label(Color32::from_gray(80), "—");
                            }

                            if ui.checkbox(&mut self.bipolar[idx], "").changed() {
                                changed = true;
                            }

                            ui.end_row();
                        }
                    }
                });
        });

        if changed { self.push_settings(); }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
