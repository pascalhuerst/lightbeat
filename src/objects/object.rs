use super::channel::Channel;
use super::fixture::{DmxAddress, Fixture};
use super::universe::DmxUniverse;

/// An instance of a fixture — has a specific DMX address and interface assignment.
/// Owns a clone of the fixture's channels (with runtime values).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Object {
    pub id: u32,
    pub name: String,
    /// Which fixture template this is an instance of.
    pub fixture_id: u32,
    /// DMX address for this instance.
    pub address: DmxAddress,
    /// Interface ID this object sends through (0 = none/default).
    pub interface_id: u32,
    /// Runtime channel values (cloned from fixture template).
    pub channels: Vec<Channel>,
}

impl Object {
    /// Create a new object from a fixture template.
    pub fn new(id: u32, name: impl Into<String>, fixture: &Fixture, address: DmxAddress) -> Self {
        Self {
            id,
            name: name.into(),
            fixture_id: fixture.id,
            address,
            interface_id: 0,
            channels: fixture.channels.clone(),
        }
    }

    /// Write all channel values into a DMX buffer at this object's address.
    pub fn write_dmx(&self, buf: &mut [u8; 512]) {
        let base = self.address.base_offset();
        for ch in &self.channels {
            ch.write_dmx(buf, base);
        }
    }

    /// Write all channel values into a DmxUniverse, with dirty tracking.
    pub fn write_to_universe(&self, universe: &mut DmxUniverse) {
        let mut local = universe.channels;
        self.write_dmx(&mut local);
        for i in 0..512 {
            if local[i] != universe.channels[i] {
                universe.set(i, local[i]);
            }
        }
    }

    /// Check if this object's address matches a given universe.
    pub fn matches_universe(&self, net: u8, subnet: u8, universe: u8) -> bool {
        self.address.net == net
            && self.address.subnet == subnet
            && self.address.universe == universe
    }

    /// Total DMX channels this object occupies.
    pub fn dmx_footprint(&self) -> usize {
        self.channels
            .iter()
            .map(|c| c.offset as usize + c.kind.dmx_channel_count())
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgb;
    use crate::objects::channel::{Channel, ColorMode};

    fn test_fixture() -> Fixture {
        let mut f = Fixture::new(1, "Par");
        f.add_channel(Channel::dimmer("Dimmer"));
        f.add_channel(Channel::color("Color", ColorMode::Rgb));
        f.add_channel(Channel::pan_tilt("Pan/Tilt", false));
        f
    }

    #[test]
    fn write_dmx_layout() {
        let fixture = test_fixture();
        let mut obj = Object::new(1, "Par #1", &fixture, DmxAddress {
            start_channel: 10,
            ..Default::default()
        });

        obj.channels.iter_mut().find(|c| c.name == "Dimmer").unwrap().set_dimmer(1.0);
        obj.channels.iter_mut().find(|c| c.name == "Color").unwrap().set_color(Rgb::new(1.0, 0.0, 0.5));
        obj.channels.iter_mut().find(|c| c.name == "Pan/Tilt").unwrap().set_pan_tilt(0.5, 0.75);

        let mut buf = [0u8; 512];
        obj.write_dmx(&mut buf);

        assert_eq!(buf[9], 255);   // dimmer
        assert_eq!(buf[10], 255);  // R
        assert_eq!(buf[11], 0);    // G
        assert_eq!(buf[12], 128);  // B
        assert_eq!(buf[13], 128);  // pan
        assert_eq!(buf[14], 191);  // tilt
    }
}
