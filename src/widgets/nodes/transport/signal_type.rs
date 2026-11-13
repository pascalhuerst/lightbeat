//! The widget side of the "Type" setting shared by Sample & Hold, Tick Delay
//! and Smoothing: starts on Auto and takes the type of the first wire.

use crate::engine::nodes::math::multiplex::{MUX_TYPE_NAMES, type_to_index};
use crate::engine::nodes::transport::signal_type::SignalTypeDisplay;
use crate::engine::types::*;
use crate::widgets::nodes::node::param_usize;

pub struct SignalTypeSetting {
    pub port_type: PortType,
    /// Where the node keeps the type among its settings.
    param_index: usize,
}

impl SignalTypeSetting {
    pub fn new(param_index: usize) -> Self {
        Self { port_type: PortType::Any, param_index }
    }

    /// A file from before the setting existed has no entry, which means Auto:
    /// the wires loaded next set the type, and those carried plain numbers.
    pub fn restore(&mut self, params: &[(usize, ParamValue)]) {
        self.port_type = param_usize(params, self.param_index)
            .map(crate::engine::nodes::math::multiplex::type_from_index)
            .unwrap_or(PortType::Any);
    }

    /// Follow what the engine runs, so the inspector and snapshots show.
    pub fn sync(&mut self, shared: &SharedState) {
        let s = shared.lock();
        if let Some(d) = s.display.as_ref().and_then(|d| d.downcast_ref::<SignalTypeDisplay>()) {
            self.port_type = d.port_type;
        }
    }

    /// What the body shows: the number, or the type's name for a bundle of
    /// channels that one number cannot stand for.
    pub fn summary(&self, first_channel: f32) -> String {
        if self.port_type.channel_count() > 1 {
            MUX_TYPE_NAMES[type_to_index(self.port_type)].to_string()
        } else {
            format!("{:.3}", first_channel)
        }
    }

    pub fn adopt(&mut self, shared: &SharedState, wire_type: PortType) {
        if self.port_type != PortType::Any || wire_type == PortType::Any { return; }
        self.port_type = wire_type;
        shared.lock().pending_params
            .push((self.param_index, ParamValue::Choice(type_to_index(wire_type))));
    }
}
