use std::sync::{Arc, Mutex};

use crate::interfaces::DmxOutput;
use crate::objects::fixture::Fixture;
use crate::objects::universe::DmxUniverse;

/// Per-channel override state for the DMX monitor.
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

/// Shared state between the DmxOutputManager (engine side) and the UI.
pub struct DmxSharedState {
    /// Final output values after merging fixture data + overrides.
    pub output: [u8; 512],
    /// Override layer — UI writes, engine reads and merges.
    pub overrides: DmxOverride,
    /// Bypass: stop sending DMX entirely.
    pub bypass: bool,
    /// Blackout: force all channels to 0 on the wire.
    pub blackout: bool,
}

impl DmxSharedState {
    pub fn new() -> Self {
        Self {
            output: [0u8; 512],
            overrides: DmxOverride::new(),
            bypass: false,
            blackout: false,
        }
    }
}

pub type SharedDmxState = Arc<Mutex<DmxSharedState>>;

pub fn new_shared_dmx_state() -> SharedDmxState {
    Arc::new(Mutex::new(DmxSharedState::new()))
}

/// Shared fixture store — engine nodes write values here,
/// DmxOutputManager reads them to build the universe.
pub struct FixtureStore {
    pub fixtures: Vec<Fixture>,
}

impl FixtureStore {
    pub fn new() -> Self {
        Self { fixtures: Vec::new() }
    }

    pub fn fixture_mut(&mut self, id: u32) -> Option<&mut Fixture> {
        self.fixtures.iter_mut().find(|f| f.id == id)
    }
}

pub type SharedFixtureStore = Arc<Mutex<FixtureStore>>;

pub fn new_shared_fixture_store() -> SharedFixtureStore {
    Arc::new(Mutex::new(FixtureStore::new()))
}

/// Manages DMX universe buffers, reads fixture data, merges with overrides,
/// and sends to interfaces.
pub struct DmxOutputManager {
    pub universe: DmxUniverse,
    shared: SharedDmxState,
    fixture_store: SharedFixtureStore,
    interfaces: Vec<Box<dyn DmxOutput>>,
}

impl DmxOutputManager {
    pub fn new(shared: SharedDmxState, fixture_store: SharedFixtureStore) -> Self {
        Self {
            universe: DmxUniverse::new(0, 0, 0),
            shared,
            fixture_store,
            interfaces: Vec::new(),
        }
    }

    /// Add a DMX output interface.
    pub fn add_interface(&mut self, interface: Box<dyn DmxOutput>) {
        self.interfaces.push(interface);
    }

    /// Replace all interfaces.
    pub fn set_interfaces(&mut self, interfaces: Vec<Box<dyn DmxOutput>>) {
        self.interfaces = interfaces;
    }

    pub fn tick(&mut self) {
        // 1. Write all fixture channel values into the universe buffer.
        {
            let store = self.fixture_store.lock().unwrap();
            for fixture in &store.fixtures {
                if fixture.matches_universe(
                    self.universe.net,
                    self.universe.subnet,
                    self.universe.universe,
                ) {
                    fixture.write_to_universe(&mut self.universe);
                }
            }
        }

        // 2. Merge with overrides and update shared output.
        let mut shared = self.shared.lock().unwrap();
        let mut output = self.universe.channels;
        for i in 0..512 {
            if shared.overrides.active[i] {
                output[i] = shared.overrides.values[i];
            }
        }
        shared.output = output;
        let bypass = shared.bypass;
        let blackout = shared.blackout;
        drop(shared);

        // 3. Send to interfaces.
        if !bypass && !self.interfaces.is_empty() {
            if blackout {
                let mut blackout_uni = self.universe.clone();
                blackout_uni.blackout();
                for iface in &mut self.interfaces {
                    if let Err(e) = iface.send_universe(&blackout_uni) {
                        eprintln!("DMX send error: {}", e);
                    }
                }
            } else if self.universe.dirty {
                let mut send_uni = self.universe.clone();
                send_uni.channels = output;
                for iface in &mut self.interfaces {
                    if let Err(e) = iface.send_universe(&send_uni) {
                        eprintln!("DMX send error: {}", e);
                    }
                }
            }
        }

        self.universe.mark_clean();
    }
}
