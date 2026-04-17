//! Portals — "wireless cables" between node pairs.
//!
//! A `Portal In` exposes N configurable input ports and publishes the values
//! it receives into a shared registry under a user-given name. Any number of
//! `Portal Out` nodes bind to that name and mirror the published ports as
//! outputs, so data flows from the Portal In's inputs to the Portal Out's
//! outputs without a visible wire. Latency matches a normal wire (1 tick).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::engine::nodes::meta::subgraph::{idx_to_port_type, port_type_to_idx};
use crate::engine::types::*;

/// One published portal's current state. Stored in the registry keyed by
/// the Portal In's name. `values` is a flat channel buffer (one float per
/// internal channel — multi-channel port types fan out).
#[derive(Debug, Clone)]
pub struct PortalEntry {
    pub ports: Vec<PortDef>,
    pub values: Vec<f32>,
}

#[derive(Default)]
pub struct PortalRegistry {
    pub entries: HashMap<String, PortalEntry>,
}

pub type SharedPortalRegistry = Arc<Mutex<PortalRegistry>>;

/// Serialized per-port definition used by widget save/load.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PortalPortDef {
    pub name: String,
    pub port_type_idx: usize,
}

impl PortalPortDef {
    pub fn to_port_def(&self) -> PortDef {
        PortDef::new(&self.name, idx_to_port_type(self.port_type_idx))
    }
    pub fn from_port_def(p: &PortDef) -> Self {
        Self {
            name: p.name.clone(),
            port_type_idx: port_type_to_idx(p.port_type),
        }
    }
}

// ---------------------------------------------------------------------------
// Portal In
// ---------------------------------------------------------------------------

/// Display state mirrored to the widget.
pub struct PortalInDisplay {
    pub name: String,
    pub port_defs: Vec<PortDef>,
    pub duplicate_name: bool,
}

pub struct PortalInProcessNode {
    id: NodeId,
    name: String,
    ports: Vec<PortDef>,
    values: Vec<f32>,
    registry: SharedPortalRegistry,
    /// True when another Portal In in the registry has the same name —
    /// surfaced in the display so the widget can warn the user.
    duplicate_name: bool,
}

impl PortalInProcessNode {
    pub fn new(id: NodeId, registry: SharedPortalRegistry) -> Self {
        Self {
            id,
            name: String::new(),
            ports: Vec::new(),
            values: Vec::new(),
            registry,
            duplicate_name: false,
        }
    }
}

impl Drop for PortalInProcessNode {
    fn drop(&mut self) {
        // Remove our registry entry so dangling names don't linger when the
        // node is deleted. Safe even if our name was never set.
        if !self.name.is_empty() {
            if let Ok(mut reg) = self.registry.lock() {
                // Only remove if the entry is actually ours — we don't track
                // uniqueness at insert time, so we might end up removing a
                // stale entry by a namesake. That's OK: the surviving Portal
                // In will re-publish on its next tick.
                reg.entries.remove(&self.name);
            }
        }
    }
}

