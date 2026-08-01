use std::any::Any;

use egui::{self, Sense, Ui, Vec2};

use crate::color::Rgb;
use crate::engine::nodes::output::group_preview::GroupPreviewDisplay;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::output::group::SharedGroupContext;
use crate::widgets::nodes::types::UiPortDef;

pub struct GroupPreviewWidget {
    id: NodeId,
    shared: SharedState,
    group_ctx: SharedGroupContext,
    pub selected_group_ids: Vec<u32>,
}

impl GroupPreviewWidget {
    pub fn new(id: NodeId, shared: SharedState, group_ctx: SharedGroupContext) -> Self {
        Self { id, shared, group_ctx, selected_group_ids: Vec::new() }
    }

    /// Build the engine-side config payload (group_ids + names + object_ids
    /// + strip_layouts) — same shape `GroupWidget` pushes so both nodes
    /// share the `load_data` format.
    pub fn push_config_to_engine(&self) {
        let ctx = self.group_ctx.lock().unwrap();

        let mut object_ids = Vec::new();
        let mut group_names = Vec::new();
        let mut strip_layouts: Vec<serde_json::Value> = Vec::new();
        // Max of the selected groups' preview_columns — with one group
        // selected this is just that group's value.
        let mut preview_columns: usize = 1;
        for gid in &self.selected_group_ids {
            if let Some(group) = ctx.groups.iter().find(|g| g.id == *gid) {
                group_names.push(group.name.clone());
                preview_columns = preview_columns.max(group.preview_columns.max(1));
                for oid in &group.object_ids {
                    if !object_ids.contains(oid) {
                        object_ids.push(*oid);
                    }
                }
                for sl in &group.strip_layout {
                    strip_layouts.push(serde_json::json!({
                        "object_id": sl.object_id,
                        "logical_start": sl.logical_start,
                        "logical_end": sl.logical_end,
                    }));
                }
            }
        }

        let config = serde_json::json!({
            "group_ids": self.selected_group_ids,
            "group_names": group_names,
            "object_ids": object_ids,
            "strip_layouts": strip_layouts,
            "preview_columns": preview_columns,
        });

        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(config);
    }
}

impl NodeWidget for GroupPreviewWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Group Output Preview" }
    fn description(&self) -> &'static str {
        "Compact visual preview of what a Group Output would render. Takes \
         the same select / width / gradient inputs and draws one dot per \
         LED, arranged as one row per strip in the group's strip-layout \
         order. Useful for developing effects without touching DMX."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("select", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("width", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("gradient", PortType::Gradient)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 140.0 }
    fn min_content_height(&self) -> f32 { 40.0 }
    fn resizable(&self) -> bool { true }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let (strips, cols) = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<GroupPreviewDisplay>())
            .map(|d| (d.strips.clone(), d.preview_columns.max(1)))
            .unwrap_or_else(|| (Vec::new(), 1));
        drop(shared);

        let w = ui.available_width();
        let h = ui.available_height().max(30.0);
        let (response, painter) = ui.allocate_painter(Vec2::new(w, h), Sense::hover());
        let rect = response.rect;

        painter.rect_filled(rect, 2.0, theme::BG_DEEP);

        if strips.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No groups",
                egui::FontId::proportional(11.0),
                theme::TEXT_DIM,
            );
            return;
        }

        // Reshape the flat strip list into a `cols`-wide 2D arrangement
        // row-major: the first `cols` strips fill row 0, the next `cols`
        // fill row 1, etc. A 6-strip shelf with `cols=2` becomes 3 rows
        // of 2 strips each.
        let strip_rows: Vec<&[Vec<Rgb>]> = strips.chunks(cols).collect();
        let preview_rows = strip_rows.len();
        // Dots per preview row = cols * LEDs-per-strip. Use the widest
        // strip in each preview row so mixed-count rows still lay out.
        let widest_strip = strips.iter().map(|s| s.len()).max().unwrap_or(0);
        if widest_strip == 0 { return; }
        let dots_per_row = cols * widest_strip;

        // Cell size constrained by both axes so dots stay roughly square
        // and never get crushed when one axis is much larger.
        let pad = 2.0;
        let inner = egui::Rect::from_min_max(
            egui::pos2(rect.min.x + pad, rect.min.y + pad),
            egui::pos2(rect.max.x - pad, rect.max.y - pad),
        );
        let cell_w = inner.width() / dots_per_row as f32;
        let cell_h = inner.height() / preview_rows as f32;
        let radius = (cell_w.min(cell_h) * 0.4).max(0.5);

        // Centre the grid.
        let grid_w = cell_w * dots_per_row as f32;
        let grid_h = cell_h * preview_rows as f32;
        let x0 = inner.min.x + (inner.width() - grid_w) * 0.5;
        let y0 = inner.min.y + (inner.height() - grid_h) * 0.5;

        for (row_idx, strip_row) in strip_rows.iter().enumerate() {
            let y = y0 + (row_idx as f32 + 0.5) * cell_h;
            for (col_idx, strip) in strip_row.iter().enumerate() {
                // Each strip takes one "column slot" of `widest_strip`
                // dots; LEDs inside are left-aligned within that slot.
                let slot_x0 = x0 + col_idx as f32 * widest_strip as f32 * cell_w;
                for (led_idx, color) in strip.iter().enumerate() {
                    let x = slot_x0 + (led_idx as f32 + 0.5) * cell_w;
                    painter.circle_filled(egui::pos2(x, y), radius, color.to_egui());
                }
            }
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        ui.label(egui::RichText::new("Group").strong());

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
