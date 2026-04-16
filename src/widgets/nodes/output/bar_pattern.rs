use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::output::bar_pattern::BarPatternDisplay;
use crate::engine::types::*;
use crate::objects::channel::ChannelKind;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::output::group::SharedGroupContext;
use crate::widgets::nodes::types::UiPortDef;

pub struct BarPatternWidget {
    id: NodeId,
    shared: SharedState,
    group_ctx: SharedGroupContext,
    pub selected_group_ids: Vec<u32>,
}

impl BarPatternWidget {
    pub fn new(id: NodeId, shared: SharedState, group_ctx: SharedGroupContext) -> Self {
        Self {
            id,
            shared,
            group_ctx,
            selected_group_ids: Vec::new(),
        }
    }

    /// Push the resolved strip layout for the selected groups to the engine.
    pub fn push_config_to_engine(&self) {
        let ctx = self.group_ctx.lock().unwrap();

        let mut group_names = Vec::new();
        let mut strips = Vec::new();
        for gid in &self.selected_group_ids {
            if let Some(group) = ctx.groups.iter().find(|g| g.id == *gid) {
                group_names.push(group.name.clone());
                for layout in &group.strip_layout {
                    // Only include if the object actually has an LED strip channel.
                    if let Some(obj) = ctx.objects.iter().find(|o| o.id == layout.object_id) {
                        if obj.channels.iter().any(|c| matches!(c.kind, ChannelKind::LedStrip { .. })) {
                            strips.push(serde_json::json!({
                                "object_id": layout.object_id,
                                "logical_start": layout.logical_start,
                                "logical_end": layout.logical_end,
                            }));
                        }
                    }
                }
            }
        }

        let config = serde_json::json!({
            "group_ids": self.selected_group_ids,
            "group_names": group_names,
            "strips": strips,
        });

        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(config);
    }
}

impl NodeWidget for BarPatternWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Bar" }
    fn title(&self) -> &str { "Bar" }
    fn description(&self) -> &'static str {
        "Renders a moving bar onto LED strips in the selected groups using the group's strip layout."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("position", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("width", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("color", PortType::Color)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 130.0 }
    fn min_content_height(&self) -> f32 { 20.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref().and_then(|d| d.downcast_ref::<BarPatternDisplay>());
        let strip_count = display.map(|d| d.strip_count).unwrap_or(0);
        drop(shared);

        if self.selected_group_ids.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No groups");
        } else {
            ui.colored_label(Color32::from_gray(140),
                format!("{} groups, {} strips", self.selected_group_ids.len(), strip_count));
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
