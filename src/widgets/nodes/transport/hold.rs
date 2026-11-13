use egui::{self, Ui};

use crate::engine::nodes::transport::hold::TriggerHoldDisplay;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeWidget, node_identity, with_display};
use crate::widgets::nodes::types::UiPortDef;

pub struct TriggerHoldWidget {
    id: NodeId,
    shared: SharedState,
}

impl TriggerHoldWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for TriggerHoldWidget {
    node_identity!("Trigger Hold");
    fn description(&self) -> &'static str {
        "Holds a trigger high for a number of milliseconds after each rising edge. A new node has Duration as an input; switch it off in the inspector to type a value."
    }

    fn default_promoted(&self) -> &'static [(&'static str, PromotedKind)] {
        &[("Duration", PromotedKind::Float)]
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::transport::hold::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::transport::hold::ports().1) }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 28.0 }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let (remaining, dur) = with_display::<TriggerHoldDisplay, _>(&self.shared, |d| (d.remaining_ms, d.effective_duration_ms))
            .unwrap_or((0.0, 0));

        ui.horizontal(|ui| {
            if remaining > 0.0 {
                ui.colored_label(
                    theme::SEM_SUCCESS,
                    format!("HIGH {:.0}/{} ms", remaining, dur),
                );
            } else {
                ui.colored_label(
                    theme::TEXT_DIM,
                    format!("idle ({} ms)", dur),
                );
            }
        });
    }
}
