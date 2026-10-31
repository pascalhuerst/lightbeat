use std::any::Any;
use egui::{self, Color32, Ui};

use crate::engine::nodes::transport::clock_divider::{ClockDividerDisplay, MAX_EXTRA_OUTPUTS};
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct ClockDividerWidget {
    id: NodeId,
    shared: SharedState,
    inputs: Vec<PortDef>,
    /// Divisors of the outputs besides the first. The widget owns them and
    /// sends the list to the engine whenever it changes.
    extra: Vec<u32>,
}

impl ClockDividerWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id, shared,
            inputs: vec![PortDef::new("trigger", PortType::Logic)],
            extra: Vec::new(),
        }
    }

    fn push_extra(&self) {
        self.shared.lock().unwrap().pending_config = Some(serde_json::json!({ "extra": self.extra }));
    }

    /// Takes the extra outputs from the saved form, before wires are
    /// attached, so wires to them still have ports to land on.
    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Some(extra) = data.get("extra").and_then(|v| serde_json::from_value::<Vec<u32>>(v.clone()).ok()) {
            self.extra = extra.into_iter().take(MAX_EXTRA_OUTPUTS).collect();
        }
    }

    fn first_divisor(&self) -> i32 {
        let shared = self.shared.lock().unwrap();
        shared.display.as_ref().and_then(|d| d.downcast_ref::<ClockDividerDisplay>()).map_or(2, |d| d.divisor)
    }
}

impl NodeWidget for ClockDividerWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Clock Divider" }
    fn description(&self) -> &'static str {
        "Emits one trigger for every N input triggers. Press + to add more outputs, each with its own N."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.inputs.iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        std::iter::once(self.first_divisor() as u32)
            .chain(self.extra.iter().copied())
            .map(|d| UiPortDef::from_def(&PortDef::new(format!("1/{d}"), PortType::Logic)))
            .collect()
    }

    fn min_width(&self) -> f32 { 110.0 }
    fn min_content_height(&self) -> f32 { 20.0 }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<ClockDividerDisplay>());

        let (divisor, count) = if let Some(d) = display {
            (d.divisor, d.count)
        } else {
            (2, 0)
        };
        drop(shared);

        ui.horizontal(|ui| {
            ui.label(format!("1/{}", divisor));
            // Show progress dots.
            let progress = format!("{}/{}", count + 1, divisor);
            ui.colored_label(Color32::from_gray(120), progress);
        });

        let mut changed = false;
        let mut remove_last = false;
        let last = self.extra.len().saturating_sub(1);
        for (i, n) in self.extra.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                changed |= ui.add(egui::DragValue::new(n).range(2..=64).prefix("1/")).changed();
                if i == last && ui.small_button(egui_phosphor::regular::X).on_hover_text("Remove this output").clicked() {
                    remove_last = true;
                }
            });
        }
        if self.extra.len() < MAX_EXTRA_OUTPUTS
            && ui.small_button(egui_phosphor::regular::PLUS).on_hover_text("Add an output").clicked()
        {
            // A new output starts out twice as slow as the one before it.
            let previous = self.extra.last().copied().unwrap_or(divisor as u32);
            self.extra.push((previous * 2).min(64));
            changed = true;
        }
        if remove_last {
            self.extra.pop();
            changed = true;
        }
        if changed {
            self.push_extra();
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outputs_are_named_by_their_divisor() {
        let mut w = ClockDividerWidget::new(NodeId(1), new_shared_state(1, 16));
        w.restore_from_save_data(&serde_json::json!({ "extra": [4, 8] }));
        let names: Vec<String> = w.ui_outputs().into_iter().map(|p| p.def.name).collect();
        assert_eq!(names, ["1/2", "1/4", "1/8"]);
    }
}
