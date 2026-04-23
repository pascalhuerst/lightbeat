use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::math::multiplex::{
    MUX_DEFAULT_SLOTS, MUX_MAX_SLOTS, MUX_MIN_SLOTS, MUX_TYPE_NAMES, MuxDisplay,
    clamp_slots, type_from_index, type_to_index,
};
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

fn push_type(shared: &SharedState, port_type: PortType) {
    let mut s = shared.lock().unwrap();
    s.pending_params.push((0, ParamValue::Choice(type_to_index(port_type))));
}

fn push_slots(shared: &SharedState, slots: usize) {
    let mut s = shared.lock().unwrap();
    s.pending_params.push((1, ParamValue::Int(slots as i64)));
}

// ---------------------------------------------------------------------------
// Multiplexer widget
// ---------------------------------------------------------------------------

pub struct MultiplexerWidget {
    id: NodeId,
    shared: SharedState,
    port_type: PortType,
    slots: usize,
}

impl MultiplexerWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, port_type: PortType::Any, slots: MUX_DEFAULT_SLOTS }
    }

    /// Apply restored state before the first frame's stale-connection sweep.
    pub fn set_state_from_load(&mut self, port_type: PortType, slots: usize) {
        self.port_type = port_type;
        self.slots = clamp_slots(slots);
    }

    fn sync_from_display(&mut self) {
        let shared = self.shared.lock().unwrap();
        if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<MuxDisplay>()) {
            if d.port_type != self.port_type { self.port_type = d.port_type; }
            if d.slots != self.slots { self.slots = d.slots; }
        }
    }
}

impl NodeWidget for MultiplexerWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Multiplexer" }
    fn description(&self) -> &'static str {
        "Routes one of N typed inputs to a single output based on the `select` index (rounded, clamped 0..N-1). Port type auto-detected from the first connection; slot count set in the inspector."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        let mut v = vec![UiPortDef::from_def(&PortDef::new("select", PortType::Untyped))];
        for i in 0..self.slots {
            v.push(UiPortDef::from_def(&PortDef::new(format!("in{}", i), self.port_type)));
        }
        v
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", self.port_type))]
    }

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
            ui.colored_label(Color32::from_gray(120), "Connect to set type");
        } else {
            ui.colored_label(Color32::from_gray(140), format!("in{} → out", sel));
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        self.sync_from_display();
        let _ = ui;
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
}

impl DemultiplexerWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, port_type: PortType::Any, slots: MUX_DEFAULT_SLOTS }
    }

    pub fn set_state_from_load(&mut self, port_type: PortType, slots: usize) {
        self.port_type = port_type;
        self.slots = clamp_slots(slots);
    }

    fn sync_from_display(&mut self) {
        let shared = self.shared.lock().unwrap();
        if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<MuxDisplay>()) {
            if d.port_type != self.port_type { self.port_type = d.port_type; }
            if d.slots != self.slots { self.slots = d.slots; }
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
            UiPortDef::from_def(&PortDef::new("select", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("in", self.port_type)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        (0..self.slots)
            .map(|i| UiPortDef::from_def(&PortDef::new(format!("out{}", i), self.port_type)))
            .collect()
    }

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
            ui.colored_label(Color32::from_gray(120), "Connect to set type");
        } else {
            ui.colored_label(Color32::from_gray(140), format!("in → out{}", sel));
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        self.sync_from_display();
        ui.label(egui::RichText::new("Type").strong());
        let cur = type_to_index(self.port_type);
        let mut new_idx = cur;
        egui::ComboBox::from_id_salt(("demux_type", self.id.0))
            .selected_text(MUX_TYPE_NAMES[cur])
            .show_ui(ui, |ui| {
                for (i, label) in MUX_TYPE_NAMES.iter().enumerate() {
                    ui.selectable_value(&mut new_idx, i, *label);
                }
            });
        if new_idx != cur {
            self.port_type = type_from_index(new_idx);
            push_type(&self.shared, self.port_type);
        }

        ui.separator();
        ui.label(egui::RichText::new("Slots").strong());
        let mut slots = self.slots;
        if ui.add(
            egui::Slider::new(&mut slots, MUX_MIN_SLOTS..=MUX_MAX_SLOTS)
                .clamping(egui::SliderClamping::Always),
        ).changed() {
            self.slots = slots;
            push_slots(&self.shared, self.slots);
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
