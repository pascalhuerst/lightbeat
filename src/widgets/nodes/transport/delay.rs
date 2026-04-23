use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::transport::delay::TriggerDelayDisplay;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct TriggerDelayWidget {
    id: NodeId,
    shared: SharedState,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl TriggerDelayWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            inputs: vec![
                PortDef::new("trigger", PortType::Logic),
                PortDef::new("phase", PortType::Phase),
            ],
            outputs: vec![PortDef::new("trigger", PortType::Logic)],
        }
    }
}

fn delay_label(exp: i32) -> String {
    match exp {
        0 => "1 beat".into(),
        e if e > 0 => format!("{} beats", 1u64 << e),
        e => format!("1/{} beat", 1u64 << (-e)),
    }
}

impl NodeWidget for TriggerDelayWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Trigger Delay" }
    fn title(&self) -> &str { "Trigger Delay" }
    fn description(&self) -> &'static str { "Delays incoming triggers by a beat-aligned amount before passing them through." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.inputs.iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.outputs.iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 130.0 }
    fn min_content_height(&self) -> f32 { 20.0 }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<TriggerDelayDisplay>());

        let (exponent, pending) = if let Some(d) = display {
            (d.exponent, d.has_pending)
        } else {
            (-2, false)
        };
        drop(shared);

        ui.horizontal(|ui| {
            if ui.small_button("÷2").clicked() && exponent > -6 {
                self.shared.lock().unwrap()
                    .pending_params
                    .push((0, ParamValue::Int((exponent - 1) as i64)));
            }

            let color = if pending {
                theme::TYPE_LOGIC
            } else {
                Color32::from_gray(180)
            };
            ui.colored_label(color, delay_label(exponent));

            if ui.small_button("×2").clicked() && exponent < 6 {
                self.shared.lock().unwrap()
                    .pending_params
                    .push((0, ParamValue::Int((exponent + 1) as i64)));
            }
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
