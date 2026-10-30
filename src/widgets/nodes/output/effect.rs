//! The widget every effect node shares.
//!
//! An effect's ports and parameters come from its engine side (`EffectKind`),
//! so one widget serves Fill, Wave Loop, Ripple Burst and whatever comes
//! next. What it adds is the link to a stack: picked in the inspector, shown
//! by the node's colour, and highlighted together with the stack.

use std::any::Any;
use std::sync::{Arc, Mutex};

use egui::{self, Color32, Sense, Ui, Vec2};

use crate::engine::effects::bar::Bar;
use crate::engine::effects::fill::Fill;
use crate::engine::effects::node::{EffectDisplay, EffectKind};
use crate::engine::effects::ripple::Ripple;
use crate::engine::effects::wave::Wave;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

/// The names of all stacks in the graph, kept up to date by the graph.
pub type SharedStackNames = Arc<Mutex<Vec<String>>>;

pub fn new_shared_stack_names() -> SharedStackNames {
    Arc::new(Mutex::new(Vec::new()))
}

/// What differs between effects.
#[derive(Clone, Copy)]
pub struct EffectSpec {
    pub type_name: &'static str,
    /// "Loop" follows a phase, "Burst" is spawned by a trigger.
    pub kind: &'static str,
    pub description: &'static str,
    pub inputs: fn() -> Vec<PortDef>,
}

pub const FILL: EffectSpec = EffectSpec {
    type_name: "Fill",
    kind: "Loop",
    description: "A solid colour, or a gradient across the group. A base for other layers.",
    inputs: || Fill::default().inputs(),
};

pub const BAR_LOOP: EffectSpec = EffectSpec {
    type_name: "Bar Loop",
    kind: "Loop",
    description: "A bar of light at a position on the group. Position, width and softness can be promoted to inputs, for example to follow an LFO or a fader.",
    inputs: || Bar::default().inputs(),
};

pub const WAVE_LOOP: EffectSpec = EffectSpec {
    type_name: "Wave Loop",
    kind: "Loop",
    description: "A wave that travels across the group in time with the phase it is given, for example the beat clock's bar phase.",
    inputs: || Wave::default().inputs(),
};

pub const RIPPLE_BURST: EffectSpec = EffectSpec {
    type_name: "Ripple Burst",
    kind: "Burst",
    description: "A ring of light that spreads from a point each time it is triggered.",
    inputs: || Ripple::default().inputs(),
};

pub struct EffectWidget {
    id: NodeId,
    shared: SharedState,
    spec: EffectSpec,
    names: SharedStackNames,
    stack: String,
}

impl EffectWidget {
    pub fn new(id: NodeId, shared: SharedState, spec: EffectSpec, names: SharedStackNames) -> Self {
        Self { id, shared, spec, names, stack: String::new() }
    }

    #[cfg(test)]
    pub fn stack(&self) -> &str {
        &self.stack
    }

    pub fn set_stack(&mut self, name: &str) {
        self.stack = name.to_string();
        self.shared.lock().unwrap().pending_config = Some(serde_json::json!({ "stack": self.stack }));
    }

    fn display(&self) -> Option<(bool, f32)> {
        let shared = self.shared.lock().unwrap();
        shared
            .display
            .as_ref()
            .and_then(|d| d.downcast_ref::<EffectDisplay>())
            .map(|d| (d.linked, d.level))
    }
}

