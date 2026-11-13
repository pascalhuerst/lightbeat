use egui::Ui;

use crate::engine::nodes::math::counter::{ports_for, CounterDisplay};
use crate::engine::nodes::math::multiplex::SelectMode;
use crate::engine::types::*;
use crate::widgets::nodes::node::{NodeWidget, param_usize, node_identity};
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

    fn sync_from_display(&mut self) {
        let shared = self.shared.lock().unwrap();
        if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<CounterDisplay>())
            && d.select_mode != self.select_mode {
                self.select_mode = d.select_mode;
            }
    }
}

impl NodeWidget for CounterWidget {
    fn restore_from_save(&mut self, _data: Option<&serde_json::Value>, params: &[(usize, ParamValue)]) {
        self.select_mode = param_usize(params, 2).map(SelectMode::from_index).unwrap_or(SelectMode::Index);
    }

    node_identity!("Counter");
    fn description(&self) -> &'static str {
        "Counts through 0..Max-1. In Index mode (default) each trigger \
         rising edge advances the count. In Phase mode `count = floor(phase \
         * max)` — sweep a clock/LFO/accumulator to walk the range \
         without a separate trigger. `wrap` pulses one tick on wrap."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        UiPortDef::from_defs(&ports_for(self.select_mode).0)
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
        UiPortDef::from_defs(&ports_for(self.select_mode).1)
    }

    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 15.0 }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.sync_from_display();
        let shared = self.shared.lock().unwrap();
        let count = shared.outputs.first().copied().unwrap_or(0.0);
        drop(shared);
        ui.label(format!("{}", count as i64));
    }
}
