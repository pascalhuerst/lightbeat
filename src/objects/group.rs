use serde::{Deserialize, Serialize};

use super::channel::ChannelKind;
use super::fixture::Fixture;
use super::object::Object;

/// What capabilities a group exposes as inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GroupCapability {
    Dimmer,
    Color,
    Position,
}

impl GroupCapability {
    pub fn label(&self) -> &'static str {
        match self {
            GroupCapability::Dimmer => "Dimmer",
            GroupCapability::Color => "Color",
            GroupCapability::Position => "Position",
        }
    }
}

/// A group of objects that can be controlled together.
/// Capabilities are the UNION of all member objects' channel types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: u32,
    pub name: String,
    /// IDs of objects in this group.
    pub object_ids: Vec<u32>,
}

impl Group {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            object_ids: Vec::new(),
        }
    }

    /// Compute the union of capabilities from all member objects.
    pub fn capabilities(&self, objects: &[Object]) -> Vec<GroupCapability> {
        let mut caps = Vec::new();

        for oid in &self.object_ids {
            if let Some(obj) = objects.iter().find(|o| o.id == *oid) {
                for ch in &obj.channels {
                    let cap = match &ch.kind {
                        ChannelKind::Dimmer => GroupCapability::Dimmer,
                        ChannelKind::Color { .. } => GroupCapability::Color,
                        ChannelKind::PanTilt { .. } => GroupCapability::Position,
                        ChannelKind::Raw { .. } => continue,
                    };
                    if !caps.contains(&cap) {
                        caps.push(cap);
                    }
                }
            }
        }

        caps.sort_by_key(|c| match c {
            GroupCapability::Dimmer => 0,
            GroupCapability::Color => 1,
            GroupCapability::Position => 2,
        });

        caps
    }
}
