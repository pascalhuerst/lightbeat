use crate::engine::types::*;
use crate::objects::fixture::{DmxAddress, Fixture};
use crate::objects::channel::{Channel, ChannelKind, ColorMode};

/// Display state for the UI.
pub struct FixtureDisplay {
    pub fixture: Fixture,
}

pub struct FixtureProcessNode {
    id: NodeId,
    fixture: Fixture,
}

impl FixtureProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mut fixture = Fixture::new(1, "New Fixture", DmxAddress::default());
        fixture.add_channel(Channel::dimmer("Dimmer"));
        fixture.add_channel(Channel::color("Color", ColorMode::Rgb));

        Self { id, fixture }
    }
}

impl ProcessNode for FixtureProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Fixture" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &[] }
    fn process(&mut self) {}

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Int {
                name: "DMX Address".into(),
                value: self.fixture.address.start_channel as i64,
                min: 1,
                max: 512,
            },
            ParamDef::Int {
                name: "Universe".into(),
                value: self.fixture.address.universe as i64,
                min: 0,
                max: 15,
            },
            ParamDef::Int {
                name: "Subnet".into(),
                value: self.fixture.address.subnet as i64,
                min: 0,
                max: 15,
            },
            ParamDef::Int {
                name: "Net".into(),
                value: self.fixture.address.net as i64,
                min: 0,
                max: 127,
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match (index, value) {
            (0, ParamValue::Int(v)) => self.fixture.address.start_channel = v as u16,
            (1, ParamValue::Int(v)) => self.fixture.address.universe = v as u8,
            (2, ParamValue::Int(v)) => self.fixture.address.subnet = v as u8,
            (3, ParamValue::Int(v)) => self.fixture.address.net = v as u8,
            _ => {}
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.fixture).ok()
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Ok(f) = serde_json::from_value(data.clone()) {
            self.fixture = f;
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(FixtureDisplay {
            fixture: self.fixture.clone(),
        }));
    }
}
