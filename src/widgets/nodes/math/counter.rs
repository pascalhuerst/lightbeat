use std::any::Any;
use egui::Ui;

use crate::engine::nodes::math::counter::CounterDisplay;
use crate::engine::nodes::math::multiplex::SelectMode;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct CounterWidget {
    id: NodeId,
    shared: SharedState,
    select_mode: SelectMode,
}

impl CounterWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, select_mode: SelectMode::Index }
    }

    /// Apply restored state before the first frame's stale-connection
    /// sweep — matches the Mux/Demux loader path. Called from project
    /// load and from move-to-subgraph.
    pub fn set_state_from_load(&mut self, mode: SelectMode) {
        self.select_mode = mode;
    }

    fn sync_from_display(&mut self) {
        let shared = self.shared.lock().unwrap();
        if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<CounterDisplay>())
            && d.select_mode != self.select_mode {
                self.select_mode = d.select_mode;
            }
    }
}

impl NodeWidget for CounterWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Counter" }
    fn description(&self) -> &'static str {
        "Counts through 0..Max-1. In Index mode (default) each trigger \
         rising edge advances the count. In Phase mode `count = floor(phase \
         * max)` — sweep a clock/LFO/accumulator to walk the range \
         without a separate trigger. `wrap` pulses one tick on wrap."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        let port0 = match self.select_mode {
            SelectMode::Index => PortDef::new("trigger", PortType::Logic),
            SelectMode::Phase => PortDef::new("phase", PortType::Phase),
        };
        vec![
            UiPortDef::from_def(&port0),
            UiPortDef::from_def(&PortDef::new("reset", PortType::Logic)),
            UiPortDef::from_def(&PortDef::new("max", PortType::Untyped)),
        ]
    }

    fn overridden_param_indices(&self) -> Vec<usize> {
        // Hide the "Max" param (index 0) when the `max` input port (logical
        // index 2) is wired — its value is what the engine actually uses.
        let s = self.shared.lock().unwrap();
        if s.inputs_connected.get(2).copied().unwrap_or(false) {
            vec![0]
        } else {
            vec![]
        }
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("count", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("wrap", PortType::Logic)),
        ]
    }

    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 15.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.sync_from_display();
        let shared = self.shared.lock().unwrap();
        let count = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(format!("{}", count as i64));
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
