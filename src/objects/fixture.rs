use super::channel::Channel;
use super::universe::DmxUniverse;

/// Identifies which DMX universe and address a fixture outputs to.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DmxAddress {
    pub net: u8,
    pub subnet: u8,
    pub universe: u8,
    /// 1-based DMX start address (1–512).
    pub start_channel: u16,
}

impl Default for DmxAddress {
    fn default() -> Self {
        Self {
            net: 0,
            subnet: 0,
            universe: 0,
            start_channel: 1,
        }
    }
}

impl DmxAddress {
    /// Convert the 1-based start_channel to a 0-based offset.
    pub fn base_offset(&self) -> u16 {
        self.start_channel.saturating_sub(1)
    }
}

/// A lighting fixture. Owns a set of channels and a DMX output address.
///
/// Channels are laid out sequentially from the fixture's `start_channel`.
/// Each channel has an `offset` relative to that base, so the absolute DMX
/// address of a channel is `start_channel + channel.offset - 1` (0-based).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Fixture {
    pub id: u32,
    pub name: String,
    pub channels: Vec<Channel>,
    pub address: DmxAddress,
}

impl Fixture {
    pub fn new(id: u32, name: impl Into<String>, address: DmxAddress) -> Self {
        Self {
            id,
            name: name.into(),
            channels: Vec::new(),
            address,
        }
    }

    /// Add a channel to this fixture. The channel's offset is auto-assigned
    /// based on the current footprint. Returns `&mut Self` for chaining.
    pub fn add_channel(&mut self, mut channel: Channel) -> &mut Self {
        channel.offset = self.dmx_footprint() as u16;
        self.channels.push(channel);
        self
    }

    /// Recalculate all channel offsets based on sequential order.
    pub fn recalc_offsets(&mut self) {
        let mut offset: u16 = 0;
        for ch in &mut self.channels {
            ch.offset = offset;
            offset += ch.kind.dmx_channel_count() as u16;
        }
    }

    /// Total number of DMX channels this fixture occupies.
    pub fn dmx_footprint(&self) -> usize {
        self.channels
            .iter()
            .map(|c| c.offset as usize + c.kind.dmx_channel_count())
            .max()
            .unwrap_or(0)
    }

    /// Get a channel by name.
    pub fn channel(&self, name: &str) -> Option<&Channel> {
        self.channels.iter().find(|c| c.name == name)
    }

    /// Get a mutable channel by name.
    pub fn channel_mut(&mut self, name: &str) -> Option<&mut Channel> {
        self.channels.iter_mut().find(|c| c.name == name)
    }

    /// Write all channel values into a DMX universe buffer.
    pub fn write_dmx(&self, buf: &mut [u8; 512]) {
        let base = self.address.base_offset();
        for ch in &self.channels {
            ch.write_dmx(buf, base);
        }
    }

    /// Write all channel values into a `DmxUniverse`, marking it dirty.
    pub fn write_to_universe(&self, universe: &mut DmxUniverse) {
        let mut local = universe.channels;
        self.write_dmx(&mut local);

        // Only update changed channels to get correct dirty tracking.
        for i in 0..512 {
            if local[i] != universe.channels[i] {
                universe.set(i, local[i]);
            }
        }
    }

    /// Check if this fixture's address matches a given universe.
    pub fn matches_universe(&self, net: u8, subnet: u8, universe: u8) -> bool {
        self.address.net == net
            && self.address.subnet == subnet
            && self.address.universe == universe
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgb;
    use crate::objects::channel::{Channel, ColorMode};

    fn test_fixture() -> Fixture {
        let mut f = Fixture::new(1, "Par", DmxAddress {
            start_channel: 10,
            ..Default::default()
        });
        f.add_channel(Channel::dimmer("Dimmer"));
        f.add_channel(Channel::color("Color", ColorMode::Rgb));
        f.add_channel(Channel::pan_tilt("Pan/Tilt", false));
        f
    }

    #[test]
    fn dmx_footprint() {
        let f = test_fixture();
        // PanTilt at offset 4, 2 channels → 6 total
        assert_eq!(f.dmx_footprint(), 6);
    }

    #[test]
    fn write_dmx_layout() {
        let mut f = test_fixture();
        f.channel_mut("Dimmer").unwrap().set_dimmer(1.0);
        f.channel_mut("Color").unwrap().set_color(Rgb::new(1.0, 0.0, 0.5));
        f.channel_mut("Pan/Tilt").unwrap().set_pan_tilt(0.5, 0.75);

        let mut buf = [0u8; 512];
        f.write_dmx(&mut buf);

        // base=9 (1-based 10 → 0-based 9)
        assert_eq!(buf[9], 255);        // dimmer at offset 0
        assert_eq!(buf[10], 255);       // R at offset 1
        assert_eq!(buf[11], 0);         // G
        assert_eq!(buf[12], 128);       // B (0.5 → 128)
        assert_eq!(buf[13], 128);       // pan (0.5 → 128)
        assert_eq!(buf[14], 191);       // tilt (0.75 → 191)
    }

    #[test]
    fn channel_lookup() {
        let f = test_fixture();
        assert!(f.channel("Dimmer").is_some());
        assert!(f.channel("Color").is_some());
        assert!(f.channel("Nonexistent").is_none());
    }
}
