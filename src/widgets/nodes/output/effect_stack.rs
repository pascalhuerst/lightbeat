//! The Effect Stack node: the list of effects linked to one name, their order
//! and blending, and a live picture of the result.
//!
//! The effects themselves are separate nodes that pick this stack in their
//! inspector. The graph tells the stack every frame which effects point at it
//! (`set_stack_members`); the stack keeps their order and per-layer settings.

use std::any::Any;

use egui::{self, Color32, Sense, Ui, Vec2};

use crate::color::BlendMode;
use crate::engine::effects::TargetSpec;
use crate::engine::effects::stack::{LayerConfig, PreviewStrip, StackConfig, StackDisplay};
use crate::engine::types::*;
use crate::objects::channel::ChannelKind;
use crate::theme;
use crate::widgets::nodes::node::{NodeWidget, StackMember};
use crate::widgets::group_context::SharedGroupContext;
use crate::widgets::nodes::types::UiPortDef;

const BLEND_MODES: &[(&str, BlendMode)] = &[
    ("Override", BlendMode::Override),
    ("Add", BlendMode::Add),
    ("Max", BlendMode::Max),
    ("Min", BlendMode::Min),
    ("Multiply", BlendMode::Multiply),
];

fn blend_label(b: BlendMode) -> &'static str {
    BLEND_MODES.iter().find(|(_, bb)| *bb == b).map(|(s, _)| *s).unwrap_or("?")
}

fn blend_combo(ui: &mut Ui, id: impl std::hash::Hash, blend: &mut BlendMode) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).selected_text(blend_label(*blend)).show_ui(ui, |ui| {
        for (label, bm) in BLEND_MODES {
            if ui.selectable_label(*blend == *bm, *label).clicked() {
                *blend = *bm;
                changed = true;
            }
        }
    });
    changed
}

pub struct EffectStackWidget {
    id: NodeId,
    shared: SharedState,
    group_ctx: SharedGroupContext,
    config: StackConfig,
    members: Vec<StackMember>,
    /// The layer whose stack-level settings the inspector shows.
    selected: Option<u64>,
    /// Set when the user renames the stack, for the graph to retarget effects.
    renamed_from: Option<String>,
}

impl EffectStackWidget {
    pub fn new(id: NodeId, shared: SharedState, group_ctx: SharedGroupContext) -> Self {
        let name = if id.0 == 0 { "Stack".to_string() } else { format!("Stack {}", id.0) };
        Self {
            id,
            shared,
            group_ctx,
            config: StackConfig { name, ..Default::default() },
            members: Vec::new(),
            selected: None,
            renamed_from: None,
        }
    }

    #[cfg(test)]
    pub fn config(&self) -> &StackConfig {
        &self.config
    }

    /// Restore a saved configuration and send it to the engine.
    pub fn set_config(&mut self, config: StackConfig) {
        self.config = config;
        self.refresh_targets();
        self.push_config();
    }

    fn push_config(&self) {
        if let Ok(value) = serde_json::to_value(&self.config) {
            self.shared.lock().unwrap().pending_config = Some(value);
        }
    }

    /// Work out what the selected groups are made of: LED strips with their
    /// layout, and single fixtures spread evenly along the axis by their
    /// order in the group. True when that differs from what the engine has.
    fn refresh_targets(&mut self) -> bool {
        let ctx = self.group_ctx.lock().unwrap();
        let mut names = Vec::new();
        let mut targets = Vec::new();
        for gid in &self.config.group_ids {
            let Some(group) = ctx.groups.iter().find(|g| g.id == *gid) else { continue };
            names.push(group.name.clone());

            for layout in &group.strip_layout {
                let is_strip = ctx
                    .objects
                    .iter()
                    .find(|o| o.id == layout.object_id)
                    .is_some_and(|o| o.channels.iter().any(|c| matches!(c.kind, ChannelKind::LedStrip { .. })));
                if is_strip {
                    targets.push(TargetSpec::Strip {
                        object_id: layout.object_id,
                        logical_start: layout.logical_start,
                        logical_end: layout.logical_end,
                    });
                }
            }

            let fixtures: Vec<u32> = group
                .object_ids
                .iter()
                .filter_map(|oid| ctx.objects.iter().find(|o| o.id == *oid))
                .filter(|o| o.channels.iter().any(|c| matches!(c.kind, ChannelKind::Color { .. })))
                .filter(|o| !o.channels.iter().any(|c| matches!(c.kind, ChannelKind::LedStrip { .. })))
                .map(|o| o.id)
                .collect();
            let n = fixtures.len();
            for (i, object_id) in fixtures.into_iter().enumerate() {
                let position = if n <= 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
                targets.push(TargetSpec::Fixture { object_id, position });
            }
        }
        drop(ctx);
        let changed = targets != self.config.targets || names != self.config.group_names;
        self.config.targets = targets;
        self.config.group_names = names;
        changed
    }

