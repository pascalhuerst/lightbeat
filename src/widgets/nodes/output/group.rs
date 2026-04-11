use std::any::Any;
use egui::{self, Color32, Ui};

use crate::engine::nodes::output::group::GroupNodeDisplay;
use crate::engine::types::*;
use crate::objects::group::GroupCapability;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct GroupWidget {
    id: NodeId,
    shared: SharedState,
    group_name: String,
    capabilities: Vec<GroupCapability>,
}

impl GroupWidget {
    pub fn new(id: NodeId, shared: SharedState, group_name: String, capabilities: Vec<GroupCapability>) -> Self {
        Self { id, shared, group_name, capabilities }
    }

    pub fn group_name(&self) -> String {
        self.group_name.clone()
    }

    fn build_inputs(caps: &[GroupCapability]) -> Vec<UiPortDef> {
        caps.iter().map(|cap| {
            let (name, pt) = match cap {
                GroupCapability::Dimmer => ("dimmer", PortType::Untyped),
                GroupCapability::Color => ("color", PortType::Color),
                GroupCapability::Position => ("position", PortType::Position),
            };
            UiPortDef::from_def(&PortDef::new(name, pt))
        }).collect()
    }
}

impl NodeWidget for GroupWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Group" }
    fn title(&self) -> &str { &self.group_name }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        Self::build_inputs(&self.capabilities)
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 130.0 }
    fn min_content_height(&self) -> f32 { 20.0 }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<GroupNodeDisplay>());

        if let Some(d) = display {
            self.group_name = d.group_name.clone();
            self.capabilities = d.capabilities.clone();

            ui.colored_label(
                Color32::from_gray(140),
                format!("{} fixtures", d.fixture_count),
            );
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
