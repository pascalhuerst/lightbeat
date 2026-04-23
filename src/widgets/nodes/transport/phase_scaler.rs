use std::any::Any;

use egui::Ui;

use crate::engine::nodes::transport::phase_scaler::PhaseScalerDisplay;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct PhaseScalerWidget {
    id: NodeId,
    shared: SharedState,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl PhaseScalerWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            inputs: vec![
                PortDef::new("phase", PortType::Phase),
                PortDef::new("reset", PortType::Logic),
            ],
            outputs: vec![PortDef::new("phase", PortType::Phase)],
        }
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
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Phase Scaler" }
    fn description(&self) -> &'static str {
        "Multiplies or divides phase rate by a power of two to speed up or \
         slow down cycles. A rising edge on `reset` resyncs the sub-cycle \
         counter — useful to keep a divided phase aligned to an external \
         downbeat or a detected onset."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.inputs.iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.outputs.iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 20.0 }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let exponent = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<PhaseScalerDisplay>())
            .map(|d| d.exponent)
            .unwrap_or(0);
        drop(shared);

        ui.horizontal(|ui| {
            if ui.small_button("÷2").clicked() && exponent > -6 {
                self.shared.lock().unwrap().pending_params.push((0, ParamValue::Int((exponent - 1) as i64)));
            }
            ui.label(exponent_label(exponent));
            if ui.small_button("×2").clicked() && exponent < 6 {
                self.shared.lock().unwrap().pending_params.push((0, ParamValue::Int((exponent + 1) as i64)));
            }
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
