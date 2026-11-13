use egui::{self, Ui};

use crate::engine::nodes::transport::delay::TriggerDelayDisplay;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeWidget, node_identity, with_display};
use crate::widgets::nodes::types::UiPortDef;

pub struct TriggerDelayWidget {
    id: NodeId,
    shared: SharedState,
}

impl TriggerDelayWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
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
    node_identity!("Trigger Delay");
    fn description(&self) -> &'static str { "Delays incoming triggers by a power-of-two fraction or multiple of a beat (Exponent), counted on the phase input, before passing them through." }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::transport::delay::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::transport::delay::ports().1) }

    fn min_width(&self) -> f32 { 130.0 }
    fn min_content_height(&self) -> f32 { 20.0 }


    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let (exponent, pending) = with_display::<TriggerDelayDisplay, _>(&self.shared, |d| (d.exponent, d.has_pending)).unwrap_or((-2, false));

        ui.horizontal(|ui| {
            if ui.small_button("÷2").clicked() && exponent > -6 {
                self.shared.lock().unwrap()
                    .pending_params
                    .push((0, ParamValue::Int((exponent - 1) as i64)));
            }

            let color = if pending {
                theme::TYPE_LOGIC
            } else {
                theme::TEXT_MUTED
            };
            ui.colored_label(color, delay_label(exponent));

            if ui.small_button("×2").clicked() && exponent < 6 {
                self.shared.lock().unwrap()
                    .pending_params
                    .push((0, ParamValue::Int((exponent + 1) as i64)));
            }
        });
    }
}
