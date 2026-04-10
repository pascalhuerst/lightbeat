/// A single DMX universe (512 channels).
#[derive(Debug, Clone)]
pub struct DmxUniverse {
    pub net: u8,
    pub subnet: u8,
    pub universe: u8,
    pub channels: [u8; 512],
    /// Set to true when any channel value changed since last send.
    pub dirty: bool,
}

impl DmxUniverse {
    pub fn new(net: u8, subnet: u8, universe: u8) -> Self {
        Self {
            net,
            subnet,
            universe,
            channels: [0u8; 512],
            dirty: false,
        }
    }

    /// Update a channel value. Marks the universe dirty if the value changed.
    pub fn set(&mut self, channel: usize, value: u8) {
        if channel < 512 && self.channels[channel] != value {
            self.channels[channel] = value;
            self.dirty = true;
        }
    }

    /// Clear the dirty flag after sending.
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Zero all channels and mark dirty.
    pub fn blackout(&mut self) {
        self.channels = [0u8; 512];
        self.dirty = true;
    }
}
