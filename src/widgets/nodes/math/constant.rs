use std::any::Any;

use egui::{self, Ui};

use crate::engine::nodes::math::constant::ConstantProcessNode;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct ConstantWidget {
    id: NodeId,
    port_type: PortType,
    shared: SharedState,
    outputs: Vec<PortDef>,
    /// The value being edited; the engine's copy is only mirrored every few
    /// ticks, so it would fight a drag.
    value: f32,
    last_edit: f64,
    /// A value was set before the first frame; hold it until the engine has it.
    fresh: bool,
}

impl ConstantWidget {
    pub fn new(id: NodeId, port_type: PortType, shared: SharedState) -> Self {
        Self {
            id,
            port_type,
            shared,
            outputs: vec![PortDef::new("out", port_type)],
            value: 0.0,
            last_edit: f64::NEG_INFINITY,
            fresh: false,
        }
    }

    fn push_value(&self) {
        self.shared.lock().unwrap().pending_params.push((0, ParamValue::Float(self.value)));
    }
}

impl NodeWidget for ConstantWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { ConstantProcessNode::type_name_for(self.port_type) }
    fn set_initial_value(&mut self, value: f32) {
        self.value = value;
        self.fresh = true;
        self.push_value();
    }
    fn description(&self) -> &'static str { "Constant value source for any port type, edited via the inspector." }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.outputs.iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 90.0 }
    fn min_content_height(&self) -> f32 { 15.0 }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let now = ui.input(|i| i.time);
        // Take the engine's value unless the user has just set one.
        if std::mem::take(&mut self.fresh) {
            self.last_edit = now;
        }
        if now - self.last_edit > 0.5 {
            let engine = self.shared.lock().unwrap().outputs.first().copied();
            if let Some(v) = engine {
                self.value = v;
            }
        }

        let changed = match self.port_type {
            PortType::Logic => {
                let label = if self.value >= 0.5 { "HIGH" } else { "LOW" };
                let clicked = ui.button(label).clicked();
                if clicked {
                    self.value = if self.value >= 0.5 { 0.0 } else { 1.0 };
                }
                clicked
            }
            PortType::Phase => ui
                .add(egui::DragValue::new(&mut self.value).range(0.0..=1.0).speed(0.005).fixed_decimals(3))
                .changed(),
            _ => ui
                .add(egui::DragValue::new(&mut self.value).speed(0.01).max_decimals(6))
                .changed(),
        };
        if changed {
            self.last_edit = now;
            self.push_value();
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
