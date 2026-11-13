//! The "Type" setting of nodes that hold, delay or smooth a signal of any
//! type (Sample & Hold, Tick Delay, Smoothing). Auto (`PortType::Any`) is the
//! neutral state; the widget turns the first wire into a type.

use crate::engine::nodes::math::multiplex::{MUX_TYPE_NAMES, type_from_index, type_to_index};
use crate::engine::types::*;

/// The widest signal, a gradient.
pub const MAX_CHANNELS: usize = GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS;

pub const TYPE_PARAM: &str = "Type";

pub fn type_param(port_type: PortType) -> ParamDef {
    ParamDef::Choice {
        name: TYPE_PARAM.into(),
        value: type_to_index(port_type),
        options: MUX_TYPE_NAMES.iter().map(|s| (*s).into()).collect(),
    }
}

pub fn type_from_param(value: &ParamValue) -> PortType {
    type_from_index(value.as_usize())
}

/// What the widget reads to follow the type the inspector or a snapshot set.
pub struct SignalTypeDisplay {
    pub port_type: PortType,
}

pub fn publish_type(shared: &mut NodeSharedState, port_type: PortType) {
    shared.display = Some(Box::new(SignalTypeDisplay { port_type }));
}
