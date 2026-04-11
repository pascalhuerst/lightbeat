use std::sync::{Arc, Mutex};

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
    /// Bypass: stop sending DMX entirely (output still computed for display).
    pub bypass: bool,
    /// Blackout: force all channels to 0 on the wire (display shows actual values dimmed).
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

/// Manages DMX universe buffers, merges fixture data with overrides,
/// and sends to interfaces.
pub struct DmxOutputManager {
    pub universe: DmxUniverse,
    shared: SharedDmxState,
}

impl DmxOutputManager {
    pub fn new(shared: SharedDmxState) -> Self {
        Self {
            universe: DmxUniverse::new(0, 0, 0),
            shared,
        }
    }

    pub fn tick(&mut self) {
        let mut shared = self.shared.lock().unwrap();

        // Compute output: fixture data + overrides.
        let mut output = self.universe.channels;
        for i in 0..512 {
            if shared.overrides.active[i] {
                output[i] = shared.overrides.values[i];
            }
        }
        shared.output = output;

        // Determine what actually gets sent on the wire.
        if shared.bypass {
            // Don't send anything.
        } else if shared.blackout {
            // Send all zeros.
            // TODO: send [0u8; 512] to interfaces.
        } else {
            // TODO: send `output` to interfaces if dirty.
        }

        self.universe.mark_clean();
    }

    pub fn write_channel(&mut self, channel: usize, value: u8) {
        self.universe.set(channel, value);
    }
}
