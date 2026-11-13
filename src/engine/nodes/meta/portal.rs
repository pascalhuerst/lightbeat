//! Portals: wireless cables between nodes, matched by name.
//!
//! A **Portal Send** has input ports and a name; wire signals into it. Any
//! number of **Portal Receive** nodes bound to that name show the same ports as
//! outputs, wherever in the graph they are placed. One Send per name; its ports
//! (names and types) are defined on the Send.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::engine::nodes::meta::subgraph::{idx_to_port_type, port_type_to_idx};
use crate::engine::types::*;

/// One published portal's current state. `values` is a flat channel buffer
/// (one float per internal channel — multi-channel port types fan out).
#[derive(Debug, Clone)]
pub struct PortalEntry {
    /// The Send that publishes this name; a second Send with the same name
    /// must not take it over.
    pub owner: NodeId,
    /// Changes whenever the owner's port list does, so a Receive only copies
    /// the layout when it has changed.
    pub layout: u64,
    pub ports: Vec<PortDef>,
    pub values: Vec<f32>,
}

#[derive(Default)]
pub struct PortalRegistry {
    pub entries: HashMap<String, PortalEntry>,
}

pub type SharedPortalRegistry = Arc<Mutex<PortalRegistry>>;

/// What a Portal Send (`name`) or Receive (`bound_name`) saves, and what its
/// widget pushes. Each key is optional so a partial update only touches what
/// it names.
#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub struct PortalConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bound_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "crate::engine::config_json::lenient_vec")]
    pub ports: Option<Vec<PortalPortDef>>,
}

impl PortalConfig {
    pub fn send(name: &str, ports: &[PortDef]) -> Self {
        Self::send_defs(name, ports.iter().map(PortalPortDef::from_port_def).collect())
    }

    pub fn send_defs(name: &str, ports: Vec<PortalPortDef>) -> Self {
        Self { name: Some(name.to_string()), bound_name: None, ports: Some(ports) }
    }

    pub fn receive(bound_name: &str, ports: &[PortDef]) -> Self {
        Self::receive_defs(bound_name, ports.iter().map(PortalPortDef::from_port_def).collect())
    }

    pub fn receive_defs(bound_name: &str, ports: Vec<PortalPortDef>) -> Self {
        Self { name: None, bound_name: Some(bound_name.to_string()), ports: Some(ports) }
    }

    /// Malformed data loads as an empty update, which changes nothing.
    pub fn from_value(v: &serde_json::Value) -> Self {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

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
/// binding-side picker of Portal Receive.
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
// Portal Send: has INPUT ports, publishes layout + values.
// ---------------------------------------------------------------------------

pub struct PortalSendDisplay {
    pub name: String,
    /// Another Send already publishes this name.
    pub conflict: bool,
}

pub struct PortalSendProcessNode {
    id: NodeId,
    name: String,
    ports: Vec<PortDef>,
    values: Vec<f32>,
    registry: SharedPortalRegistry,
    conflict: bool,
    layout: u64,
}

impl PortalSendProcessNode {
    pub fn new(id: NodeId, registry: SharedPortalRegistry) -> Self {
        Self {
            id,
            name: String::new(),
            ports: Vec::new(),
            values: Vec::new(),
            registry,
            conflict: false,
            layout: 0,
        }
    }

    /// Take our name out of the registry, unless it is another Send's.
    fn release(&self) {
        if !self.name.is_empty()
            && let Ok(mut reg) = self.registry.lock()
            && reg.entries.get(&self.name).is_some_and(|e| e.owner == self.id)
        {
            reg.entries.remove(&self.name);
        }
    }
}

impl Drop for PortalSendProcessNode {
    fn drop(&mut self) {
        self.release();
    }
}

impl ProcessNode for PortalSendProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Portal Send" }
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
            if reg.entries.get(&self.name).is_some_and(|e| e.owner != self.id) {
                self.conflict = true;
                return;
            }
            self.conflict = false;
            match reg.entries.get_mut(&self.name) {
                Some(e) if e.owner == self.id && e.layout == self.layout && e.values.len() == self.values.len() => {
                    e.values.copy_from_slice(&self.values);
                }
                _ => {
                    reg.entries.insert(
                        self.name.clone(),
                        PortalEntry {
                            owner: self.id,
                            layout: self.layout,
                            ports: self.ports.clone(),
                            values: self.values.clone(),
                        },
                    );
                }
            }
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(PortalConfig::send(&self.name, &self.ports).to_value())
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        self.release();
        self.layout += 1;
        let cfg = PortalConfig::from_value(data);
        if let Some(n) = cfg.name {
            self.name = n;
        }
        if let Some(ports) = cfg.ports {
            self.ports = ports.iter().map(PortalPortDef::to_port_def).collect();
            self.values = vec![0.0; total_channels(&self.ports)];
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(PortalSendDisplay {
            name: self.name.clone(),
            conflict: self.conflict,
        }));
    }
}

// ---------------------------------------------------------------------------
// Portal Receive: has OUTPUT ports, mirrors a Send's layout + values.
// ---------------------------------------------------------------------------

pub struct PortalReceiveDisplay {
    pub bound_name: String,
    pub port_defs: Vec<PortDef>,
    pub connected: bool,
}

pub struct PortalReceiveProcessNode {
    id: NodeId,
    bound_name: String,
    ports: Vec<PortDef>,
    values: Vec<f32>,
    registry: SharedPortalRegistry,
    connected: bool,
    /// Owner and layout of the entry whose ports we hold; `None` after our
    /// own ports were set from a save.
    seen: Option<(NodeId, u64)>,
}

