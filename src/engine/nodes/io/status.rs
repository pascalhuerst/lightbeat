//! The leading "connected" output shared by hardware-backed nodes.
//!
//! It goes first so the port reads as the node's health indicator and stays at
//! a fixed index while the learned/analyzer ports after it come and go.

use crate::engine::types::{PortDef, PortType};
use crate::input_controller::{ConnectionStatus, ControllerRuntime, InputControllerKind};

pub fn status_port() -> PortDef {
    PortDef::new("connected", PortType::Logic)
}

/// Prepend the status port to a node's own output layout.
pub fn with_status_port(mut ports: Vec<PortDef>) -> Vec<PortDef> {
    ports.insert(0, status_port());
    ports
}

pub fn level(connected: bool) -> f32 {
    if connected { 1.0 } else { 0.0 }
}

/// Finds the controller a hardware node is bound to and mirrors its name and
/// status into the node. `None` (no such id, or another kind) also drops
/// `connected`; the caller clears its own outputs.
pub fn bind_controller<'a>(
    controllers: &'a mut [ControllerRuntime],
    id: u32,
    is_kind: fn(&InputControllerKind) -> bool,
    name: &mut String,
    connected: &mut bool,
) -> Option<&'a mut ControllerRuntime> {
    let Some(c) = controllers.iter_mut().find(|c| c.id == id && is_kind(&c.kind)) else {
        *connected = false;
        return None;
    };
    if *name != c.name {
        *name = c.name.clone();
    }
    *connected = c.status == ConnectionStatus::Connected;
    Some(c)
}
