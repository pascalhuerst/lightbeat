use egui::{self, Ui};

use crate::engine::nodes::math::multiplex::{
    MUX_DEFAULT_SLOTS, demux_ports, mux_ports, MuxDisplay, SelectMode, clamp_slots, type_from_index, type_to_index,
};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeLayout, NodeWidget, labelled_layout, param_usize, node_identity, with_display};
use crate::widgets::nodes::types::UiPortDef;

fn push_type(shared: &SharedState, port_type: PortType) {
    let mut s = shared.lock();
    s.pending_params.push((0, ParamValue::Choice(type_to_index(port_type))));
}

// ---------------------------------------------------------------------------
// Multiplexer widget
// ---------------------------------------------------------------------------

pub struct MultiplexerWidget {
    id: NodeId,
    shared: SharedState,
    port_type: PortType,
    slots: usize,
    select_mode: SelectMode,
}

impl MultiplexerWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id, shared,
            port_type: PortType::Any,
            slots: MUX_DEFAULT_SLOTS,
            select_mode: SelectMode::Index,
        }
    }

    fn sync_from_display(&mut self) {
        with_display::<MuxDisplay, _>(&self.shared, |d| {
            if d.port_type != self.port_type { self.port_type = d.port_type; }
            if d.slots != self.slots { self.slots = d.slots; }
            if d.select_mode != self.select_mode { self.select_mode = d.select_mode; }
        });
    }
}

impl NodeWidget for MultiplexerWidget {
    fn restore_from_save(&mut self, _data: Option<&serde_json::Value>, params: &[(usize, ParamValue)]) {
        self.port_type = param_usize(params, 0).map(type_from_index).unwrap_or(PortType::Any);
        self.slots = clamp_slots(param_usize(params, 1).unwrap_or(MUX_DEFAULT_SLOTS));
        self.select_mode = param_usize(params, 2).map(SelectMode::from_index).unwrap_or(SelectMode::Index);
    }

    node_identity!("Multiplexer");
    fn description(&self) -> &'static str {
        "Routes one of N typed inputs to a single output. When selecting by Index \
         (default) `select` is an integer (rounded, clamped); selecting by \
         Phase it accepts a 0..1 phase swept across \
         slots — wire in a Clock or LFO for beat-synced switching. Port \
         type auto-detected from the first data connection."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&mux_ports(self.select_mode, self.slots, self.port_type).0)
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&mux_ports(self.select_mode, self.slots, self.port_type).1)
    }

    fn show_port_labels(&self) -> bool { true }
    fn layout(&self) -> NodeLayout { labelled_layout(&self.ui_inputs(), &self.ui_outputs()) }
    fn min_width(&self) -> f32 { 110.0 }
    fn min_content_height(&self) -> f32 { 25.0 }

    fn on_ui_connect(&mut self, input_port: usize, source_type: PortType) {
        if input_port == 0 { return; } // select is Untyped; ignore for type detection
        if self.port_type == PortType::Any && source_type != PortType::Any {
            self.port_type = source_type;
            push_type(&self.shared, self.port_type);
        }
    }
    fn on_ui_output_connect(&mut self, _output_port: usize, dest_type: PortType) {
        if self.port_type == PortType::Any && dest_type != PortType::Any {
            self.port_type = dest_type;
            push_type(&self.shared, self.port_type);
        }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.sync_from_display();
        let shared = self.shared.lock();
        let sel = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<MuxDisplay>())
            .map(|d| d.selected).unwrap_or(0);
        drop(shared);
        if self.port_type == PortType::Any {
            ui.colored_label(theme::TEXT_DIM, "Connect to set type");
        } else {
            ui.colored_label(theme::TEXT_DIM, format!("in {} → out", sel + 1));
        }
    }
}

// ---------------------------------------------------------------------------
// Demultiplexer widget
// ---------------------------------------------------------------------------

pub struct DemultiplexerWidget {
    id: NodeId,
    shared: SharedState,
    port_type: PortType,
    slots: usize,
    select_mode: SelectMode,
}

impl DemultiplexerWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id, shared,
            port_type: PortType::Any,
            slots: MUX_DEFAULT_SLOTS,
            select_mode: SelectMode::Index,
        }
    }

    fn sync_from_display(&mut self) {
        with_display::<MuxDisplay, _>(&self.shared, |d| {
            if d.port_type != self.port_type { self.port_type = d.port_type; }
            if d.slots != self.slots { self.slots = d.slots; }
            if d.select_mode != self.select_mode { self.select_mode = d.select_mode; }
        });
    }
}

impl NodeWidget for DemultiplexerWidget {
    fn restore_from_save(&mut self, _data: Option<&serde_json::Value>, params: &[(usize, ParamValue)]) {
        self.port_type = param_usize(params, 0).map(type_from_index).unwrap_or(PortType::Any);
        self.slots = clamp_slots(param_usize(params, 1).unwrap_or(MUX_DEFAULT_SLOTS));
        self.select_mode = param_usize(params, 2).map(SelectMode::from_index).unwrap_or(SelectMode::Index);
    }

    node_identity!("Demultiplexer");
    fn description(&self) -> &'static str {
        "Routes a typed input to one of N outputs. Selecting by Index (default), `select` is an integer (rounded, clamped); selecting by Phase it is a 0..1 phase swept across the outputs. Unselected outputs emit zero. Port type auto-detected from the first connection; slot count set in the inspector."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&demux_ports(self.select_mode, self.slots, self.port_type).0)
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&demux_ports(self.select_mode, self.slots, self.port_type).1)
    }

    fn show_port_labels(&self) -> bool { true }
    fn layout(&self) -> NodeLayout { labelled_layout(&self.ui_inputs(), &self.ui_outputs()) }
    fn min_width(&self) -> f32 { 110.0 }
    fn min_content_height(&self) -> f32 { 25.0 }

    fn on_ui_connect(&mut self, input_port: usize, source_type: PortType) {
        if input_port == 0 { return; } // select is Untyped
        if self.port_type == PortType::Any && source_type != PortType::Any {
            self.port_type = source_type;
            push_type(&self.shared, self.port_type);
        }
    }
    fn on_ui_output_connect(&mut self, _output_port: usize, dest_type: PortType) {
        if self.port_type == PortType::Any && dest_type != PortType::Any {
            self.port_type = dest_type;
            push_type(&self.shared, self.port_type);
        }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.sync_from_display();
        let shared = self.shared.lock();
        let sel = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<MuxDisplay>())
            .map(|d| d.selected).unwrap_or(0);
        drop(shared);
        if self.port_type == PortType::Any {
            ui.colored_label(theme::TEXT_DIM, "Connect to set type");
        } else {
            ui.colored_label(theme::TEXT_DIM, format!("in → out{}", sel));
        }
    }
}
