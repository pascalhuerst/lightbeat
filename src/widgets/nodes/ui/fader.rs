use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::ui::common::MouseOverrideMode;
use crate::engine::nodes::ui::fader::{FaderDisplay, FaderOrientation};
use crate::engine::types::*;
use crate::widgets::fader::{self, highlight_alpha, FaderStyle};
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const HIGHLIGHT_DURATION: f64 = 0.5;
/// Semi-transparent overlay color showing the override value while it's active.
const OVERRIDE_OVERLAY_COLOR: Color32 = Color32::from_rgba_premultiplied(220, 150, 40, 130);
const OVERRIDE_ICON_COLOR: Color32 = Color32::from_rgb(255, 180, 60);
const CENTER_LINE_COLOR: Color32 = Color32::from_gray(110);

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
        let mut shared = self.shared.lock().unwrap();
        let mut cfg = serde_json::json!({
            "orientation": self.orientation.as_str(),
            "inputs_enabled": self.inputs_enabled,
            "mouse_override": self.mouse_override.as_str(),
            "bipolar": self.bipolar,
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

    /// Restore port-affecting state directly from save_data. See
    /// `FaderGroupWidget::restore_from_save_data` for rationale.
    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Some(b) = data.get("inputs_enabled").and_then(|v| v.as_bool()) {
            self.inputs_enabled = b;
        }
    }
}

impl NodeWidget for FaderWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Fader" }
    fn title(&self) -> &str { "Fader" }
    fn description(&self) -> &'static str {
        "Draggable fader. Optional signal input with override; double-click resets."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        if self.inputs_enabled {
            vec![UiPortDef::from_def(&PortDef::new("in", PortType::Untyped))]
        } else {
            vec![]
        }
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", PortType::Untyped))]
    }

    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 80.0 }
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

    fn inspector_hides_default_ports(&self) -> bool { true }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        // Sync from engine display.
        {
            let shared = self.shared.lock().unwrap();
            if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<FaderDisplay>()) {
                self.orientation = d.orientation;
                self.output_value = d.output;
                self.input_value = d.input;
                self.inputs_enabled = d.inputs_enabled;
                self.mouse_override = d.mouse_override;
                self.override_active = d.override_active;
                self.override_value = d.override_value;
                self.bipolar = d.bipolar;
            }
        }

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
                    egui::Stroke::new(1.0, b), egui::StrokeKind::Inside);
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
        // Orientation
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Orientation:");
            if ui.radio_value(&mut self.orientation, FaderOrientation::Vertical, "Vertical").clicked() {
                changed = true;
            }
            if ui.radio_value(&mut self.orientation, FaderOrientation::Horizontal, "Horizontal").clicked() {
                changed = true;
            }
        });

        ui.separator();
        ui.label(egui::RichText::new("Output").strong());
        if cell_inspector_section(
            ui,
            self.id.0 as u64,
            None,
            &mut self.inputs_enabled,
            &mut self.mouse_override,
            &mut self.bipolar,
            self.input_value,
            self.override_active,
            self.override_value,
            self.output_value,
        ) {
            changed = true;
        }

        if changed { self.push_settings(); }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

/// Render one combined per-cell control block in the inspector.
/// Returns true if any setting changed.
pub fn cell_inspector_section(
    ui: &mut Ui,
    salt: u64,
    label: Option<&str>,
    inputs_enabled: &mut bool,
    mouse_override: &mut MouseOverrideMode,
    bipolar: &mut bool,
    input_value: f32,
    override_active: bool,
    override_value: f32,
    output_value: f32,
) -> bool {
    let mut changed = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        if let Some(l) = label {
            ui.label(egui::RichText::new(l).strong());
        }
        if ui.checkbox(inputs_enabled, "Input enabled").changed() {
            changed = true;
        }
        if *inputs_enabled {
            ui.horizontal(|ui| {
                ui.label("Input:");
                ui.colored_label(Color32::from_gray(180), format!("{:.3}", input_value));
            });
        }
        ui.horizontal(|ui| {
            ui.label("Mouse override:");
            egui::ComboBox::from_id_salt(("mo", salt))
                .selected_text(mouse_override.label())
                .show_ui(ui, |ui| {
                    for m in [
                        MouseOverrideMode::No,
                        MouseOverrideMode::ClearOnReset,
                        MouseOverrideMode::PickupDecrease,
                        MouseOverrideMode::PickupIncrease,
                    ] {
                        if ui.selectable_label(*mouse_override == m, m.label()).clicked() {
                            *mouse_override = m;
                            changed = true;
                        }
                    }
                });
        });
        if override_active {
            ui.horizontal(|ui| {
                ui.label("Override:");
                ui.colored_label(Color32::from_rgb(220, 150, 40), format!("{:.3}", override_value));
            });
        }
        ui.horizontal(|ui| {
            ui.label("Output:");
            ui.colored_label(Color32::from_gray(220), format!("{:.3}", output_value));
        });
        if ui.checkbox(bipolar, "Bipolar").changed() {
            changed = true;
        }
    });
    changed
}
