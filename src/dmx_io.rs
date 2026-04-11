use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::interfaces::DmxOutput;
use crate::objects::object::Object;
use crate::objects::universe::DmxUniverse;

// ---------------------------------------------------------------------------
// Per-channel override
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct DmxOverride {
    pub values: [u8; 512],
    pub active: [bool; 512],
}

impl DmxOverride {
    pub fn new() -> Self {
        Self {
            values: [0u8; 512],
            active: [false; 512],
        }
    }

    pub fn set(&mut self, channel: usize, value: u8) {
        if channel < 512 {
            self.values[channel] = value;
            self.active[channel] = true;
        }
    }

    pub fn clear_all(&mut self) {
        self.active = [false; 512];
    }
}

// ---------------------------------------------------------------------------
// Universe address key
// ---------------------------------------------------------------------------

/// Identifies a specific universe on a specific interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UniverseKey {
    pub interface_id: u32,
    pub net: u8,
    pub subnet: u8,
    pub universe: u8,
}

impl UniverseKey {
    pub fn label(&self) -> String {
        format!("{}.{}.{}", self.net, self.subnet, self.universe)
    }
}

// ---------------------------------------------------------------------------
// Shared state (engine ↔ UI)
// ---------------------------------------------------------------------------

/// Per-universe output data visible to the UI.
pub struct UniverseOutput {
    pub channels: [u8; 512],
    pub overrides: DmxOverride,
}

impl UniverseOutput {
    pub fn new() -> Self {
        Self {
            channels: [0u8; 512],
            overrides: DmxOverride::new(),
        }
    }
}

/// Shared state between the DmxOutputManager (engine) and the UI.
pub struct DmxSharedState {
    /// Output data per universe, keyed by (interface_id, net, subnet, universe).
    pub universes: HashMap<UniverseKey, UniverseOutput>,
    /// Which universe the DMX monitor is currently viewing.
    pub monitor_key: Option<UniverseKey>,
    /// Global bypass: stop sending DMX entirely.
    pub bypass: bool,
    /// Global blackout: force all channels to 0 on the wire.
    pub blackout: bool,
}

impl DmxSharedState {
    pub fn new() -> Self {
        Self {
            universes: HashMap::new(),
            monitor_key: None,
            bypass: false,
            blackout: false,
        }
    }
}

pub type SharedDmxState = Arc<Mutex<DmxSharedState>>;

pub fn new_shared_dmx_state() -> SharedDmxState {
    Arc::new(Mutex::new(DmxSharedState::new()))
}

// ---------------------------------------------------------------------------
// Object store
// ---------------------------------------------------------------------------

pub struct ObjectStore {
    pub objects: Vec<Object>,
}

impl ObjectStore {
    pub fn new() -> Self {
        Self { objects: Vec::new() }
    }

    pub fn object_mut(&mut self, id: u32) -> Option<&mut Object> {
        self.objects.iter_mut().find(|o| o.id == id)
    }
}

pub type SharedObjectStore = Arc<Mutex<ObjectStore>>;

pub fn new_shared_object_store() -> SharedObjectStore {
    Arc::new(Mutex::new(ObjectStore::new()))
}

// ---------------------------------------------------------------------------
// Per-interface output handler
// ---------------------------------------------------------------------------

struct InterfaceOutput {
    id: u32,
    output: Box<dyn DmxOutput>,
    /// Universe buffers for this interface, keyed by (net, subnet, universe).
    universes: HashMap<(u8, u8, u8), DmxUniverse>,
}

// ---------------------------------------------------------------------------
// DmxOutputManager
// ---------------------------------------------------------------------------

const SEND_EVERY_N_TICKS: u32 = 23; // ~44fps at 1kHz

pub struct DmxOutputManager {
    shared: SharedDmxState,
    object_store: SharedObjectStore,
    interfaces: Vec<InterfaceOutput>,
    send_counter: u32,
}

impl DmxOutputManager {
    pub fn new(shared: SharedDmxState, object_store: SharedObjectStore) -> Self {
        Self {
            shared,
            object_store,
            interfaces: Vec::new(),
            send_counter: 0,
        }
    }

    /// Replace all interfaces. Each gets an id + output.
    pub fn set_interfaces(&mut self, interfaces: Vec<(u32, Box<dyn DmxOutput>)>) {
        self.interfaces = interfaces
            .into_iter()
            .map(|(id, output)| InterfaceOutput {
                id,
                output,
                universes: HashMap::new(),
            })
            .collect();
    }

    pub fn tick(&mut self) {
        // 1. Write all object channel values into the correct interface's universe buffer.
        {
            let store = self.object_store.lock().unwrap();
            for obj in &store.objects {
                // Find (or create) the universe buffer on the object's assigned interface.
                let iface = self.interfaces.iter_mut().find(|i| i.id == obj.interface_id);
                if let Some(iface) = iface {
                    let uni_key = (obj.address.net, obj.address.subnet, obj.address.universe);
                    let universe = iface.universes
                        .entry(uni_key)
                        .or_insert_with(|| DmxUniverse::new(uni_key.0, uni_key.1, uni_key.2));
                    obj.write_to_universe(universe);
                }
            }
        }

        // 2. Apply overrides and update shared output for the monitored universe(s).
        {
            let mut shared = self.shared.lock().unwrap();

            // Update all universe outputs in shared state.
            for iface in &self.interfaces {
                for (&(net, subnet, uni), universe) in &iface.universes {
                    let key = UniverseKey {
                        interface_id: iface.id,
                        net,
                        subnet,
                        universe: uni,
                    };

                    let entry = shared.universes.entry(key).or_insert_with(UniverseOutput::new);

                    // Merge: universe data + overrides.
                    let mut output = universe.channels;
                    for i in 0..512 {
                        if entry.overrides.active[i] {
                            output[i] = entry.overrides.values[i];
                        }
                    }
                    entry.channels = output;
                }
            }
        }

        // 3. Send to interfaces (rate-limited).
        self.send_counter += 1;
        let shared = self.shared.lock().unwrap();
        let bypass = shared.bypass;
        let blackout = shared.blackout;

        // Grab override data for merging into sends.
        let overrides: HashMap<UniverseKey, DmxOverride> = shared.universes
            .iter()
            .map(|(k, v)| (*k, v.overrides.clone()))
            .collect();
        drop(shared);

        if !bypass && self.send_counter >= SEND_EVERY_N_TICKS {
            self.send_counter = 0;

            for iface in &mut self.interfaces {
                for (&(net, subnet, uni), universe) in &iface.universes {
                    let mut send_uni = universe.clone();

                    if blackout {
                        send_uni.blackout();
                    } else {
                        // Apply overrides to the send buffer.
                        let key = UniverseKey {
                            interface_id: iface.id,
                            net,
                            subnet,
                            universe: uni,
                        };
                        if let Some(ovr) = overrides.get(&key) {
                            for i in 0..512 {
                                if ovr.active[i] {
                                    send_uni.channels[i] = ovr.values[i];
                                }
                            }
                        }
                    }

                    if let Err(e) = iface.output.send_universe(&send_uni) {
                        eprintln!("DMX send error: {}", e);
                    }
                }
            }
        }

        // 4. Mark all universes clean.
        for iface in &mut self.interfaces {
            for universe in iface.universes.values_mut() {
                universe.mark_clean();
            }
        }
    }
}
