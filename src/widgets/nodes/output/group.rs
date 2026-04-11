use std::any::Any;
use std::sync::{Arc, Mutex};

use egui::{self, Color32, Ui};

use crate::engine::nodes::output::group::GroupNodeDisplay;
use crate::engine::types::*;
use crate::objects::group::Group;
use crate::objects::object::Object;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

/// Shared reference to groups and objects for the widget to read.
pub struct GroupOutputContext {
    pub groups: Vec<Group>,
    pub objects: Vec<Object>,
}

pub type SharedGroupContext = Arc<Mutex<GroupOutputContext>>;

pub fn new_shared_group_context() -> SharedGroupContext {
    Arc::new(Mutex::new(GroupOutputContext {
        groups: Vec::new(),
        objects: Vec::new(),
    }))
}

pub struct GroupWidget {
    id: NodeId,
    shared: SharedState,
    group_ctx: SharedGroupContext,
    /// Which group IDs are selected.
    selected_group_ids: Vec<u32>,
}

impl GroupWidget {
    pub fn new(id: NodeId, shared: SharedState, group_ctx: SharedGroupContext) -> Self {
        Self {
            id,
            shared,
            group_ctx,
            selected_group_ids: Vec::new(),
        }
    }

    /// Push the current group selection to the engine via shared state.
    fn push_config_to_engine(&self) {
        let ctx = self.group_ctx.lock().unwrap();

        let mut object_ids = Vec::new();
        let mut group_names = Vec::new();
        for gid in &self.selected_group_ids {
            if let Some(group) = ctx.groups.iter().find(|g| g.id == *gid) {
                group_names.push(group.name.clone());
                for oid in &group.object_ids {
                    if !object_ids.contains(oid) {
                        object_ids.push(*oid);
                    }
                }
            }
        }

        let config = serde_json::json!({
            "group_ids": self.selected_group_ids,
            "group_names": group_names,
            "object_ids": object_ids,
        });

        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(config);
    }
}

impl NodeWidget for GroupWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Group Output" }
    fn title(&self) -> &str { "Group Output" }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("palette", PortType::ColorStack)),
            UiPortDef::from_def(&PortDef::new("dimmer", PortType::Untyped)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 130.0 }
    fn min_content_height(&self) -> f32 { 20.0 }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<GroupNodeDisplay>());

        let obj_count = display.map(|d| d.object_count).unwrap_or(0);
        drop(shared);

        if self.selected_group_ids.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No groups");
        } else {
            ui.colored_label(Color32::from_gray(140), format!("{} groups, {} obj", self.selected_group_ids.len(), obj_count));
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        ui.label(egui::RichText::new("Groups").strong());

        let ctx = self.group_ctx.lock().unwrap();
        let groups: Vec<(u32, String)> = ctx.groups.iter()
            .map(|g| (g.id, g.name.clone()))
            .collect();
        drop(ctx);

        let mut changed = false;
        for (gid, name) in &groups {
            let mut selected = self.selected_group_ids.contains(gid);
            if ui.checkbox(&mut selected, name).changed() {
                if selected {
                    self.selected_group_ids.push(*gid);
                } else {
                    self.selected_group_ids.retain(|id| id != gid);
                }
                changed = true;
            }
        }

        if changed {
            self.push_config_to_engine();
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
