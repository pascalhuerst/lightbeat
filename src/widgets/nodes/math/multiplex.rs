use std::any::Any;

use egui::{self, Ui};

use crate::engine::nodes::math::multiplex::{
    MUX_DEFAULT_SLOTS, MuxDisplay, SelectMode, clamp_slots, type_to_index,
};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::{NodeLayout, NodeWidget, labelled_layout};
use crate::widgets::nodes::types::UiPortDef;

fn push_type(shared: &SharedState, port_type: PortType) {
    let mut s = shared.lock().unwrap();
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

    /// Apply restored state before the first frame's stale-connection sweep.
    pub fn set_state_from_load(&mut self, port_type: PortType, slots: usize, mode: SelectMode) {
        self.port_type = port_type;
        self.slots = clamp_slots(slots);
        self.select_mode = mode;
    }

    fn sync_from_display(&mut self) {
        let shared = self.shared.lock().unwrap();
        if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<MuxDisplay>()) {
            if d.port_type != self.port_type { self.port_type = d.port_type; }
            if d.slots != self.slots { self.slots = d.slots; }
            if d.select_mode != self.select_mode { self.select_mode = d.select_mode; }
        }
    }
}

impl NodeWidget for MultiplexerWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Multiplexer" }
    fn description(&self) -> &'static str {
        "Routes one of N typed inputs to a single output. In Index select \
         mode (default) `select` is an integer (rounded, clamped). In \
         Phase select mode `select` accepts a 0..1 phase swept across \
         slots — wire in a Clock or LFO for beat-synced switching. Port \
         type auto-detected from the first data connection."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        let sel_ty = self.select_mode.select_port_type();
        let mut v = vec![UiPortDef::from_def(&PortDef::new("select", sel_ty))];
        for i in 0..self.slots {
            v.push(UiPortDef::from_def(&PortDef::new(format!("in{}", i), self.port_type)));
        }
        v
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", self.port_type))]
    }

    fn show_port_labels(&self) -> bool { true }
    fn layout(&self) -> NodeLayout { labelled_layout(&self.ui_inputs(), &self.ui_outputs()) }
    fn min_width(&self) -> f32 { 110.0 }
    fn min_content_height(&self) -> f32 { 25.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

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
        let shared = self.shared.lock().unwrap();
        let sel = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<MuxDisplay>())
            .map(|d| d.selected).unwrap_or(0);
        drop(shared);
        if self.port_type == PortType::Any {
            ui.colored_label(theme::TEXT_DIM, "Connect to set type");
        } else {
            ui.colored_label(theme::TEXT_DIM, format!("in{} → out", sel));
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
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

    pub fn set_state_from_load(&mut self, port_type: PortType, slots: usize, mode: SelectMode) {
        self.port_type = port_type;
        self.slots = clamp_slots(slots);
        self.select_mode = mode;
    }

    fn sync_from_display(&mut self) {
        let shared = self.shared.lock().unwrap();
        if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<MuxDisplay>()) {
            if d.port_type != self.port_type { self.port_type = d.port_type; }
            if d.slots != self.slots { self.slots = d.slots; }
            if d.select_mode != self.select_mode { self.select_mode = d.select_mode; }
        }
    }
}

impl NodeWidget for DemultiplexerWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Demultiplexer" }
    fn description(&self) -> &'static str {
        "Routes a typed input to one of N outputs based on the `select` index. Unselected outputs emit zero. Port type auto-detected from the first connection; slot count set in the inspector."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("select", self.select_mode.select_port_type())),
            UiPortDef::from_def(&PortDef::new("in", self.port_type)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        (0..self.slots)
            .map(|i| UiPortDef::from_def(&PortDef::new(format!("out{}", i), self.port_type)))
            .collect()
    }

    fn show_port_labels(&self) -> bool { true }
    fn layout(&self) -> NodeLayout { labelled_layout(&self.ui_inputs(), &self.ui_outputs()) }
    fn min_width(&self) -> f32 { 110.0 }
    fn min_content_height(&self) -> f32 { 25.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

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
        let shared = self.shared.lock().unwrap();
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

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