impl PortalReceiveProcessNode {
    pub fn new(id: NodeId, registry: SharedPortalRegistry) -> Self {
        Self {
            id,
            bound_name: String::new(),
            ports: Vec::new(),
            values: Vec::new(),
            registry,
            connected: false,
            seen: None,
        }
    }
}

impl ProcessNode for PortalReceiveProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Portal Receive" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.ports }

    fn process(&mut self) {
        if self.bound_name.is_empty() {
            self.ports.clear();
            self.values.clear();
            self.seen = None;
            self.connected = false;
            return;
        }
        if let Ok(reg) = self.registry.lock() {
            match reg.entries.get(&self.bound_name) {
                Some(entry) => {
                    if self.seen != Some((entry.owner, entry.layout)) {
                        self.ports = entry.ports.clone();
                        self.seen = Some((entry.owner, entry.layout));
                    }
                    if self.values.len() == entry.values.len() {
                        self.values.copy_from_slice(&entry.values);
                    } else {
                        self.values = entry.values.clone();
                    }
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
        Some(PortalConfig::receive(&self.bound_name, &self.ports).to_value())
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        let cfg = PortalConfig::from_value(data);
        if let Some(n) = cfg.bound_name {
            self.bound_name = n;
        }
        if let Some(ports) = cfg.ports {
            self.ports = ports.iter().map(PortalPortDef::to_port_def).collect();
            self.seen = None;
            self.values = vec![0.0; total_channels(&self.ports)];
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(PortalReceiveDisplay {
            bound_name: self.bound_name.clone(),
            port_defs: self.ports.clone(),
            connected: self.connected,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn send(id: u64, reg: &SharedPortalRegistry, name: &str) -> PortalSendProcessNode {
        let mut n = PortalSendProcessNode::new(NodeId(id), reg.clone());
        n.load_data(&serde_json::json!({"name": name, "ports": [{"name": "a", "port_type_idx": 2}]}));
        n
    }

    #[test]
    fn a_receive_sees_what_the_send_gets() {
        let reg: SharedPortalRegistry = Default::default();
        let mut tx = send(1, &reg, "bus");
        let mut rx = PortalReceiveProcessNode::new(NodeId(2), reg.clone());
        rx.load_data(&serde_json::json!({"bound_name": "bus"}));
        tx.write_input(0, 0.75);
        tx.process();
        rx.process();
        assert_eq!(rx.read_output(0), 0.75);
    }

    #[test]
    fn a_value_still_arrives_after_the_port_list_changes() {
        let reg: SharedPortalRegistry = Default::default();
        let mut tx = send(1, &reg, "bus");
        let mut rx = PortalReceiveProcessNode::new(NodeId(2), reg.clone());
        rx.load_data(&serde_json::json!({"bound_name": "bus"}));
        tx.write_input(0, 0.5);
        tx.process();
        rx.process();
        assert_eq!(rx.outputs().len(), 1);

        // A second port is added.
        tx.load_data(&serde_json::json!({"name": "bus", "ports": [
            {"name": "a", "port_type_idx": 2},
            {"name": "b", "port_type_idx": 2},
        ]}));
        tx.write_input(0, 0.25);
        tx.write_input(1, 0.75);
        tx.process();
        rx.process();
        assert_eq!(rx.outputs().len(), 2, "the receive follows the new layout");
        assert_eq!((rx.read_output(0), rx.read_output(1)), (0.25, 0.75));

        tx.write_input(1, 0.9);
        tx.process();
        rx.process();
        assert_eq!(rx.read_output(1), 0.9, "values keep flowing without a layout change");
    }

    #[test]
    fn a_second_send_cannot_take_a_name() {
        let reg: SharedPortalRegistry = Default::default();
        let mut first = send(1, &reg, "bus");
        let mut second = send(2, &reg, "bus");
        first.write_input(0, 1.0);
        first.process();
        second.write_input(0, 5.0);
        second.process();
        assert!(second.conflict && !first.conflict);
        assert_eq!(reg.lock().unwrap().entries["bus"].values[0], 1.0);
        drop(second);
        assert!(reg.lock().unwrap().entries.contains_key("bus"));
    }

    #[test]
    fn the_hand_written_save_shapes_load_and_save_unchanged() {
        let reg: SharedPortalRegistry = Default::default();
        let sent = serde_json::json!({"name": "bus", "ports": [{"name": "a", "port_type_idx": 2}]});
        let tx = send(1, &reg, "bus");
        assert_eq!(tx.save_data(), Some(sent));

        let received = serde_json::json!({"bound_name": "bus", "ports": [{"name": "a", "port_type_idx": 2}]});
        let mut rx = PortalReceiveProcessNode::new(NodeId(2), reg.clone());
        rx.load_data(&received);
        assert_eq!(rx.bound_name, "bus");
        assert_eq!(rx.save_data(), Some(received));
    }

    #[test]
    fn a_port_that_does_not_parse_is_dropped_and_the_rest_kept() {
        let reg: SharedPortalRegistry = Default::default();
        let mut rx = PortalReceiveProcessNode::new(NodeId(2), reg);
        rx.load_data(&serde_json::json!({"ports": [{"name": "a", "port_type_idx": 2}, {"nonsense": true}]}));
        assert_eq!(rx.ports.len(), 1);
    }
}
