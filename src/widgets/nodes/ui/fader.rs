use egui::{self, Color32, Ui};

use crate::engine::nodes::ui::common::MouseOverrideMode;
use crate::engine::nodes::ui::fader::{ports_for, FaderDisplay, FaderOrientation};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::setup_ui;
use crate::widgets::fader::{self, highlight_alpha, FaderStyle};
use crate::widgets::nodes::node::{param_usize, NodeLayout, NodeWidget, node_identity, with_display};
use crate::widgets::nodes::types::UiPortDef;

const HIGHLIGHT_DURATION: f64 = 0.5;
const OVERRIDE_OVERLAY_COLOR: Color32 = theme::SEM_WARNING_FILL;
const OVERRIDE_ICON_COLOR: Color32 = theme::SEM_WARNING_BRIGHT;
const CENTER_LINE_COLOR: Color32 = theme::CENTER_LINE;

/// Icon string indicating how the override clears for the given mode.
pub fn override_mode_icon(mode: MouseOverrideMode) -> &'static str {
    match mode {
        MouseOverrideMode::PickupIncrease => egui_phosphor::regular::ARROW_UP,
        MouseOverrideMode::PickupDecrease => egui_phosphor::regular::ARROW_DOWN,
        MouseOverrideMode::ClearOnReset => egui_phosphor::regular::X,
        MouseOverrideMode::No => "",
    }
}

/// Paint the small reset-mode glyph at the top of `rect` (used when an
/// override is active).
pub fn draw_override_indicator(
    painter: &egui::Painter,
    rect: egui::Rect,
    mode: MouseOverrideMode,
    zoom: f32,
) {
    let icon = override_mode_icon(mode);
    if icon.is_empty() { return; }
    painter.text(
        egui::pos2(rect.center().x, rect.min.y + 4.0 * zoom),
        egui::Align2::CENTER_TOP,
        icon,
        egui::FontId::proportional(12.0 * zoom),
        OVERRIDE_ICON_COLOR,
    );
}

pub struct FaderWidget {
    id: NodeId,
    shared: SharedState,
    // Mirrored from engine display.
    name: String,
    orientation: FaderOrientation,
    output_value: f32,
    input_value: f32,
    inputs_enabled: bool,
    mouse_override: MouseOverrideMode,
    override_active: bool,
    override_value: f32,
    bipolar: bool,
    /// Last timestamp the user touched the fader — drives port highlight.
    last_interact_time: Option<f64>,
}

impl FaderWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            name: String::new(),
            orientation: FaderOrientation::Vertical,
            output_value: 0.0,
            input_value: 0.0,
            inputs_enabled: false,
            mouse_override: MouseOverrideMode::No,
            override_active: false,
            override_value: 0.0,
            bipolar: false,
            last_interact_time: None,
        }
    }

    fn push_config(&self, mouse_value: Option<f32>, override_value: Option<Option<f32>>) {
        let mut shared = self.shared.lock();
        let mut cfg = serde_json::json!({
            "name": self.name,
        });
        if let Some(v) = mouse_value {
            cfg["mouse_value"] = serde_json::json!(v);
        }
        if let Some(ov) = override_value {
            cfg["override_value"] = match ov {
                Some(v) => serde_json::json!(v),
                None => serde_json::Value::Null,
            };
        }
        shared.pending_config = Some(cfg);
    }

    fn push_settings(&self) {
        self.push_config(None, None);
    }
}

impl NodeWidget for FaderWidget {
    /// Restore port-affecting state directly from save_data. See
    /// `FaderGroupWidget::restore_from_save` for rationale.
    fn restore_from_save(&mut self, data: Option<&serde_json::Value>, params: &[(usize, crate::engine::types::ParamValue)]) {
        // Files from before the settings were params keep it in the config.
        self.inputs_enabled = match param_usize(params, 1) {
            Some(v) => v != 0,
            None => data.and_then(|d| d.get("inputs_enabled")).and_then(|v| v.as_bool()).unwrap_or(false),
        };
    }

