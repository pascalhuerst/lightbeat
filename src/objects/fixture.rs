use super::channel::Channel;

/// A fixture template — defines the channel layout for a type of light.
/// Does NOT have an address. Instances (Objects) reference this.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fixture {
    pub id: u32,
    pub name: String,
    pub channels: Vec<Channel>,
}

impl Fixture {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            channels: Vec::new(),
        }
    }

    pub fn add_channel(&mut self, mut channel: Channel) -> &mut Self {
        channel.offset = self.dmx_footprint() as u16;
        self.channels.push(channel);
        self
    }

    pub fn recalc_offsets(&mut self) {
        let mut offset: u16 = 0;
        for ch in &mut self.channels {
            ch.offset = offset;
            offset += ch.kind.dmx_channel_count() as u16;
        }
    }

    pub fn dmx_footprint(&self) -> usize {
        self.channels
            .iter()
            .map(|c| c.offset as usize + c.kind.dmx_channel_count())
            .max()
            .unwrap_or(0)
    }

    pub fn channel(&self, name: &str) -> Option<&Channel> {
        self.channels.iter().find(|c| c.name == name)
    }

    pub fn channel_mut(&mut self, name: &str) -> Option<&mut Channel> {
        self.channels.iter_mut().find(|c| c.name == name)
    }
}

/// DMX address for an object instance.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DmxAddress {
    pub net: u8,
    pub subnet: u8,
    pub universe: u8,
    /// 1-based DMX start address (1–512).
    pub start_channel: u16,
}

impl Default for DmxAddress {
    fn default() -> Self {
        Self { net: 0, subnet: 0, universe: 0, start_channel: 1 }
    }
}

impl DmxAddress {
    pub fn base_offset(&self) -> u16 {
        self.start_channel.saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::channel::{Channel, ColorMode};

    #[test]
    fn dmx_footprint() {
        let mut f = Fixture::new(1, "Par");
        f.add_channel(Channel::dimmer("Dimmer"));
        f.add_channel(Channel::color("Color", ColorMode::Rgb));
        f.add_channel(Channel::pan_tilt("Pan/Tilt", false));
        assert_eq!(f.dmx_footprint(), 6);
    }

    #[test]
    fn channel_lookup() {
        let mut f = Fixture::new(1, "Par");
        f.add_channel(Channel::dimmer("Dimmer"));
        f.add_channel(Channel::color("Color", ColorMode::Rgb));
        assert!(f.channel("Dimmer").is_some());
        assert!(f.channel("Color").is_some());
        assert!(f.channel("Nonexistent").is_none());
    }
}
