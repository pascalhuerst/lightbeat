use std::any::Any;
use std::collections::BTreeMap;

use egui::{self, Color32, Ui, Vec2};

use crate::engine::nodes::ui::button::ButtonMode;
use crate::engine::nodes::ui::button_group::ButtonGroupDisplay;
use crate::engine::nodes::ui::common::MouseOverrideMode;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::fader::highlight_alpha;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const HIGHLIGHT_DURATION: f64 = 0.5;

pub struct ButtonGroupWidget {
    id: NodeId,
    shared: SharedState,
    name: String,
    rows: usize,
    cols: usize,
    mode: ButtonMode,
    states: Vec<bool>,
    input_values: Vec<f32>,
    override_active: Vec<bool>,
    inputs_enabled: bool,
    override_enabled: bool,
    reset_mode: MouseOverrideMode,
    click_ids: BTreeMap<(usize, usize), u64>,
    last_click_time: Vec<Option<f64>>,
}

impl ButtonGroupWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        let rows = 2;
        let cols = 2;
        let n = rows * cols;
        Self {
            id, shared, name: String::new(), rows, cols,
            mode: ButtonMode::Trigger,
            states: vec![false; n],
            input_values: vec![0.0; n],
            override_active: vec![false; n],
            inputs_enabled: false,
            override_enabled: false,
            reset_mode: MouseOverrideMode::ClearOnReset,
            click_ids: BTreeMap::new(),
            last_click_time: vec![None; n],
        }
    }

    fn cell_count(&self) -> usize { self.rows * self.cols }

    fn push_config(&self, override_states: Option<&[Option<bool>]>) {
        let clicks: serde_json::Map<String, serde_json::Value> = self.click_ids.iter()
            .map(|((r, c), v)| (format!("{},{}", r, c), serde_json::json!(v)))
            .collect();
        let mut shared = self.shared.lock().unwrap();
        let mut cfg = serde_json::json!({
            "name": self.name,
            "rows": self.rows,
            "cols": self.cols,
            "mode": self.mode.as_str(),
            "clicks": clicks,
            "inputs_enabled": self.inputs_enabled,
            "override_enabled": self.override_enabled,
            "reset_mode": self.reset_mode.as_str(),
        });
        if let Some(ovs) = override_states {
            let arr: Vec<serde_json::Value> = ovs.iter().map(|o| match o {
                Some(b) => serde_json::json!(b),
                None => serde_json::Value::Null,
            }).collect();
            cfg["override_states"] = serde_json::json!(arr);
        }
        shared.pending_config = Some(cfg);
    }

    fn push_settings(&self) {
        self.push_config(None);
    }

    fn resize(&mut self) {
        let n = self.cell_count();
        self.states.resize(n, false);
        self.input_values.resize(n, 0.0);
        self.override_active.resize(n, false);
        self.last_click_time.resize(n, None);
    }
}

