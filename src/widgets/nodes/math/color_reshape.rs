//! The widget of the four nodes that take palettes and gradients apart and
//! put them together (see the engine side).


use egui::{self, Ui};

use crate::engine::nodes::math::color_reshape::{MAX_STOPS, MIN_STOPS, ReshapeKind, DEFAULT_STOPS, clamp_stops, ports};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeWidget, param_usize, node_identity};
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
}

impl NodeWidget for ColorReshapeWidget {
    fn restore_from_save(&mut self, _data: Option<&serde_json::Value>, params: &[(usize, ParamValue)]) {
        if let Some(stops) = param_usize(params, 0) {
            self.stops = clamp_stops(stops);
        }
    }

    node_identity!(by |w| w.kind.type_name());
    fn description(&self) -> &'static str { self.kind.description() }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        ports(self.kind, self.stops).0.iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        ports(self.kind, self.stops).1.iter().map(UiPortDef::from_def).collect()
    }

    fn show_port_labels(&self) -> bool { true }
    fn layout(&self) -> crate::widgets::nodes::node::NodeLayout {
        let (inputs, outputs) = (self.ui_inputs(), self.ui_outputs());
        crate::widgets::nodes::node::NodeLayout::STANDARD.with_gutters(
            crate::widgets::nodes::node::label_gutter(inputs.iter().map(|p| p.def.name.as_str())),
            crate::widgets::nodes::node::label_gutter(outputs.iter().map(|p| p.def.name.as_str())),
        )
    }
    fn min_width(&self) -> f32 { 110.0 }
    fn min_content_height(&self) -> f32 { 4.0 }

    fn show_content(&mut self, _ui: &mut Ui, _zoom: f32) {}

    /// The slider in `show_inspector` changes the ports in the same frame; a
    /// generic row would only move the engine's copy.
    fn hidden_param_indices(&self) -> Vec<usize> {
        if self.kind.has_stops() { vec![0] } else { Vec::new() }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        if !self.kind.has_stops() {
            return;
        }
        let mut stops = self.stops;
        ui.horizontal(|ui| {
            ui.label("Stops");
            if ui.add(egui::Slider::new(&mut stops, MIN_STOPS..=MAX_STOPS)).changed() {
                self.stops = stops;
                self.shared.lock().pending_params.push((0, ParamValue::Int(stops as i64)));
            }
        });
        ui.colored_label(theme::TEXT_DIM, "Changing the count adds or removes ports; wires on removed ports are dropped.");
    }
}
