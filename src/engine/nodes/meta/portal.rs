//! Portals — "wireless cables" between node pairs.
//!
//! Two families, matching the two signal-flow directions:
//!
//! - **Output Portal** beams an *output* signal to many consumers. One TX
//!   (singleton, defines layout, has INPUT ports) publishes; many RX
//!   widgets bound to the same name mirror those ports as OUTPUTS and
//!   hand them to downstream nodes. Use it for "broadcast this signal to
//!   scattered places in the graph".
//!
//! - **Input Portal** makes a local output-hub that's fed from somewhere
//!   far away. One RX (singleton, defines layout, has OUTPUT ports) is
//!   placed near the consumers; exactly one TX (singleton, has INPUT
//!   ports) elsewhere accepts the upstream signal and pipes it into the
//!   RX. Use it for "consumers here, source lives elsewhere".
//!
//! The two families use independent registries so a name can be reused
//! across them without collision.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::engine::nodes::meta::subgraph::{idx_to_port_type, port_type_to_idx};
use crate::engine::types::*;

/// One published portal's current state. `values` is a flat channel buffer
/// (one float per internal channel — multi-channel port types fan out).
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

/// Snapshot of the current portal names in a registry — used by the
/// binding-side picker (Output Portal RX and Input Portal TX).
pub fn available_portal_names(registry: &SharedPortalRegistry) -> Vec<String> {
    let reg = match registry.lock() {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut names: Vec<String> = reg.entries.keys().cloned().collect();
    names.sort();
    names
}

// ---------------------------------------------------------------------------
// Output Portal TX — has INPUT ports, publishes layout + values.
// Was "Portal In" pre-rename.
// ---------------------------------------------------------------------------

pub struct OutputPortalTxDisplay {
    pub name: String,
    pub port_defs: Vec<PortDef>,
}

pub struct OutputPortalTxProcessNode {
    id: NodeId,
    name: String,
    ports: Vec<PortDef>,
    values: Vec<f32>,
    registry: SharedPortalRegistry,
}

impl OutputPortalTxProcessNode {
    pub fn new(id: NodeId, registry: SharedPortalRegistry) -> Self {
        Self {
            id,
            name: String::new(),
            ports: Vec::new(),
            values: Vec::new(),
            registry,
        }
    }
}

impl Drop for OutputPortalTxProcessNode {
    fn drop(&mut self) {
        if !self.name.is_empty()
            && let Ok(mut reg) = self.registry.lock() {
                reg.entries.remove(&self.name);
            }
    }
}

impl ProcessNode for OutputPortalTxProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Output Portal TX" }
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
            reg.entries.insert(
                self.name.clone(),
                PortalEntry { ports: self.ports.clone(), values: self.values.clone() },
            );
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        let ports: Vec<PortalPortDef> = self.ports.iter().map(PortalPortDef::from_port_def).collect();
        Some(serde_json::json!({ "name": self.name, "ports": ports }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        let old_name = self.name.clone();
        if !old_name.is_empty()
            && let Ok(mut reg) = self.registry.lock() {
                reg.entries.remove(&old_name);
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
        shared.display = Some(Box::new(OutputPortalTxDisplay {
            name: self.name.clone(),
            port_defs: self.ports.clone(),
        }));
    }
}

// ---------------------------------------------------------------------------
// Output Portal RX — has OUTPUT ports, mirrors a TX's layout + values.
// Was "Portal Out" pre-rename.
// ---------------------------------------------------------------------------

pub struct OutputPortalRxDisplay {
    pub bound_name: String,
    pub port_defs: Vec<PortDef>,
    pub connected: bool,
}

pub struct OutputPortalRxProcessNode {
    id: NodeId,
    bound_name: String,
    ports: Vec<PortDef>,
    values: Vec<f32>,
    registry: SharedPortalRegistry,
    connected: bool,
}

impl OutputPortalRxProcessNode {
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

impl ProcessNode for OutputPortalRxProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Output Portal RX" }
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
                    // wires survive the TX disappearing mid-frame.
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
        let ports: Vec<PortalPortDef> = self.ports.iter().map(PortalPortDef::from_port_def).collect();
        Some(serde_json::json!({ "bound_name": self.bound_name, "ports": ports }))
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
        shared.display = Some(Box::new(OutputPortalRxDisplay {
            bound_name: self.bound_name.clone(),
            port_defs: self.ports.clone(),
            connected: self.connected,
        }));
    }
}

// ---------------------------------------------------------------------------
// Input Portal RX — has OUTPUT ports, defines layout. Reads values from
// the paired TX via the input-portal registry.
// ---------------------------------------------------------------------------

pub struct InputPortalRxDisplay {
    pub name: String,
    pub port_defs: Vec<PortDef>,
}

pub struct InputPortalRxProcessNode {
    id: NodeId,
    name: String,
    ports: Vec<PortDef>,
    values: Vec<f32>,
    registry: SharedPortalRegistry,
}

impl InputPortalRxProcessNode {
    pub fn new(id: NodeId, registry: SharedPortalRegistry) -> Self {
        Self {
            id,
            name: String::new(),
            ports: Vec::new(),
            values: Vec::new(),
            registry,
        }
    }
}

impl Drop for InputPortalRxProcessNode {
    fn drop(&mut self) {
        if !self.name.is_empty()
            && let Ok(mut reg) = self.registry.lock() {
                reg.entries.remove(&self.name);
            }
    }
}

impl ProcessNode for InputPortalRxProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Input Portal RX" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.ports }

    fn process(&mut self) {
        if self.name.is_empty() { return; }
        if let Ok(mut reg) = self.registry.lock() {
            // Publish own port layout so the TX widget can mirror it.
            // Preserve (and subsequently read) whatever values the TX
            // already wrote this tick — otherwise we'd flush them.
            let entry = reg.entries.entry(self.name.clone()).or_insert_with(|| PortalEntry {
                ports: self.ports.clone(),
                values: vec![0.0; total_channels(&self.ports)],
            });
            entry.ports = self.ports.clone();
            // Resize values buffer to match our port layout; zero-fill any
            // new slots.
            let needed = total_channels(&self.ports);
            if entry.values.len() != needed {
                entry.values.resize(needed, 0.0);
            }
            self.values = entry.values.clone();
        }
    }

    fn read_output(&self, port_index: usize) -> f32 {
        self.values.get(port_index).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        let ports: Vec<PortalPortDef> = self.ports.iter().map(PortalPortDef::from_port_def).collect();
        Some(serde_json::json!({ "name": self.name, "ports": ports }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        let old_name = self.name.clone();
        if !old_name.is_empty()
            && let Ok(mut reg) = self.registry.lock() {
                reg.entries.remove(&old_name);
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
        shared.display = Some(Box::new(InputPortalRxDisplay {
            name: self.name.clone(),
            port_defs: self.ports.clone(),
        }));
    }
}

// ---------------------------------------------------------------------------
// Input Portal TX — has INPUT ports, mirrors the paired RX's layout and
// forwards its current input values to the input-portal registry.
// ---------------------------------------------------------------------------

pub struct InputPortalTxDisplay {
    pub bound_name: String,
    pub port_defs: Vec<PortDef>,
    pub connected: bool,
}

pub struct InputPortalTxProcessNode {
    id: NodeId,
    bound_name: String,
    ports: Vec<PortDef>,
    values: Vec<f32>,
    registry: SharedPortalRegistry,
    connected: bool,
}

impl InputPortalTxProcessNode {
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

impl ProcessNode for InputPortalTxProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Input Portal TX" }
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
        if self.bound_name.is_empty() {
            self.ports.clear();
            self.values.clear();
            self.connected = false;
            return;
        }
        if let Ok(mut reg) = self.registry.lock() {
            match reg.entries.get_mut(&self.bound_name) {
                Some(entry) => {
                    // Mirror the RX's port layout — it owns the definition.
                    // Compare by (name, type) pairs since PortDef itself
                    // isn't PartialEq.
                    let layout_changed = entry.ports.len() != self.ports.len()
                        || entry.ports.iter().zip(self.ports.iter()).any(|(a, b)| {
                            a.name != b.name || a.port_type != b.port_type
                        });
                    if layout_changed {
                        self.ports = entry.ports.clone();
                        self.values.resize(total_channels(&self.ports), 0.0);
                    }
                    // Write our input values back so the RX picks them up on
                    // its next tick.
                    let n = entry.values.len().min(self.values.len());
                    entry.values[..n].copy_from_slice(&self.values[..n]);
                    self.connected = true;
                }
                None => {
                    for v in self.values.iter_mut() { *v = 0.0; }
                    self.connected = false;
                }
            }
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        let ports: Vec<PortalPortDef> = self.ports.iter().map(PortalPortDef::from_port_def).collect();
        Some(serde_json::json!({ "bound_name": self.bound_name, "ports": ports }))
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
        shared.display = Some(Box::new(InputPortalTxDisplay {
            bound_name: self.bound_name.clone(),
            port_defs: self.ports.clone(),
            connected: self.connected,
        }));
    }
}
