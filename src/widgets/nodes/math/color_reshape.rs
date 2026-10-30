//! The widget of the four nodes that take palettes and gradients apart and
//! put them together (see the engine side).

use std::any::Any;

use egui::{self, Ui};

use crate::engine::nodes::math::color_reshape::{MAX_STOPS, MIN_STOPS, ReshapeKind, DEFAULT_STOPS, clamp_stops, ports};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct ColorReshapeWidget {
    id: NodeId,
    shared: SharedState,
    kind: ReshapeKind,
    /// How many stops the gradient nodes show. Kept here, not read back from
    /// the engine, because ports have to change in the same frame the
    /// slider does.
    stops: usize,
}

impl ColorReshapeWidget {
    pub fn new(id: NodeId, shared: SharedState, kind: ReshapeKind) -> Self {
        Self { id, shared, kind, stops: DEFAULT_STOPS }
    }

    /// Restore the saved stop count; needed before saved wires are attached.
    pub fn set_state_from_load(&mut self, stops: usize) {
        self.stops = clamp_stops(stops);
    }
}

impl NodeWidget for ColorReshapeWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.kind.type_name() }
    fn description(&self) -> &'static str { self.kind.description() }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        ports(self.kind, self.stops).0.iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        ports(self.kind, self.stops).1.iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 110.0 }
    fn min_content_height(&self) -> f32 { 4.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, _ui: &mut Ui, _zoom: f32) {}

    fn show_inspector(&mut self, ui: &mut Ui) {
        if !self.kind.has_stops() {
            return;
        }
        let mut stops = self.stops;
        ui.horizontal(|ui| {
            ui.label("Stops:");
            if ui.add(egui::Slider::new(&mut stops, MIN_STOPS..=MAX_STOPS)).changed() {
                self.stops = stops;
                self.shared.lock().unwrap().pending_params.push((0, ParamValue::Int(stops as i64)));
            }
        });
        ui.colored_label(theme::TEXT_DIM, "Changing the count adds or removes ports; wires on removed ports are dropped.");
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
