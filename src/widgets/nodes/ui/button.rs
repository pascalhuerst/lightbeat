use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::ui::button::{ButtonDisplay, ButtonMode};
use crate::engine::nodes::ui::common::MouseOverrideMode;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::fader::highlight_alpha;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const HIGHLIGHT_DURATION: f64 = 0.5;

pub struct ButtonWidget {
    id: NodeId,
    shared: SharedState,
    label: String,
    mode: ButtonMode,
    /// Local mirror of toggle state (synced from engine display).
    state: bool,
    input_value: f32,
    inputs_enabled: bool,
    override_enabled: bool,
    override_active: bool,
    reset_mode: MouseOverrideMode,
    /// Monotonic counter pushed to engine on click.
    click_id: u64,
    last_interact_time: Option<f64>,
}

impl ButtonWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            label: "Button".to_string(),
            mode: ButtonMode::Trigger,
            state: false,
            input_value: 0.0,
            inputs_enabled: false,
            override_enabled: false,
            override_active: false,
            reset_mode: MouseOverrideMode::ClearOnReset,
            click_id: 0,
            last_interact_time: None,
        }
    }

    fn push_config(&self, override_state: Option<Option<bool>>) {
        let mut shared = self.shared.lock().unwrap();
        let mut cfg = serde_json::json!({
            "label": self.label,
            "mode": self.mode.as_str(),
            "click_id": self.click_id,
            "inputs_enabled": self.inputs_enabled,
            "override_enabled": self.override_enabled,
            "reset_mode": self.reset_mode.as_str(),
        });
        if let Some(os) = override_state {
            cfg["override_state"] = match os {
                Some(b) => serde_json::json!(b),
                None => serde_json::Value::Null,
            };
        }
        shared.pending_config = Some(cfg);
    }

    fn push_settings(&self) {
        self.push_config(None);
    }
}

impl NodeWidget for ButtonWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Button" }
    fn title(&self) -> &str {
        if self.label.is_empty() { "Button" } else { self.label.as_str() }
    }
    fn description(&self) -> &'static str {
        "Clickable button outputting a trigger pulse or persistent toggle state."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        if self.inputs_enabled {
            vec![UiPortDef::from_def(&PortDef::new("in", PortType::Logic))]
        } else {
            vec![]
        }
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", PortType::Logic))]
    }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 30.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn output_highlight(&self, port_idx: usize, now: f64) -> f32 {
        if port_idx != 0 { return 0.0; }
        highlight_alpha(self.last_interact_time, now, HIGHLIGHT_DURATION)
    }
    fn input_highlight(&self, port_idx: usize, now: f64) -> f32 {
        if port_idx != 0 { return 0.0; }
        highlight_alpha(self.last_interact_time, now, HIGHLIGHT_DURATION)
    }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        // Sync state and label from engine display.
        {
            let shared = self.shared.lock().unwrap();
            if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<ButtonDisplay>()) {
                self.label = d.label.clone();
                self.mode = d.mode;
                self.state = d.state;
                self.input_value = d.input_value;
                self.inputs_enabled = d.inputs_enabled;
                self.override_enabled = d.override_enabled;
                self.override_active = d.override_active;
                self.reset_mode = d.reset_mode;
            }
        }

        let pressed = matches!(self.mode, ButtonMode::Toggle) && self.state;
        let fill = if pressed {
            theme::SEM_PRIMARY
        } else {
            Color32::from_gray(60)
        };
        let text_color = if pressed { Color32::BLACK } else { Color32::WHITE };

        let avail = ui.available_size();
        if avail.x <= 0.0 || avail.y <= 0.0 { return; }
        // Toggle + inputs_enabled + override_off: button is not interactive
        // (input fully drives state).
        let interactive = match self.mode {
            ButtonMode::Trigger => true,
            ButtonMode::Toggle => !self.inputs_enabled || self.override_enabled,
        };
        let sense = if interactive { egui::Sense::click() } else { egui::Sense::hover() };
        let (rect, resp) = ui.allocate_exact_size(avail, sense);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, fill);
        let border = if self.override_active {
            // Bright orange border to indicate override is active.
            theme::SEM_WARNING
        } else if resp.hovered() {
            Color32::from_gray(140)
        } else {
            Color32::from_gray(90)
        };
        let border_width = if self.override_active { 2.0 } else { 1.0 };
        painter.rect_stroke(
            rect, 3.0,
            egui::Stroke::new(border_width, border),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &self.label,
            egui::FontId::proportional(13.0 * zoom),
            text_color,
        );

        // Show the underlying input state in the corner when overridden.
        if self.inputs_enabled && self.override_active {
            let dot_r = 4.0 * zoom;
            let dot_pos = egui::pos2(rect.max.x - dot_r - 3.0, rect.min.y + dot_r + 3.0);
            let in_color = if self.input_value >= 0.5 {
                theme::SEM_PRIMARY
            } else {
                Color32::from_gray(80)
            };
            painter.circle_filled(dot_pos, dot_r, in_color);
            painter.circle_stroke(dot_pos, dot_r,
                egui::Stroke::new(1.0, theme::SEM_WARNING));
        }

        if interactive {
            let now = ui.ctx().input(|i| i.time);
            if resp.double_clicked() {
                self.last_interact_time = Some(now);
                if self.mode == ButtonMode::Toggle && self.inputs_enabled && self.override_enabled {
                    // Clear override.
                    self.push_config(Some(None));
                } else {
                    // Trigger mode double-click: just register as a click.
                    self.click_id = self.click_id.wrapping_add(1);
                    self.push_settings();
                }
            } else if resp.clicked() {
                self.last_interact_time = Some(now);
                self.click_id = self.click_id.wrapping_add(1);
                self.push_settings();
            }
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Label:");
            if ui.text_edit_singleline(&mut self.label).changed() {
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

        if ui.checkbox(&mut self.inputs_enabled, "Enable input").changed() {
            changed = true;
        }
        if self.inputs_enabled && self.mode == ButtonMode::Toggle {
            ui.indent("btn_input_opts", |ui| {
                if ui.checkbox(&mut self.override_enabled, "Allow mouse override").changed() {
                    changed = true;
                }
                if self.override_enabled {
                    ui.horizontal(|ui| {
                        ui.label("Reset:");
                        egui::ComboBox::from_id_salt(("btn_reset", self.id))
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
            ui.indent("btn_input_opts", |ui| {
                ui.colored_label(
                    Color32::from_gray(140),
                    "Trigger mode: input rising edges fire the output. No override.",
                );
            });
        }

        if changed { self.push_settings(); }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
