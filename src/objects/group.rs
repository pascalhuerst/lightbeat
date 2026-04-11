use serde::{Deserialize, Serialize};

use super::channel::ChannelKind;
use super::fixture::Fixture;

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

/// A group of fixtures that can be controlled together.
/// Capabilities are the UNION of all member fixtures' channel types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: u32,
    pub name: String,
    /// IDs of fixtures in this group.
    pub fixture_ids: Vec<u32>,
}

impl Group {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            fixture_ids: Vec::new(),
        }
    }

    /// Compute the union of capabilities from all member fixtures.
    pub fn capabilities(&self, fixtures: &[Fixture]) -> Vec<GroupCapability> {
        let mut caps = Vec::new();

        for fid in &self.fixture_ids {
            if let Some(fixture) = fixtures.iter().find(|f| f.id == *fid) {
                for ch in &fixture.channels {
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

        // Sort for consistent ordering: Dimmer, Color, Position.
        caps.sort_by_key(|c| match c {
            GroupCapability::Dimmer => 0,
            GroupCapability::Color => 1,
            GroupCapability::Position => 2,
        });

        caps
    }

    /// Check if a fixture supports a capability.
    pub fn fixture_has_capability(fixture: &Fixture, cap: GroupCapability) -> bool {
        fixture.channels.iter().any(|ch| match (&ch.kind, cap) {
            (ChannelKind::Dimmer, GroupCapability::Dimmer) => true,
            (ChannelKind::Color { .. }, GroupCapability::Color) => true,
            (ChannelKind::PanTilt { .. }, GroupCapability::Position) => true,
            _ => false,
        })
    }
}
