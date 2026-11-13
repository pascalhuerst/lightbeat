use egui::{self, Ui};

use crate::engine::nodes::ui::button::{ports_for, ButtonDisplay, ButtonMode};
use crate::engine::nodes::ui::common::MouseOverrideMode;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::fader::highlight_alpha;
use crate::widgets::nodes::node::{param_usize, NodeLayout, NodeWidget, node_identity, with_display};
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
            "click_id": self.click_id,
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
    /// Restore the port-affecting state (specifically `inputs_enabled`,
    /// which gates whether `ui_inputs()` exposes a port) directly from
    /// saved data. Called by the project loader *before*
    /// `cleanup_stale_connections` runs, so a wire targeting the button's
    /// input port isn't dropped on the first frame while we're still
    /// waiting for the engine to finish applying `load_data`.
    fn restore_from_save(&mut self, data: Option<&serde_json::Value>, params: &[(usize, crate::engine::types::ParamValue)]) {
        // Files from before the settings were params keep it in the config.
        self.inputs_enabled = match param_usize(params, 1) {
            Some(v) => v != 0,
            None => data.and_then(|d| d.get("inputs_enabled")).and_then(|v| v.as_bool()).unwrap_or(false),
        };
    }

    fn param_display(&self, name: &str) -> (Option<String>, String) {
        (None, if name == "Button mode" { "Mode".to_string() } else { name.to_string() })
    }

    /// The mouse override only exists on a toggle that has an input, and its
    /// reset only when the override is allowed.
    fn overridden_param_indices(&self) -> Vec<usize> {
        let applies = self.inputs_enabled && self.mode == ButtonMode::Toggle;
        let mut hidden = Vec::new();
        if !applies { hidden.push(2); }
        if !(applies && self.override_enabled) { hidden.push(3); }
        hidden
    }

    node_identity!("Button");
    fn title(&self) -> &str { &self.label }
    fn description(&self) -> &'static str {
        "Clickable button outputting a trigger pulse or persistent toggle state."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&ports_for(self.inputs_enabled).0)
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&ports_for(self.inputs_enabled).1)
    }

    // The button is the node: no header, it is moved by dragging it, and its
    // label is on the button.
    fn layout(&self) -> NodeLayout { NodeLayout::compact().headerless() }
    fn set_name(&mut self, name: &str) {
        self.label = name.to_string();
        self.push_settings();
    }
    fn min_width(&self) -> f32 { 90.0 }
    fn min_content_height(&self) -> f32 { 26.0 }
    fn resizable(&self) -> bool { true }

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
        with_display::<ButtonDisplay, _>(&self.shared, |d| {
            self.label = d.label.clone();
            self.mode = d.mode;
            self.state = d.state;
            self.input_value = d.input_value;
            self.inputs_enabled = d.inputs_enabled;
            self.override_enabled = d.override_enabled;
            self.override_active = d.override_active;
            self.reset_mode = d.reset_mode;
        });

        let pressed = matches!(self.mode, ButtonMode::Toggle) && self.state;
        let fill = if pressed {
            theme::SEM_PRIMARY
        } else {
            theme::OFF_GREY
        };
        let text_color = if pressed { theme::ON_LIGHT } else { theme::TEXT_BRIGHT };

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
            theme::TEXT_DIM
        } else {
            theme::PLOT_FRAME
        };
        let border_width = if self.override_active { 2.0 } else { 1.0_f32 };
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
                theme::STROKE
            };
            painter.circle_filled(dot_pos, dot_r, in_color);
            painter.circle_stroke(dot_pos, dot_r,
                egui::Stroke::new(1.0_f32, theme::SEM_WARNING));
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
            ui.label("Name");
            if ui.text_edit_singleline(&mut self.label).changed() {
                changed = true;
            }
        });

        if self.inputs_enabled && self.mode == ButtonMode::Trigger {
            ui.colored_label(
                theme::TEXT_DIM,
                "Trigger mode: input rising edges fire the output. No override.",
            );
        }

        if changed { self.push_settings(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::nodes::node::node_size;

    #[test]
    fn a_button_is_just_the_button_with_the_output_on_its_edge() {
        let b = ButtonWidget::new(NodeId(1), new_shared_state(1, 1));
        let l = b.layout();
        assert!(!l.toggle && !l.show_title && l.bare && l.drag_by_body);
        assert_eq!(node_size(&b, None), egui::Vec2::new(90.0, 26.0));
    }

    #[test]
    fn a_resized_button_keeps_filling_its_node() {
        let b = ButtonWidget::new(NodeId(1), new_shared_state(1, 1));
        let size = node_size(&b, Some(egui::Vec2::new(200.0, 70.0)));
        assert_eq!(size, egui::Vec2::new(200.0, 70.0));
        // Smaller than the minimum is raised to it.
        assert_eq!(node_size(&b, Some(egui::Vec2::new(10.0, 10.0))), egui::Vec2::new(90.0, 26.0));
    }

    #[test]
    fn the_input_port_comes_back_from_either_form_of_a_saved_button() {
        let old = serde_json::json!({ "inputs_enabled": true });
        let mut b = ButtonWidget::new(NodeId(1), new_shared_state(1, 1));
        b.restore_from_save(Some(&old), &[]);
        assert_eq!(b.ui_inputs().len(), 1);
        let mut b = ButtonWidget::new(NodeId(1), new_shared_state(1, 1));
        b.restore_from_save(None, &[(1, ParamValue::Bool(true))]);
        assert_eq!(b.ui_inputs().len(), 1);
        let mut b = ButtonWidget::new(NodeId(1), new_shared_state(1, 1));
        b.restore_from_save(None, &[]);
        assert!(b.ui_inputs().is_empty());
    }
}