    node_identity!("Fader");
    fn title(&self) -> &str { &self.name }
    fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
        self.push_config(None, None);
    }
    fn description(&self) -> &'static str {
        "Draggable fader. Optional signal input with override; double-click resets."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&ports_for(self.inputs_enabled).0)
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&ports_for(self.inputs_enabled).1)
    }

    fn layout(&self) -> NodeLayout { NodeLayout::compact() }
    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 80.0 }
    fn resizable(&self) -> bool { true }

    fn output_highlight(&self, port_idx: usize, now: f64) -> f32 {
        if port_idx != 0 { return 0.0; }
        highlight_alpha(self.last_interact_time, now, HIGHLIGHT_DURATION)
    }

    fn input_highlight(&self, port_idx: usize, now: f64) -> f32 {
        if port_idx != 0 { return 0.0; }
        highlight_alpha(self.last_interact_time, now, HIGHLIGHT_DURATION)
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        // Sync from engine display.
        with_display::<FaderDisplay, _>(&self.shared, |d| {
            self.name = d.name.clone();
            self.orientation = d.orientation;
            self.output_value = d.output;
            self.input_value = d.input;
            self.inputs_enabled = d.inputs_enabled;
            self.mouse_override = d.mouse_override;
            self.override_active = d.override_active;
            self.override_value = d.override_value;
            self.bipolar = d.bipolar;
        });

        let avail = ui.available_size();
        if avail.x <= 0.0 || avail.y <= 0.0 { return; }

        let orient = match self.orientation {
            FaderOrientation::Vertical => fader::Orientation::Vertical,
            FaderOrientation::Horizontal => fader::Orientation::Horizontal,
        };

        let mouse_interactive = !self.inputs_enabled || self.mouse_override.allows_override();
        let sense = if mouse_interactive {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::hover()
        };

        let (response, painter) = ui.allocate_painter(avail, sense);
        let rect = response.rect;

        // Main fill = the value the fader is "tracking":
        //   inputs disabled: mouse_value (= output)
        //   inputs enabled:  input value (always visible; override drawn on top)
        let main_value = if self.inputs_enabled { self.input_value } else { self.output_value };
        let style = FaderStyle::default();
        if self.bipolar {
            painter.rect_filled(rect, style.corner_radius, style.bg);
            fader::draw_bipolar_fill(&painter, rect, main_value, orient, style.fill);
            fader::draw_bipolar_center_line(&painter, rect, orient, CENTER_LINE_COLOR);
            if let Some(b) = style.border {
                painter.rect_stroke(rect, style.corner_radius,
                    egui::Stroke::new(1.0_f32, b), egui::StrokeKind::Inside);
            }
        } else {
            fader::draw_fader(&painter, rect, main_value, orient, &style, false);
        }

        // Override overlay on top: shows where the user-held value sits.
        if self.inputs_enabled && self.override_active {
            if self.bipolar {
                fader::draw_bipolar_fill(&painter, rect, self.override_value, orient, OVERRIDE_OVERLAY_COLOR);
            } else {
                fader::draw_fader_overlay(&painter, rect, self.override_value, orient, OVERRIDE_OVERLAY_COLOR);
            }
            draw_override_indicator(&painter, rect, self.mouse_override, 1.0);
        }

        // Handle interaction.
        if mouse_interactive {
            let mut v = if self.inputs_enabled && self.mouse_override.allows_override() {
                if self.override_active { self.override_value } else { self.input_value }
            } else {
                self.output_value
            };
            let before = v;
            let changed = fader::handle_fader_interaction(ui, &response, rect, orient, &mut v);
            let now = ui.ctx().input(|i| i.time);

            if response.double_clicked() {
                self.last_interact_time = Some(now);
                if self.inputs_enabled && self.mouse_override.allows_override() {
                    // Clear override; fader will resume tracking input.
                    self.push_config(None, Some(None));
                } else {
                    let reset = if self.bipolar { 0.5 } else { 0.0 };
                    self.push_config(Some(reset), None);
                }
            } else if changed || (v - before).abs() > f32::EPSILON {
                self.last_interact_time = Some(now);
                if self.inputs_enabled && self.mouse_override.allows_override() {
                    self.push_config(None, Some(Some(v)));
                } else {
                    self.push_config(Some(v), None);
                }
            }
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Name");
            if ui.text_edit_singleline(&mut self.name).changed() {
                changed = true;
            }
        });
        ui.separator();
        setup_ui::section(ui, "State");
        if self.inputs_enabled {
            ui.horizontal(|ui| {
                ui.label("Input");
                ui.colored_label(theme::TEXT_MUTED, format!("{:.3}", self.input_value));
            });
        }
        if self.override_active {
            ui.horizontal(|ui| {
                ui.label("Override");
                ui.colored_label(theme::SEM_WARNING, format!("{:.3}", self.override_value));
            });
        }
        ui.horizontal(|ui| {
            ui.label("Output");
            ui.colored_label(theme::TEXT, format!("{:.3}", self.output_value));
        });

        if changed { self.push_settings(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_input_port_comes_back_from_either_form_of_a_saved_fader() {
        let old = serde_json::json!({ "inputs_enabled": true });
        let mut f = FaderWidget::new(NodeId(1), new_shared_state(1, 1));
        f.restore_from_save(Some(&old), &[]);
        assert_eq!(f.ui_inputs().len(), 1);
        let mut f = FaderWidget::new(NodeId(1), new_shared_state(1, 1));
        f.restore_from_save(None, &[(1, ParamValue::Bool(true))]);
        assert_eq!(f.ui_inputs().len(), 1);
        let mut f = FaderWidget::new(NodeId(1), new_shared_state(1, 1));
        f.restore_from_save(None, &[]);
        assert!(f.ui_inputs().is_empty());
    }
}
