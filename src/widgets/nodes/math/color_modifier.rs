use egui::{self, Ui};

use crate::engine::nodes::math::color_modifier::{
    ports_for, ColorModifierDisplay, MODIFIER_OP_NAMES, ModifierOp,
};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeWidget, node_identity, with_display};
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
        let mut s = self.shared.lock();
        s.pending_params.push((0, ParamValue::Choice(idx)));
    }

    fn sync_from_display(&mut self) {
        with_display::<ColorModifierDisplay, _>(&self.shared, |d| {
            if d.port_type != self.port_type { self.port_type = d.port_type; }
            if d.op != self.op { self.op = d.op; }
        });
    }
}

impl NodeWidget for ColorModifierWidget {
    node_identity!("Color Modifier");
    fn description(&self) -> &'static str {
        "Applies a HSV / brightness / alpha operation to a Color or Gradient signal. For gradients the op is applied to every active stop. Amount is 1.0 = identity for scale ops, 0.0 = identity for Hue Shift."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&ports_for(self.port_type).0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&ports_for(self.port_type).1) }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 25.0 }

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
            let shared = self.shared.lock();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<ColorModifierDisplay>())
                .map(|d| d.amount).unwrap_or(0.0)
        };
        if self.port_type == PortType::Any {
            ui.colored_label(theme::TEXT_DIM, "Connect to set type");
        } else {
            ui.colored_label(
                theme::TEXT_DIM,
                format!("{} · {:.2}", MODIFIER_OP_NAMES[self.op.to_index()], amount),
            );
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        self.sync_from_display();
        ui.colored_label(theme::TEXT_DIM, match self.op {
            ModifierOp::HueShift => "amount = hue rotation (0..1 = full turn)",
            ModifierOp::Saturation => "amount = saturation multiplier (1.0 = pass-through)",
            ModifierOp::Value => "amount = value/brightness multiplier (HSV V)",
            ModifierOp::Brightness => "amount = RGB multiplier (linear scale)",
            ModifierOp::Alpha => "amount = alpha multiplier (Gradient stops only)",
        });
    }
}
