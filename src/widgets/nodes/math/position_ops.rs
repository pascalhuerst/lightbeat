use std::any::Any;
use egui::Ui;

use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct PositionMergeWidget {
    id: NodeId,
    shared: SharedState,
}

impl PositionMergeWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for PositionMergeWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Pan Tilt to Position" }
    fn description(&self) -> &'static str { "Combines pan and tilt values into a single position output." }
    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::position_ops::merge_ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::position_ops::merge_ports().1) }
    fn min_width(&self) -> f32 { 90.0 }
    fn min_content_height(&self) -> f32 { 10.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }
    fn show_content(&mut self, _ui: &mut Ui, _zoom: f32) {}
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

pub struct PositionSplitWidget {
    id: NodeId,
    shared: SharedState,
}

impl PositionSplitWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared }
    }
}

impl NodeWidget for PositionSplitWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Position to Pan Tilt" }
    fn description(&self) -> &'static str { "Splits a position into separate pan and tilt component outputs." }
    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::position_ops::split_ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::position_ops::split_ports().1) }
    fn min_width(&self) -> f32 { 90.0 }
    fn min_content_height(&self) -> f32 { 10.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }
    fn show_content(&mut self, _ui: &mut Ui, _zoom: f32) {}
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