    fn label_of(&self, node_id: u64) -> String {
        self.members
            .iter()
            .find(|m| m.node_id.0 == node_id)
            .map(|m| m.label.clone())
            .unwrap_or_else(|| "(missing)".into())
    }

    fn kind_of(&self, node_id: u64) -> &'static str {
        self.members.iter().find(|m| m.node_id.0 == node_id).map(|m| m.kind).unwrap_or("")
    }

    /// The stack's picture, laid out like the group: one line per row, each
    /// strip placed on the group's axis so a strip that starts at the centre
    /// grows outwards from there.
    fn draw_preview(ui: &mut Ui, preview: &[PreviewStrip], rows: usize) {
        let row_h = 6.0;
        let gap = 2.0;
        let size = Vec2::new(ui.available_width().max(60.0), rows as f32 * (row_h + gap) + gap);
        let (resp, painter) = ui.allocate_painter(size, Sense::hover());
        painter.rect_filled(resp.rect, 2.0, theme::BG_DEEP);
        let inner = resp.rect.shrink2(Vec2::new(gap, 0.0));
        for strip in preview {
            let y = resp.rect.min.y + gap + strip.row as f32 * (row_h + gap);
            let n = strip.colors.len() as f32;
            let at = |f: f32| inner.min.x + (strip.start + (strip.end - strip.start) * f) * inner.width();
            for (i, c) in strip.colors.iter().enumerate() {
                let (x0, x1) = (at(i as f32 / n), at((i as f32 + 1.0) / n));
                let (lo, hi) = (x0.min(x1), x0.max(x1));
                // A fixture is a single cell with no span; keep it visible.
                let hi = hi.max(lo + 4.0);
                painter.rect_filled(
                    egui::Rect::from_min_max(egui::pos2(lo, y), egui::pos2(hi + 0.5, y + row_h)),
                    0.0,
                    Color32::from_rgb(c[0], c[1], c[2]),
                );
            }
        }
    }

    fn display(&self) -> Option<(usize, Vec<PreviewStrip>, usize, Vec<(u64, f32)>)> {
        let shared = self.shared.lock().unwrap();
        shared
            .display
            .as_ref()
            .and_then(|d| d.downcast_ref::<StackDisplay>())
            .map(|d| (d.pixels, d.preview.clone(), d.rows, d.levels.clone()))
    }
}