impl NodeWidget for EffectWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.spec.type_name }
    fn description(&self) -> &'static str { self.spec.description }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        (self.spec.inputs)().iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 150.0 }
    fn min_content_height(&self) -> f32 { 24.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn accent_color(&self) -> Option<Color32> {
        (!self.stack.is_empty()).then(|| theme::link_color(&self.stack))
    }
    fn link_key(&self) -> Option<String> {
        (!self.stack.is_empty()).then(|| format!("stack:{}", self.stack))
    }
    fn linked_stack(&self) -> Option<String> {
        (!self.stack.is_empty()).then(|| self.stack.clone())
    }
    fn effect_kind(&self) -> &'static str { self.spec.kind }
    fn rename_linked_stack(&mut self, from: &str, to: &str) {
        if self.stack == from {
            self.set_stack(to);
        }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let (linked, level) = self.display().unwrap_or((false, 0.0));
        ui.horizontal(|ui| {
            let icon = if self.spec.kind == "Burst" { egui_phosphor::regular::LIGHTNING } else { egui_phosphor::regular::WAVE_SINE };
            ui.colored_label(theme::TEXT_DIM, icon);
            if self.stack.is_empty() {
                ui.colored_label(theme::SEM_WARNING, "no stack");
            } else if !linked {
                ui.colored_label(theme::SEM_WARNING, format!("{} (not found)", self.stack));
            } else {
                ui.colored_label(theme::TEXT, &self.stack);
            }
            let (r, p) = ui.allocate_painter(Vec2::new(18.0, 8.0), Sense::hover());
            p.rect_filled(r.rect, 1.0, theme::BG_DEEP);
            let mut bar = r.rect;
            bar.set_width(r.rect.width() * level.clamp(0.0, 1.0));
            p.rect_filled(bar, 1.0, theme::SEM_SUCCESS);
        });
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Stack:");
            let names = self.names.lock().unwrap().clone();
            let current = if self.stack.is_empty() { "(none)".to_string() } else { self.stack.clone() };
            let mut picked: Option<String> = None;
            egui::ComboBox::from_id_salt(("effect_stack_pick", self.id.0))
                .selected_text(current)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.stack.is_empty(), "(none)").clicked() {
                        picked = Some(String::new());
                    }
                    for name in &names {
                        if ui.selectable_label(&self.stack == name, name).clicked() {
                            picked = Some(name.clone());
                        }
                    }
                });
            if let Some(name) = picked {
                self.set_stack(&name);
            }
        });
        if !self.stack.is_empty() {
            ui.colored_label(theme::TEXT_DIM, "Blend, opacity, order, mute and solo are set in the stack.");
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ripple() -> EffectWidget {
        EffectWidget::new(NodeId(3), new_shared_state(48, 0), RIPPLE_BURST, new_shared_stack_names())
    }

    #[test]
    fn ports_come_from_the_engine_side_effect() {
        let w = ripple();
        let names: Vec<String> = w.ui_inputs().into_iter().map(|p| p.def.name).collect();
        assert_eq!(names, ["trigger", "color"]);
        let channels = total_channels(&w.ui_inputs().iter().map(|p| p.def.clone()).collect::<Vec<_>>());
        assert!(channels <= 48, "the shared state is big enough ({channels} channels)");
    }

    #[test]
    fn linking_sets_the_link_key_and_tells_the_engine() {
        let mut w = ripple();
        assert_eq!(w.link_key(), None);
        w.set_stack("Shelf");
        assert_eq!(w.link_key().as_deref(), Some("stack:Shelf"));
        assert_eq!(w.linked_stack().as_deref(), Some("Shelf"));
        let sent = w.shared.lock().unwrap().pending_config.take().unwrap();
        assert_eq!(sent["stack"], "Shelf");
    }

    #[test]
    fn a_renamed_stack_takes_its_effects_along() {
        let mut w = ripple();
        w.set_stack("Old");
        w.rename_linked_stack("Other", "X");
        assert_eq!(w.stack(), "Old");
        w.rename_linked_stack("Old", "New");
        assert_eq!(w.stack(), "New");
    }

    #[test]
    fn every_effect_spec_matches_its_engine_node() {
        use crate::engine::effects::node::EffectKind;
        assert_eq!(FILL.type_name, Fill::default().type_name());
        assert_eq!(WAVE_LOOP.type_name, Wave::default().type_name());
        assert_eq!(BAR_LOOP.type_name, Bar::default().type_name());
        assert_eq!(RIPPLE_BURST.type_name, Ripple::default().type_name());
    }
}
