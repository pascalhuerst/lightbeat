//! The leading "connected" output shared by hardware-backed nodes.
//!
//! It goes first so the port reads as the node's health indicator and stays at
//! a fixed index while the learned/analyzer ports after it come and go.

use crate::engine::types::{PortDef, PortType};

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
