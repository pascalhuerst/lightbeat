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
    fn type_name(&self) -> &'static str { "Position Merge" }
    fn description(&self) -> &'static str { "Combines pan and tilt values into a single position output." }
    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("Pan", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("Tilt", PortType::Untyped)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("position", PortType::Position))]
    }
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
    fn type_name(&self) -> &'static str { "Position Split" }
    fn description(&self) -> &'static str { "Splits a position into separate pan and tilt component outputs." }
    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("position", PortType::Position))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("Pan", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("Tilt", PortType::Untyped)),
        ]
    }
    fn min_width(&self) -> f32 { 90.0 }
    fn min_content_height(&self) -> f32 { 10.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }
    fn show_content(&mut self, _ui: &mut Ui, _zoom: f32) {}
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
