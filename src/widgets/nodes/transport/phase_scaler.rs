use egui::{self, Ui};

use crate::engine::nodes::transport::phase_scaler::{ports_for, ExtraOutput, MAX_EXTRA_OUTPUTS, PhaseScalerDisplay};
use crate::engine::types::*;
use crate::widgets::nodes::node::{NodeWidget, node_identity};
use crate::widgets::setup_ui;
use crate::widgets::nodes::types::UiPortDef;

pub struct PhaseScalerWidget {
    id: NodeId,
    shared: SharedState,
    /// Outputs besides the first, each with its own ratio. The widget owns
    /// them and sends the list to the engine whenever it changes.
    extra: Vec<ExtraOutput>,
}

impl PhaseScalerWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            extra: Vec::new(),
        }
    }

    fn push_extra(&self) {
        self.shared.lock().pending_config = Some(serde_json::json!({ "extra": self.extra }));
    }

    fn first_exponent(&self) -> i32 {
        let shared = self.shared.lock();
        shared.display.as_ref().and_then(|d| d.downcast_ref::<PhaseScalerDisplay>()).map(|d| d.exponent).unwrap_or(0)
    }
}

fn exponent_label(exp: i32) -> String {
    match exp {
        0 => "×1".into(),
        e if e > 0 => format!("×{}", 1u64 << e),
        e => format!("÷{}", 1u64 << (-e)),
    }
}

impl NodeWidget for PhaseScalerWidget {
    /// Takes the extra outputs from the saved form, before wires are
    /// attached, so wires to them still have ports to land on.
    fn restore_from_save(&mut self, data: Option<&serde_json::Value>, _params: &[(usize, crate::engine::types::ParamValue)]) {
        let Some(data) = data else { return };
        if let Some(extra) = data.get("extra").and_then(|v| serde_json::from_value::<Vec<ExtraOutput>>(v.clone()).ok()) {
            self.extra = extra.into_iter().take(MAX_EXTRA_OUTPUTS).collect();
        }
    }

    node_identity!("Phase Scaler");
    fn description(&self) -> &'static str {
        "Multiplies or divides phase rate by a power of two to speed up or \
         slow down cycles. A rising edge on `reset` resyncs the sub-cycle \
         counter — useful to keep a divided phase aligned to an external \
         downbeat or a detected onset. Wiring the `exp` input overrides \
         the param: its value is taken directly as the exponent \
         (rounded, clamped to -6..=6), so feeding a Phase Accumulator or \
         other integer source works without any scaling. Press + to add \
         more outputs, each with its own ratio, all following the same \
         input and reset; the exp and offset inputs drive the first one."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&ports_for(self.extra.len()).0)
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        // Each output is named by what it does to the phase.
        let mut ports = UiPortDef::from_defs(&ports_for(self.extra.len()).1);
        let exponents = std::iter::once(self.first_exponent()).chain(self.extra.iter().map(|x| x.exponent));
        for (port, e) in ports.iter_mut().zip(exponents) {
            port.def.name = exponent_label(e);
        }
        ports
    }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 20.0 }


    /// When the `exp` / `offset` inputs are wired the corresponding params
    /// are overridden by the engine. Hide them from the auto-inspector so
    /// the user isn't editing values that get thrown away.
    fn overridden_param_indices(&self) -> Vec<usize> {
        let shared = self.shared.lock();
        let exp_wired = shared.inputs_connected.get(2).copied().unwrap_or(false);
        let offset_wired = shared.inputs_connected.get(3).copied().unwrap_or(false);
        drop(shared);
        let mut v = Vec::new();
        if exp_wired { v.push(0); }     // param 0 = Exponent
        if offset_wired { v.push(1); }  // param 1 = Offset
        v
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let exponent = self.first_exponent();
        let mut changed = false;
        let mut remove_last = false;
        let mut add = false;

        ui.horizontal(|ui| {
            if ui.small_button("÷2").clicked() && exponent > -6 {
                self.shared.lock().pending_params.push((0, ParamValue::Int((exponent - 1) as i64)));
            }
            ui.label(exponent_label(exponent));
            if ui.small_button("×2").clicked() && exponent < 6 {
                self.shared.lock().pending_params.push((0, ParamValue::Int((exponent + 1) as i64)));
            }
        });
        let last = self.extra.len().saturating_sub(1);
        for (i, x) in self.extra.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui.small_button("÷2").clicked() && x.exponent > -6 {
                    x.exponent -= 1;
                    changed = true;
                }
                ui.label(exponent_label(x.exponent));
                if ui.small_button("×2").clicked() && x.exponent < 6 {
                    x.exponent += 1;
                    changed = true;
                }
                if i == last && ui.small_button(egui_phosphor::regular::X).on_hover_text("Remove this output").clicked() {
                    remove_last = true;
                }
            });
        }
        if self.extra.len() < MAX_EXTRA_OUTPUTS
            && ui.small_button(egui_phosphor::regular::PLUS).on_hover_text("Add an output").clicked()
        {
            add = true;
        }

        if remove_last {
            self.extra.pop();
            changed = true;
        }
        if add {
            // A new output starts one step slower than the one before it.
            let previous = self.extra.last().map_or(exponent, |x| x.exponent);
            self.extra.push(ExtraOutput { exponent: (previous - 1).max(-6), offset: 0.0 });
            changed = true;
        }
        if changed {
            self.push_extra();
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        if self.extra.is_empty() {
            return;
        }
        setup_ui::section(ui, "Extra outputs");
        let mut changed = false;
        for (i, x) in self.extra.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{}:", i + 2));
                changed |= ui.add(egui::Slider::new(&mut x.exponent, -6..=6).text("exponent")).changed();
                changed |= ui.add(egui::Slider::new(&mut x.offset, 0.0..=1.0).text("offset")).changed();
            });
        }
        if changed {
            self.push_extra();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outputs_are_named_by_their_ratio_and_come_back_from_the_saved_form() {
        let mut w = PhaseScalerWidget::new(NodeId(1), new_shared_state(4, 16));
        w.restore_from_save(Some(&serde_json::json!({ "extra": [{"exponent": -1, "offset": 0.0}, {"exponent": 2, "offset": 0.0}] })), &[]);
        let names: Vec<String> = w.ui_outputs().into_iter().map(|p| p.def.name).collect();
        assert_eq!(names, ["×1", "÷2", "×4"]);
        w.push_extra();
        let sent = w.shared.lock().pending_config.take().unwrap();
        assert_eq!(sent["extra"].as_array().map(Vec::len), Some(2));
    }
}