impl NodeWidget for EffectStackWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Effect Stack" }
    fn title(&self) -> &str { &self.config.name }
    fn description(&self) -> &'static str {
        "Blends the effects linked to it, bottom to top, onto one or more groups. Several stacks on a group are blended in priority order, so a stack is also a scene."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("amount", PortType::Untyped))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 200.0 }
    fn min_content_height(&self) -> f32 { 28.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn accent_color(&self) -> Option<Color32> {
        Some(theme::link_color(&self.config.name))
    }
    fn link_key(&self) -> Option<String> {
        Some(format!("stack:{}", self.config.name))
    }
    fn stack_name(&self) -> Option<String> {
        Some(self.config.name.clone())
    }

    fn take_stack_rename(&mut self) -> Option<(String, String)> {
        self.renamed_from.take().map(|from| (from, self.config.name.clone()))
    }

    fn set_stack_members(&mut self, members: Vec<StackMember>) {
        let mut changed = self.refresh_targets();
        // Keep the order of effects already listed, drop the ones that left,
        // and put new ones on top.
        let before = self.config.layers.clone();
        self.config.layers.retain(|l| members.iter().any(|m| m.node_id.0 == l.node_id));
        for m in members.iter().rev() {
            if !self.config.layers.iter().any(|l| l.node_id == m.node_id.0) {
                self.config.layers.insert(0, LayerConfig::new(m.node_id.0));
            }
        }
        changed |= before != self.config.layers;
        self.members = members;
        if changed {
            self.push_config();
        }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let (pixels, preview, rows, levels) = self.display().unwrap_or((0, Vec::new(), 1, Vec::new()));
        let mut changed = false;

        ui.horizontal(|ui| {
            let icon = if self.config.folded { egui_phosphor::regular::CARET_RIGHT } else { egui_phosphor::regular::CARET_DOWN };
            if ui.small_button(icon).on_hover_text("Fold or unfold the layer list").clicked() {
                self.config.folded = !self.config.folded;
                changed = true;
            }
            if self.config.group_ids.is_empty() {
                ui.colored_label(theme::SEM_WARNING, "no group");
            } else {
                ui.colored_label(theme::TEXT_DIM, format!("{}  ·  {} px", self.config.group_names.join(", "), pixels));
            }
        });

        if !preview.is_empty() {
            Self::draw_preview(ui, &preview, rows);
        }

        if self.config.folded {
            ui.colored_label(theme::TEXT_DIM, format!("{} layers", self.config.layers.len()));
        } else {
            let layers = self.config.layers.clone();
            let mut moved: Option<(usize, usize)> = None;
            let mut mute_toggle: Option<usize> = None;
            let mut solo_toggle: Option<usize> = None;
            let mut select: Option<u64> = None;

            if layers.is_empty() {
                ui.colored_label(theme::TEXT_DIM, "No effects linked. Pick this stack in an effect's inspector.");
            }
            for (i, layer) in layers.iter().enumerate() {
                let row_id = egui::Id::new(("stack_row", self.id.0, layer.node_id));
                let selected = self.selected == Some(layer.node_id);
                let level = levels.iter().find(|(n, _)| *n == layer.node_id).map(|(_, l)| *l).unwrap_or(0.0);

                let (_, dropped) = ui.dnd_drop_zone::<usize, _>(egui::Frame::new(), |ui| {
                    egui::Frame::new()
                        .fill(if selected { theme::BG_HIGH } else { Color32::TRANSPARENT })
                        .inner_margin(egui::Margin::symmetric(2, 1))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.dnd_drag_source(row_id, i, |ui| {
                                    ui.label(egui::RichText::new(egui_phosphor::regular::DOTS_SIX_VERTICAL).color(theme::TEXT_DIM));
                                })
                                .response
                                .on_hover_text("Drag to reorder");

                                let mute_text = if layer.muted { egui_phosphor::regular::EYE_SLASH } else { egui_phosphor::regular::EYE };
                                if ui.small_button(mute_text).on_hover_text("Mute").clicked() {
                                    mute_toggle = Some(i);
                                }
                                let solo_btn = egui::Button::new("S").fill(if layer.solo { theme::SEM_WARNING } else { Color32::TRANSPARENT });
                                if ui.add(solo_btn).on_hover_text("Solo").clicked() {
                                    solo_toggle = Some(i);
                                }

                                let color = if layer.muted { theme::TEXT_DIM } else { theme::TEXT };
                                let label = egui::Label::new(egui::RichText::new(self.label_of(layer.node_id)).color(color))
                                    .sense(Sense::click())
                                    .truncate();
                                if ui.add(label).clicked() {
                                    select = Some(layer.node_id);
                                }

                                // Activity: lights up when the effect is drawing.
                                let (r, p) = ui.allocate_painter(Vec2::new(18.0, 8.0), Sense::hover());
                                p.rect_filled(r.rect, 1.0, theme::BG_DEEP);
                                let mut bar = r.rect;
                                bar.set_width(r.rect.width() * level.clamp(0.0, 1.0));
                                p.rect_filled(bar, 1.0, theme::SEM_SUCCESS);
                            });
                        });
                });
                if let Some(from) = dropped {
                    moved = Some((*from, i));
                }
            }

            if let Some((from, to)) = moved
                && from != to
                && from < self.config.layers.len()
                && to < self.config.layers.len()
            {
                let item = self.config.layers.remove(from);
                self.config.layers.insert(to, item);
                changed = true;
            }
            if let Some(i) = mute_toggle {
                self.config.layers[i].muted = !self.config.layers[i].muted;
                changed = true;
            }
            if let Some(i) = solo_toggle {
                self.config.layers[i].solo = !self.config.layers[i].solo;
                changed = true;
            }
            if let Some(n) = select {
                self.selected = Some(n);
            }
        }

        if changed {
            self.push_config();
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Name:");
            let mut name = self.config.name.clone();
            if ui.text_edit_singleline(&mut name).changed() && !name.is_empty() {
                if self.renamed_from.is_none() {
                    self.renamed_from = Some(self.config.name.clone());
                }
                self.config.name = name;
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Priority:");
            if ui
                .add(egui::DragValue::new(&mut self.config.priority).range(-100..=100))
                .on_hover_text("Stacks on a group are blended in this order, lowest first.")
                .changed()
            {
                changed = true;
            }
            ui.label("Blend:");
            if blend_combo(ui, "stack_blend", &mut self.config.blend) {
                changed = true;
            }
        });

        ui.add_space(4.0);
        ui.label(egui::RichText::new("Groups").strong());
        let groups: Vec<(u32, String)> =
            self.group_ctx.lock().unwrap().groups.iter().map(|g| (g.id, g.name.clone())).collect();
        for (gid, name) in &groups {
            let mut on = self.config.group_ids.contains(gid);
            if ui.checkbox(&mut on, name).changed() {
                if on {
                    self.config.group_ids.push(*gid);
                } else {
                    self.config.group_ids.retain(|id| id != gid);
                }
                self.refresh_targets();
                changed = true;
            }
        }

        ui.add_space(6.0);
        ui.separator();
        ui.label(egui::RichText::new("Selected layer").strong());
        match self.selected.and_then(|n| self.config.layers.iter().position(|l| l.node_id == n)) {
            None => {
                ui.colored_label(theme::TEXT_DIM, "Click a layer in the stack.");
            }
            Some(i) => {
                let label = self.label_of(self.config.layers[i].node_id);
                let kind = self.kind_of(self.config.layers[i].node_id);
                ui.colored_label(theme::TEXT, format!("{label}  ({kind})"));
                let layer = &mut self.config.layers[i];
                ui.horizontal(|ui| {
                    ui.label("Blend:");
                    if blend_combo(ui, ("layer_blend", i), &mut layer.blend) {
                        changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Opacity:");
                    if ui.add(egui::Slider::new(&mut layer.opacity, 0.0..=1.0).fixed_decimals(2)).changed() {
                        changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    changed |= ui.checkbox(&mut layer.muted, "Mute").changed();
                    changed |= ui.checkbox(&mut layer.solo, "Solo").changed();
                });
            }
        }

        if changed {
            self.push_config();
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::group_context::new_shared_group_context;

    fn member(id: u64, label: &str) -> StackMember {
        StackMember { node_id: NodeId(id), label: label.into(), kind: "Burst" }
    }

    fn stack() -> EffectStackWidget {
        EffectStackWidget::new(NodeId(7), new_shared_state(1, 0), new_shared_group_context())
    }

    fn order(w: &EffectStackWidget) -> Vec<u64> {
        w.config().layers.iter().map(|l| l.node_id).collect()
    }

    #[test]
    fn new_effects_join_on_top_and_keep_the_order_of_the_old_ones() {
        let mut s = stack();
        s.set_stack_members(vec![member(1, "a"), member(2, "b")]);
        assert_eq!(order(&s), vec![1, 2]);

        // The user drags b above a.
        s.config.layers.swap(0, 1);
        s.set_stack_members(vec![member(1, "a"), member(2, "b"), member(3, "c")]);
        assert_eq!(order(&s), vec![3, 2, 1], "c arrives on top, b stays above a");

        s.set_stack_members(vec![member(3, "c"), member(1, "a")]);
        assert_eq!(order(&s), vec![3, 1], "b left the stack");
    }

    #[test]
    fn layer_settings_survive_membership_changes() {
        let mut s = stack();
        s.set_stack_members(vec![member(1, "a"), member(2, "b")]);
        s.config.layers[1].opacity = 0.25;
        s.config.layers[1].muted = true;
        s.set_stack_members(vec![member(1, "a"), member(2, "b"), member(3, "c")]);
        let b = s.config.layers.iter().find(|l| l.node_id == 2).unwrap();
        assert_eq!((b.opacity, b.muted), (0.25, true));
    }

    #[test]
    fn changes_reach_the_engine_but_a_quiet_frame_sends_nothing() {
        let mut s = stack();
        s.set_stack_members(vec![member(1, "a")]);
        assert!(s.shared.lock().unwrap().pending_config.take().is_some());
        s.set_stack_members(vec![member(1, "a")]);
        assert!(s.shared.lock().unwrap().pending_config.is_none());
    }

    #[test]
    fn a_rename_is_reported_once_for_the_graph_to_follow() {
        let mut s = stack();
        s.renamed_from = Some("Stack 7".into());
        s.config.name = "Shelf".into();
        assert_eq!(s.take_stack_rename(), Some(("Stack 7".to_string(), "Shelf".to_string())));
        assert_eq!(s.take_stack_rename(), None);
    }
}