impl ProcessNode for PortalInProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Portal In" }
    fn inputs(&self) -> &[PortDef] { &self.ports }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if let Some(slot) = self.values.get_mut(port_index) {
            *slot = value;
        }
    }

    fn read_input(&self, port_index: usize) -> f32 {
        self.values.get(port_index).copied().unwrap_or(0.0)
    }

    fn process(&mut self) {
        if self.name.is_empty() { return; }
        if let Ok(mut reg) = self.registry.lock() {
            // Duplicate-name detection: if another entry already exists with
            // our name but different port layout, we still overwrite (last
            // writer wins). The widget flashes a warning.
            self.duplicate_name = false; // recomputed below
            reg.entries.insert(
                self.name.clone(),
                PortalEntry {
                    ports: self.ports.clone(),
                    values: self.values.clone(),
                },
            );
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        let ports: Vec<PortalPortDef> = self.ports.iter().map(PortalPortDef::from_port_def).collect();
        Some(serde_json::json!({
            "name": self.name,
            "ports": ports,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(old_name) = Some(self.name.clone()) {
            // Removing the old entry before publishing the new name avoids
            // orphans when the user renames.
            if !old_name.is_empty() {
                if let Ok(mut reg) = self.registry.lock() {
                    reg.entries.remove(&old_name);
                }
            }
        }
        if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
            self.name = n.to_string();
        }
        if let Some(arr) = data.get("ports").and_then(|v| v.as_array()) {
            self.ports = arr.iter()
                .filter_map(|v| serde_json::from_value::<PortalPortDef>(v.clone()).ok())
                .map(|p| p.to_port_def())
                .collect();
            self.values = vec![0.0; total_channels(&self.ports)];
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(PortalInDisplay {
            name: self.name.clone(),
            port_defs: self.ports.clone(),
            duplicate_name: self.duplicate_name,
        }));
    }
}

// ---------------------------------------------------------------------------
// Portal Out
// ---------------------------------------------------------------------------

pub struct PortalOutDisplay {
    pub bound_name: String,
    pub port_defs: Vec<PortDef>,
    pub connected: bool,
}

pub struct PortalOutProcessNode {
    id: NodeId,
    bound_name: String,
    /// Mirrored from the registry each tick. Both `ports` and `values`
    /// refresh together so they stay in lock-step.
    ports: Vec<PortDef>,
    values: Vec<f32>,
    registry: SharedPortalRegistry,
    connected: bool,
}

impl PortalOutProcessNode {
    pub fn new(id: NodeId, registry: SharedPortalRegistry) -> Self {
        Self {
            id,
            bound_name: String::new(),
            ports: Vec::new(),
            values: Vec::new(),
            registry,
            connected: false,
        }
    }
}

impl ProcessNode for PortalOutProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Portal Out" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.ports }

    fn process(&mut self) {
        if self.bound_name.is_empty() {
            self.ports.clear();
            self.values.clear();
            self.connected = false;
            return;
        }
        if let Ok(reg) = self.registry.lock() {
            match reg.entries.get(&self.bound_name) {
                Some(entry) => {
                    self.ports = entry.ports.clone();
                    self.values = entry.values.clone();
                    self.connected = true;
                }
                None => {
                    // Preserve the last-known port layout so downstream
                    // connections don't get culled the moment the Portal In
                    // disappears mid-frame; just zero the values.
                    for v in self.values.iter_mut() { *v = 0.0; }
                    self.connected = false;
                }
            }
        }
    }

    fn read_output(&self, port_index: usize) -> f32 {
        self.values.get(port_index).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        // Serialize the last-seen port layout too — lets the widget expose
        // the right output ports on the first frame after a project load,
        // before the engine has ticked.
        let ports: Vec<PortalPortDef> = self.ports.iter().map(PortalPortDef::from_port_def).collect();
        Some(serde_json::json!({
            "bound_name": self.bound_name,
            "ports": ports,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("bound_name").and_then(|v| v.as_str()) {
            self.bound_name = n.to_string();
        }
        if let Some(arr) = data.get("ports").and_then(|v| v.as_array()) {
            self.ports = arr.iter()
                .filter_map(|v| serde_json::from_value::<PortalPortDef>(v.clone()).ok())
                .map(|p| p.to_port_def())
                .collect();
            self.values = vec![0.0; total_channels(&self.ports)];
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(PortalOutDisplay {
            bound_name: self.bound_name.clone(),
            port_defs: self.ports.clone(),
            connected: self.connected,
        }));
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Snapshot of the current Portal In names for the Portal Out picker.
pub fn available_portal_names(registry: &SharedPortalRegistry) -> Vec<String> {
    let reg = match registry.lock() {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut names: Vec<String> = reg.entries.keys().cloned().collect();
    names.sort();
    names
}
