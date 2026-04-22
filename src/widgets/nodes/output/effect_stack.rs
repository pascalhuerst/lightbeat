use std::any::Any;

use egui::{self, Color32, Ui};

use crate::color::BlendMode;
use crate::engine::nodes::output::effect_stack::{EffectLayerConfig, EffectStackDisplay};
use crate::engine::patterns::{all_pattern_types, create_pattern, pattern_channel_count};
use crate::engine::types::*;
use crate::objects::channel::ChannelKind;
use crate::widgets::fader::highlight_alpha;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::output::group::SharedGroupContext;
use crate::widgets::nodes::types::UiPortDef;

const LAYER_HIGHLIGHT_DURATION: f64 = 0.6;

pub struct EffectStackWidget {
    id: NodeId,
    shared: SharedState,
    group_ctx: SharedGroupContext,
    pub selected_group_ids: Vec<u32>,
    pub layers: Vec<EffectLayerConfig>,
    /// Most-recent inspector hover timestamp per layer; drives port highlight.
    layer_hover_time: Vec<Option<f64>>,
}

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

impl EffectStackWidget {
    pub fn new(id: NodeId, shared: SharedState, group_ctx: SharedGroupContext) -> Self {
        Self {
            id,
            shared,
            group_ctx,
            selected_group_ids: Vec::new(),
            layers: Vec::new(),
            layer_hover_time: Vec::new(),
        }
    }

    /// For input port `port_idx`, return the index of the layer that owns it.
    /// Walks the same per-layer port layout as `ui_inputs`.
    fn layer_for_input(&self, port_idx: usize) -> Option<usize> {
        let mut acc = 0usize;
        for (li, layer) in self.layers.iter().enumerate() {
            let n = create_pattern(&layer.pattern_type)
                .map(|p| p.input_ports().len())
                .unwrap_or(0);
            if port_idx < acc + n {
                return Some(li);
            }
            acc += n;
        }
        None
    }

    /// Resolve render targets for selected groups + push layers to engine.
    /// LED strip members come with explicit `StripLayout`. Non-strip Color
    /// fixtures get an implicit position derived from their order in the group.
    pub fn push_config_to_engine(&self) {
        let ctx = self.group_ctx.lock().unwrap();

        let mut group_names = Vec::new();
        let mut targets = Vec::new();
        for gid in &self.selected_group_ids {
            let group = match ctx.groups.iter().find(|g| g.id == *gid) {
                Some(g) => g,
                None => continue,
            };
            group_names.push(group.name.clone());

            // Strip members: use the explicit strip_layout entries.
            for layout in &group.strip_layout {
                if let Some(obj) = ctx.objects.iter().find(|o| o.id == layout.object_id)
                    && obj.channels.iter().any(|c| matches!(c.kind, ChannelKind::LedStrip { .. })) {
                        targets.push(serde_json::json!({
                            "kind": "strip",
                            "object_id": layout.object_id,
                            "logical_start": layout.logical_start,
                            "logical_end": layout.logical_end,
                        }));
                    }
            }

            // Non-strip fixtures with a Color channel: distribute evenly along
            // the 0..1 axis based on their order in the group.
            let fixture_objs: Vec<&_> = group.object_ids.iter()
                .filter_map(|oid| ctx.objects.iter().find(|o| o.id == *oid))
                .filter(|o| o.channels.iter().any(|c| matches!(c.kind, ChannelKind::Color { .. })))
                .filter(|o| !o.channels.iter().any(|c| matches!(c.kind, ChannelKind::LedStrip { .. })))
                .collect();
            let n = fixture_objs.len();
            for (i, obj) in fixture_objs.iter().enumerate() {
                let pos = if n <= 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
                targets.push(serde_json::json!({
                    "kind": "fixture",
                    "object_id": obj.id,
                    "position": pos,
                }));
            }
        }
        drop(ctx);

        let config = serde_json::json!({
            "group_ids": self.selected_group_ids,
            "group_names": group_names,
            "strips": targets,
            "layers": self.layers,
        });
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(config);
    }
}

