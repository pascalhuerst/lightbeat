use std::any::Any;
use std::sync::{Arc, Mutex};

use egui::{self, Color32, Sense, Ui, Vec2};

use crate::engine::nodes::math::palette_select::PaletteSelectDisplay;
use crate::engine::types::*;
use crate::objects::color_palette::{ColorPalette, ColorPaletteGroup, PALETTE_SIZE};
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct PaletteContext {
    pub palettes: Vec<ColorPalette>,
    pub groups: Vec<ColorPaletteGroup>,
}

pub type SharedPaletteContext = Arc<Mutex<PaletteContext>>;

pub fn new_shared_palette_context() -> SharedPaletteContext {
    Arc::new(Mutex::new(PaletteContext {
        palettes: Vec::new(),
        groups: Vec::new(),
    }))
}

pub struct PaletteSelectWidget {
    id: NodeId,
    shared: SharedState,
    palette_ctx: SharedPaletteContext,
    /// Ordered list of group IDs this node uses.
    selected_group_ids: Vec<u32>,
}

impl PaletteSelectWidget {
    pub fn new(id: NodeId, shared: SharedState, palette_ctx: SharedPaletteContext) -> Self {
        Self { id, shared, palette_ctx, selected_group_ids: Vec::new() }
    }

    fn push_data_to_engine(&self) {
        let ctx = self.palette_ctx.lock().unwrap();
        let groups_data: Vec<serde_json::Value> = self.selected_group_ids.iter().filter_map(|gid| {
            let group = ctx.groups.iter().find(|g| g.id == *gid)?;
            let palettes: Vec<&ColorPalette> = group.palette_ids.iter()
                .filter_map(|pid| ctx.palettes.iter().find(|s| s.id == *pid))
                .collect();
            Some(serde_json::json!({
                "name": group.name,
                "palettes": palettes,
            }))
        }).collect();
        drop(ctx);

        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({
            "group_ids": self.selected_group_ids,
            "groups": groups_data,
        }));
    }
}

impl NodeWidget for PaletteSelectWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Palette Select" }
    fn title(&self) -> &str { "Palette Select" }
    fn description(&self) -> &'static str { "Picks a palette (a set of 4 colors) from a palette group, indexed by group and palette index." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("group", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("palette", PortType::Untyped)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("palette", PortType::Palette))]
    }

    fn min_width(&self) -> f32 { 140.0 }
    fn min_content_height(&self) -> f32 { 30.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<PaletteSelectDisplay>());

        let (colors, group_idx, palette_idx, palette_count, group_count, restored_ids) = if let Some(d) = display {
            let ids = if self.selected_group_ids.is_empty() && !d.group_ids.is_empty() {
                Some(d.group_ids.clone())
            } else { None };
            (d.current_colors, d.current_group_index, d.current_palette_index, d.palette_count, d.group_names.len(), ids)
        } else {
            ([crate::color::Rgb::BLACK; PALETTE_SIZE], 0, 0, 0, 0, None)
        };
        drop(shared);

        // Restore widget selection from engine on first frame after load.
        if let Some(ids) = restored_ids {
            self.selected_group_ids = ids;
        }

        if group_count == 0 {
            ui.colored_label(Color32::from_gray(120), "No groups");
            return;
        }

        // Current colors — single painter, no layout spacing issues.
        let w = ui.available_width();
        let swatch_h = (w / PALETTE_SIZE as f32 * 0.6).clamp(4.0, 20.0);
        let swatch_w = w / PALETTE_SIZE as f32;
        let (resp, painter) = ui.allocate_painter(Vec2::new(w, swatch_h), Sense::hover());
        for (i, c) in colors.iter().enumerate() {
            let color = Color32::from_rgb(
                (c.r.clamp(0.0, 1.0) * 255.0) as u8,
                (c.g.clamp(0.0, 1.0) * 255.0) as u8,
                (c.b.clamp(0.0, 1.0) * 255.0) as u8,
            );
            let rect = egui::Rect::from_min_size(
                egui::pos2(resp.rect.min.x + i as f32 * swatch_w, resp.rect.min.y),
                Vec2::new(swatch_w, swatch_h),
            );
            painter.rect_filled(rect, 1.0, color);
        }
        ui.colored_label(Color32::from_gray(120),
            format!("G:{}/{} P:{}/{}", group_idx + 1, group_count, palette_idx + 1, palette_count));
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let ctx = self.palette_ctx.lock().unwrap();
        let all_groups: Vec<(u32, String)> = ctx.groups.iter()
            .map(|g| (g.id, g.name.clone()))
            .collect();
        drop(ctx);

        ui.label(egui::RichText::new("Palette Groups (ordered)").strong());

        // Show selected groups with remove + move buttons.
        let mut changed = false;
        let mut remove_idx = None;
        let mut swap: Option<(usize, usize)> = None;

        for (i, gid) in self.selected_group_ids.iter().enumerate() {
            let name = all_groups.iter().find(|(id, _)| id == gid)
                .map(|(_, n)| n.as_str()).unwrap_or("???");
            ui.horizontal(|ui| {
                ui.label(format!("{}. {}", i, name));
                if i > 0 && ui.small_button("↑").clicked() {
                    swap = Some((i, i - 1));
                }
                if i + 1 < self.selected_group_ids.len() && ui.small_button("↓").clicked() {
                    swap = Some((i, i + 1));
                }
                if ui.small_button("x").clicked() {
                    remove_idx = Some(i);
                }
            });
        }

        if let Some(i) = remove_idx {
            self.selected_group_ids.remove(i);
            changed = true;
        }
        if let Some((a, b)) = swap {
            self.selected_group_ids.swap(a, b);
            changed = true;
        }

        // Add group.
        let available: Vec<(u32, String)> = all_groups.iter()
            .filter(|(id, _)| !self.selected_group_ids.contains(id))
            .cloned()
            .collect();
        if !available.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label("Add:");
                for (gid, name) in &available {
                    if ui.small_button(name).clicked() {
                        self.selected_group_ids.push(*gid);
                        changed = true;
                    }
                }
            });
        }

        if changed {
            self.push_data_to_engine();
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
