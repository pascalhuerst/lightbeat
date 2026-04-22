use serde::{Deserialize, Serialize};

use super::channel::ChannelKind;
use super::object::Object;

/// Layout entry for an LED strip object within a group.
/// Defines where the strip's first and last LED map onto the group's
/// normalized 0..1 logical axis. If `logical_end < logical_start`,
/// the strip is logically reversed (LED 0 at the high end).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StripLayout {
    pub object_id: u32,
    pub logical_start: f32,
    pub logical_end: f32,
}

impl StripLayout {
    /// Convert a logical position (0..1) into a fractional LED index for this strip.
    /// Returns `None` if the position is outside the strip's logical range.
    pub fn logical_to_led(&self, logical: f32, pixel_count: usize) -> Option<f32> {
        if pixel_count == 0 { return None; }
        let lo = self.logical_start.min(self.logical_end);
        let hi = self.logical_start.max(self.logical_end);
        if logical < lo || logical > hi { return None; }

        // Normalize within the strip's range.
        let span = self.logical_end - self.logical_start;
        if span.abs() < 1e-6 { return None; }
        let t = (logical - self.logical_start) / span; // 0..1, accounts for reversal
        Some(t * (pixel_count as f32 - 1.0))
    }
}

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: u32,
    pub name: String,
    /// IDs of objects in this group.
    pub object_ids: Vec<u32>,
    /// Optional per-LED-strip layout for strip-aware pattern nodes.
    /// Strips not listed here are still in the group but have no layout role.
    #[serde(default)]
    pub strip_layout: Vec<StripLayout>,
}

impl Group {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            object_ids: Vec::new(),
            strip_layout: Vec::new(),
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
                        ChannelKind::LedStrip { .. } => continue,
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
