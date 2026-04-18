use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::math::color_modifier::{
    ColorModifierDisplay, MODIFIER_OP_NAMES, ModifierOp,
};
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct ColorModifierWidget {
    id: NodeId,
    shared: SharedState,
    port_type: PortType,
    op: ModifierOp,
}

impl ColorModifierWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, port_type: PortType::Any, op: ModifierOp::HueShift }
    }

    fn push_type(&self) {
        let idx = match self.port_type {
            PortType::Color => 1,
            PortType::Gradient => 2,
            _ => 0,
        };
        let mut s = self.shared.lock().unwrap();
        s.pending_params.push((0, ParamValue::Choice(idx)));
    }

    fn sync_from_display(&mut self) {
        let shared = self.shared.lock().unwrap();
        if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<ColorModifierDisplay>()) {
            if d.port_type != self.port_type { self.port_type = d.port_type; }
            if d.op != self.op { self.op = d.op; }
        }
    }
}

impl NodeWidget for ColorModifierWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color Modifier" }
    fn title(&self) -> &str { "Color Modifier" }
    fn description(&self) -> &'static str {
        "Applies a HSV / brightness / alpha operation to a Color or Gradient signal. For gradients the op is applied to every active stop. Amount is 1.0 = identity for scale ops, 0.0 = identity for Hue Shift."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        let main_name = match self.port_type {
            PortType::Color => "color",
            PortType::Gradient => "gradient",
            _ => "?",
        };
        vec![
            UiPortDef::from_def(&PortDef::new(main_name, self.port_type)),
            UiPortDef::from_def(&PortDef::new("amount", PortType::Untyped)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        let main_name = match self.port_type {
            PortType::Color => "color",
            PortType::Gradient => "gradient",
            _ => "?",
        };
        vec![UiPortDef::from_def(&PortDef::new(main_name, self.port_type))]
    }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 25.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn on_ui_connect(&mut self, input_port: usize, source_type: PortType) {
        // Only the main input (port 0) carries type info; amount is Untyped.
        if input_port != 0 { return; }
        if self.port_type == PortType::Any {
            let new_pt = match source_type {
                PortType::Color => PortType::Color,
                PortType::Gradient => PortType::Gradient,
                _ => return,
            };
            self.port_type = new_pt;
            self.push_type();
        }
    }
    fn on_ui_output_connect(&mut self, _output_port: usize, dest_type: PortType) {
        if self.port_type == PortType::Any {
            let new_pt = match dest_type {
                PortType::Color => PortType::Color,
                PortType::Gradient => PortType::Gradient,
                _ => return,
            };
            self.port_type = new_pt;
            self.push_type();
        }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.sync_from_display();
        let amount = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<ColorModifierDisplay>())
                .map(|d| d.amount).unwrap_or(0.0)
        };
        if self.port_type == PortType::Any {
            ui.colored_label(Color32::from_gray(120), "Connect to set type");
        } else {
            ui.colored_label(
                Color32::from_gray(140),
                format!("{} · {:.2}", MODIFIER_OP_NAMES[self.op.to_index()], amount),
            );
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        self.sync_from_display();
        ui.colored_label(egui::Color32::from_gray(140), match self.op {
            ModifierOp::HueShift => "amount = hue rotation (0..1 = full turn)",
            ModifierOp::Saturation => "amount = saturation multiplier (1.0 = pass-through)",
            ModifierOp::Value => "amount = value/brightness multiplier (HSV V)",
            ModifierOp::Brightness => "amount = RGB multiplier (linear scale)",
            ModifierOp::Alpha => "amount = alpha multiplier (Gradient stops only)",
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