impl NodeWidget for ButtonGroupWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Button Group" }
    fn title(&self) -> &str { &self.name }
    fn description(&self) -> &'static str {
        "Grid of buttons with one Logic output per cell. Trigger or Toggle mode applies to all."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        if !self.inputs_enabled { return vec![]; }
        (0..self.rows).flat_map(|r| (0..self.cols).map(move |c| {
            UiPortDef::from_def(&PortDef::new(format!("{},{}", r, c), PortType::Logic))
        })).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        (0..self.rows).flat_map(|r| (0..self.cols).map(move |c| {
            UiPortDef::from_def(&PortDef::new(format!("{},{}", r, c), PortType::Logic))
        })).collect()
    }

    fn min_width(&self) -> f32 { (self.cols as f32 * 40.0).max(120.0) }
    fn min_content_height(&self) -> f32 { (self.rows as f32 * 32.0).max(60.0) }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn output_highlight(&self, port_idx: usize, now: f64) -> f32 {
        let last = self.last_click_time.get(port_idx).copied().flatten();
        highlight_alpha(last, now, HIGHLIGHT_DURATION)
    }
    fn input_highlight(&self, port_idx: usize, now: f64) -> f32 {
        let last = self.last_click_time.get(port_idx).copied().flatten();
        highlight_alpha(last, now, HIGHLIGHT_DURATION)
    }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        // Sync from engine display.
        let snapshot: Option<(
            String, usize, usize, ButtonMode, Vec<bool>, Vec<f32>, Vec<bool>,
            bool, bool, MouseOverrideMode,
        )> = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<ButtonGroupDisplay>())
                .map(|d| (
                    d.name.clone(),
                    d.rows, d.cols, d.mode, d.states.clone(),
                    d.input_values.clone(), d.override_active.clone(),
                    d.inputs_enabled, d.override_enabled, d.reset_mode,
                ))
        };
        if let Some((name, rows, cols, mode, states, ins, ovs, ie, oe, rm)) = snapshot {
            let resized = rows != self.rows || cols != self.cols;
            self.name = name;
            self.rows = rows;
            self.cols = cols;
            self.mode = mode;
            self.states = states;
            self.input_values = ins;
            self.override_active = ovs;
            self.inputs_enabled = ie;
            self.override_enabled = oe;
            self.reset_mode = rm;
            if resized { self.resize(); }
        }

        let avail = ui.available_size();
        if avail.x <= 0.0 || avail.y <= 0.0 { return; }
        let cols = self.cols.max(1);
        let rows = self.rows.max(1);
        let cw = avail.x / cols as f32;
        let rh = avail.y / rows as f32;
        let (rect, _resp) = ui.allocate_exact_size(avail, egui::Sense::hover());
        let now = ui.ctx().input(|i| i.time);

        let interactive = match self.mode {
            ButtonMode::Trigger => true,
            ButtonMode::Toggle => !self.inputs_enabled || self.override_enabled,
        };

        let mut new_overrides: Vec<Option<bool>> = self.override_active.iter().enumerate()
            .map(|(i, &a)| if a { Some(self.states.get(i).copied().unwrap_or(false)) } else { None })
            .collect();
        let mut overrides_changed = false;
        let mut clicks_changed = false;

        for r in 0..rows {
            for c in 0..cols {
                let idx = r * cols + c;
                let cell_min = egui::pos2(rect.min.x + c as f32 * cw, rect.min.y + r as f32 * rh);
                let cell_rect = egui::Rect::from_min_size(cell_min, Vec2::new(cw, rh));
                let inner = cell_rect.shrink(1.5);
                let id = ui.id().with((self.id, r, c));
                let sense = if interactive { egui::Sense::click() } else { egui::Sense::hover() };
                let resp = ui.interact(inner, id, sense);

                let pressed = matches!(self.mode, ButtonMode::Toggle)
                    && self.states.get(idx).copied().unwrap_or(false);
                let fill = if pressed {
                    theme::SEM_PRIMARY
                } else if resp.hovered() {
                    Color32::from_gray(80)
                } else {
                    Color32::from_gray(60)
                };
                let text_color = if pressed { Color32::BLACK } else { Color32::WHITE };

                let painter = ui.painter_at(inner);
                painter.rect_filled(inner, 3.0, fill);
                let override_active = self.override_active.get(idx).copied().unwrap_or(false);
                let border = if override_active {
                    theme::SEM_WARNING
                } else if resp.hovered() {
                    Color32::from_gray(140)
                } else {
                    Color32::from_gray(90)
                };
                let border_width = if override_active { 2.0 } else { 1.0 };
                painter.rect_stroke(inner, 3.0,
                    egui::Stroke::new(border_width, border), egui::StrokeKind::Inside);
                painter.text(
                    inner.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{},{}", r, c),
                    egui::FontId::proportional(11.0 * zoom),
                    text_color,
                );

                // Input-state badge when overridden.
                if self.inputs_enabled && override_active {
                    let dot_r = 3.5 * zoom;
                    let dot_pos = egui::pos2(inner.max.x - dot_r - 2.0, inner.min.y + dot_r + 2.0);
                    let in_color = if self.input_values.get(idx).copied().unwrap_or(0.0) >= 0.5 {
                        theme::SEM_PRIMARY
                    } else {
                        Color32::from_gray(80)
                    };
                    painter.circle_filled(dot_pos, dot_r, in_color);
                    painter.circle_stroke(dot_pos, dot_r,
                        egui::Stroke::new(1.0, theme::SEM_WARNING));
                }

                if interactive {
                    if resp.double_clicked() {
                        if let Some(slot) = self.last_click_time.get_mut(idx) { *slot = Some(now); }
                        if self.mode == ButtonMode::Toggle && self.inputs_enabled && self.override_enabled {
                            new_overrides[idx] = None;
                            overrides_changed = true;
                        } else {
                            let entry = self.click_ids.entry((r, c)).or_insert(0);
                            *entry = entry.wrapping_add(1);
                            clicks_changed = true;
                        }
                    } else if resp.clicked() {
                        if let Some(slot) = self.last_click_time.get_mut(idx) { *slot = Some(now); }
                        let entry = self.click_ids.entry((r, c)).or_insert(0);
                        *entry = entry.wrapping_add(1);
                        clicks_changed = true;
                    }
                }
            }
        }

        if overrides_changed {
            self.push_config(Some(&new_overrides));
        } else if clicks_changed {
            self.push_settings();
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
        ui.separator();
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
        ui.horizontal(|ui| {
            ui.label("Mode:");
            if ui.radio_value(&mut self.mode, ButtonMode::Trigger, "Trigger").clicked() {
                changed = true;
            }
            if ui.radio_value(&mut self.mode, ButtonMode::Toggle, "Toggle").clicked() {
                changed = true;
            }
        });
        if ui.checkbox(&mut self.inputs_enabled, "Enable inputs").changed() {
            changed = true;
        }
        if self.inputs_enabled && self.mode == ButtonMode::Toggle {
            ui.indent("bg_input_opts", |ui| {
                if ui.checkbox(&mut self.override_enabled, "Allow mouse override").changed() {
                    changed = true;
                }
                if self.override_enabled {
                    ui.horizontal(|ui| {
                        ui.label("Reset:");
                        egui::ComboBox::from_id_salt(("bg_reset", self.id))
                            .selected_text(self.reset_mode.label())
                            .show_ui(ui, |ui| {
                                for m in [
                                    MouseOverrideMode::ClearOnReset,
                                    MouseOverrideMode::PickupIncrease,
                                    MouseOverrideMode::PickupDecrease,
                                ] {
                                    if ui.selectable_label(self.reset_mode == m, m.label()).clicked() {
                                        self.reset_mode = m;
                                        changed = true;
                                    }
                                }
                            });
                    });
                }
            });
        } else if self.inputs_enabled && self.mode == ButtonMode::Trigger {
            ui.indent("bg_input_opts", |ui| {
                ui.colored_label(
                    Color32::from_gray(140),
                    "Trigger mode: input rising edges fire the corresponding output. No override.",
                );
            });
        }
        if changed { self.push_settings(); }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
