//! The widgets of Bus Merge, Bus Split and Bus Pick. Like Port Hub they take
//! their type from the first wire; Split and Pick take their width from the
//! wire on the bus input (`follow_bus`), Merge has as many inputs as the
//! "Inputs" setting says.

use egui::Ui;

use crate::engine::nodes::math::bus::{
    BUS_DEFAULT_LANES, BusDisplay, PICK_PARAM_TYPE, clamp_lanes,
    clamp_merge_inputs, merge_ports, pick_ports, split_ports,
};
use crate::engine::nodes::math::multiplex::{MUX_TYPE_NAMES, type_from_index, type_to_index};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeLayout, NodeWidget, labelled_layout, param_usize, with_display};
use crate::widgets::nodes::types::UiPortDef;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Merge,
    Split,
    Pick,
}

pub struct BusWidget {
    id: NodeId,
    shared: SharedState,
    kind: Kind,
    port_type: PortType,
    /// Merge: the number of inputs. Split and Pick: the width of the bus input.
    count: usize,
}

impl BusWidget {
    fn new(id: NodeId, shared: SharedState, kind: Kind) -> Self {
        Self { id, shared, kind, port_type: PortType::Any, count: BUS_DEFAULT_LANES }
    }

    pub fn merge(id: NodeId, shared: SharedState) -> Self {
        Self::new(id, shared, Kind::Merge)
    }

    pub fn split(id: NodeId, shared: SharedState) -> Self {
        Self::new(id, shared, Kind::Split)
    }

    pub fn pick(id: NodeId, shared: SharedState) -> Self {
        Self::new(id, shared, Kind::Pick)
    }

    fn ports(&self) -> (Vec<PortDef>, Vec<PortDef>) {
        match self.kind {
            Kind::Merge => merge_ports(self.count, self.port_type),
            Kind::Split => split_ports(self.count, self.port_type),
            Kind::Pick => pick_ports(self.count, self.port_type),
        }
    }

    fn sync_from_display(&mut self) {
        with_display::<BusDisplay, _>(&self.shared, |d| {
            self.port_type = d.port_type;
            self.count = d.count;
        });
    }

    /// Only the Auto setting adopts the first wire's type.
    fn adopt(&mut self, wire_type: PortType) {
        if self.port_type != PortType::Any || wire_type == PortType::Any {
            return;
        }
        self.port_type = wire_type;
        self.shared.lock().pending_params.push((PICK_PARAM_TYPE, ParamValue::Choice(type_to_index(wire_type))));
    }
}

impl NodeWidget for BusWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str {
        match self.kind {
            Kind::Merge => "Bus Merge",
            Kind::Split => "Bus Split",
            Kind::Pick => "Bus Pick",
        }
    }
    fn shared_state(&self) -> &SharedState { &self.shared }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn restore_from_save(&mut self, _data: Option<&serde_json::Value>, params: &[(usize, ParamValue)]) {
        self.port_type = param_usize(params, 0).map(type_from_index).unwrap_or(PortType::Any);
        let saved = param_usize(params, 1).unwrap_or(BUS_DEFAULT_LANES);
        self.count = match self.kind {
            Kind::Merge => clamp_merge_inputs(saved),
            _ => clamp_lanes(saved),
        };
    }

    fn description(&self) -> &'static str {
        match self.kind {
            Kind::Merge => "Several single wires in, one bus out: a wire of as many lanes, \
                one per input. A node fed by the bus works on all lanes at once.",
            Kind::Split => "A bus in, one wire out per lane. The number of outputs follows \
                the width of the wire you connect.",
            Kind::Pick => "One lane of a bus, by index. Switch the Index setting on as an \
                input to choose the lane with a wire.",
        }
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&self.ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&self.ports().1) }

    fn show_port_labels(&self) -> bool { self.kind != Kind::Pick }
    fn layout(&self) -> NodeLayout { labelled_layout(&self.ui_inputs(), &self.ui_outputs()) }
    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 14.0 }

    fn default_promoted(&self) -> &'static [(&'static str, PromotedKind)] {
        if self.kind == Kind::Pick { &[("Index", PromotedKind::Int)] } else { &[] }
    }

    fn on_ui_connect(&mut self, _input_port: usize, source_type: PortType) {
        self.adopt(source_type);
    }
    fn on_ui_output_connect(&mut self, _output_port: usize, dest_type: PortType) {
        self.adopt(dest_type);
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.sync_from_display();
        let what = MUX_TYPE_NAMES[type_to_index(self.port_type)];
        let text = match self.kind {
            Kind::Merge => format!("{} \u{2192} bus of {}", self.count, what),
            Kind::Split => format!("bus of {} \u{2192} {}", what, self.count),
            Kind::Pick => format!("1 of {} {}", self.count, what),
        };
        ui.colored_label(theme::TEXT_DIM, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widget(kind: Kind) -> BusWidget {
        BusWidget::new(NodeId(1), new_shared_state(64 * 40, 64 * 40), kind)
    }

    #[test]
    fn the_first_wire_sets_the_type_on_either_side() {
        let mut w = widget(Kind::Merge);
        w.on_ui_connect(0, PortType::Logic);
        assert_eq!(w.port_type, PortType::Logic);
        let mut w = widget(Kind::Split);
        w.on_ui_output_connect(1, PortType::Color);
        assert_eq!(w.port_type, PortType::Color);
        assert_eq!(w.shared.lock().pending_params.len(), 1);
    }

    #[test]
    fn pick_starts_with_the_index_as_an_input() {
        let w = widget(Kind::Pick);
        assert_eq!(w.default_promoted(), &[("Index", PromotedKind::Int)]);
    }
}