impl NodeWidget for EffectStackWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Effect Stack" }
    fn title(&self) -> &str { "Effect Stack" }
    fn description(&self) -> &'static str {
        "Composes multiple LED-strip effects with blend modes onto the selected group(s)."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        // Mirror the engine's port layout: per-layer prefixed ports.
        let mut ports = Vec::new();
        for (i, layer) in self.layers.iter().enumerate() {
            if let Some(p) = create_pattern(&layer.pattern_type) {
                for port in p.input_ports() {
                    ports.push(UiPortDef::from_def(&PortDef::new(
                        format!("L{}.{}", i + 1, port.name),
                        port.port_type,
                    )));
                }
            }
        }
        ports
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 150.0 }
    fn min_content_height(&self) -> f32 { 24.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn input_highlight(&self, port_idx: usize, now: f64) -> f32 {
        let layer = match self.layer_for_input(port_idx) {
            Some(l) => l,
            None => return 0.0,
        };
        let last = self.layer_hover_time.get(layer).copied().flatten();
        highlight_alpha(last, now, LAYER_HIGHLIGHT_DURATION)
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref().and_then(|d| d.downcast_ref::<EffectStackDisplay>());
        let strip_count = display.map(|d| d.strip_count).unwrap_or(0);
        drop(shared);

        if self.selected_group_ids.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No groups");
        } else {
            ui.colored_label(Color32::from_gray(140),
                format!("{} layers, {} strips", self.layers.len(), strip_count));
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        // -- Groups --
        ui.label(egui::RichText::new("Groups").strong());
        let ctx = self.group_ctx.lock().unwrap();
        let groups: Vec<(u32, String)> = ctx.groups.iter()
            .map(|g| (g.id, g.name.clone()))
            .collect();
        drop(ctx);

        let mut groups_changed = false;
        for (gid, name) in &groups {
            let mut sel = self.selected_group_ids.contains(gid);
            if ui.checkbox(&mut sel, name).changed() {
                if sel { self.selected_group_ids.push(*gid); }
                else { self.selected_group_ids.retain(|id| id != gid); }
                groups_changed = true;
            }
        }

        ui.add_space(6.0);
        ui.separator();

        // -- Layers --
        ui.label(egui::RichText::new("Layers (bottom → top)").strong());

        let mut layers_changed = false;
        let mut remove_idx: Option<usize> = None;
        let mut move_up: Option<usize> = None;
        let mut move_down: Option<usize> = None;
        let now = ui.ctx().input(|i| i.time);

        // Resize hover-time vec to match layer count.
        if self.layer_hover_time.len() != self.layers.len() {
            self.layer_hover_time.resize(self.layers.len(), None);
        }

        for (i, layer) in self.layers.iter_mut().enumerate() {
            let row_resp = ui.scope(|ui| {
                ui.push_id(("layer", i), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}.", i + 1));
                        egui::ComboBox::from_id_salt(("pat_type", i))
                            .width(80.0)
                            .selected_text(&layer.pattern_type)
                            .show_ui(ui, |ui| {
                                for &pname in all_pattern_types() {
                                    if ui.selectable_label(layer.pattern_type == pname, pname).clicked() {
                                        layer.pattern_type = pname.to_string();
                                        layers_changed = true;
                                    }
                                }
                            });
                        egui::ComboBox::from_id_salt(("blend", i))
                            .width(80.0)
                            .selected_text(blend_label(layer.blend))
                            .show_ui(ui, |ui| {
                                for (label, bm) in BLEND_MODES {
                                    if ui.selectable_label(layer.blend == *bm, *label).clicked() {
                                        layer.blend = *bm;
                                        layers_changed = true;
                                    }
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Opacity:");
                        if ui.add(
                            egui::Slider::new(&mut layer.opacity, 0.0..=1.0).fixed_decimals(2)
                        ).changed() {
                            layers_changed = true;
                        }
                        if ui.small_button(egui_phosphor::regular::ARROW_UP).clicked() { move_up = Some(i); }
                        if ui.small_button(egui_phosphor::regular::ARROW_DOWN).clicked() { move_down = Some(i); }
                        if ui.small_button(egui_phosphor::regular::X).clicked() { remove_idx = Some(i); }
                    });
                    ui.separator();
                });
            });
            // Stamp the hover time whenever the pointer is over this layer's
            // row — the corresponding input ports on the node will glow.
            if row_resp.response.contains_pointer()
                && let Some(slot) = self.layer_hover_time.get_mut(i) {
                    *slot = Some(now);
                }
        }

        if let Some(i) = remove_idx {
            self.layers.remove(i);
            layers_changed = true;
        }
        if let Some(i) = move_up
            && i > 0 { self.layers.swap(i, i - 1); layers_changed = true; }
        if let Some(i) = move_down
            && i + 1 < self.layers.len() { self.layers.swap(i, i + 1); layers_changed = true; }

        ui.horizontal(|ui| {
            if ui.button("+ Add Layer").clicked() {
                // Default new layer = Bar with Override at full opacity.
                self.layers.push(EffectLayerConfig {
                    pattern_type: "Bar".into(),
                    blend: BlendMode::Override,
                    opacity: 1.0,
                });
                layers_changed = true;
            }
        });

        if groups_changed || layers_changed {
            self.push_config_to_engine();
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// Suppress unused import warnings until pattern_channel_count is wired in.
#[allow(dead_code)]
fn _force_use() {
    let _ = pattern_channel_count("Bar");
}
