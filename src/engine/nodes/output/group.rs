use crate::color::Rgb;
use crate::dmx_io::SharedObjectStore;
use crate::engine::types::*;
use crate::objects::channel::ChannelKind;
use crate::objects::group::{Group, GroupCapability};

/// Display state for group node widget.
pub struct GroupNodeDisplay {
    pub group_name: String,
    pub capabilities: Vec<GroupCapability>,
    pub object_count: usize,
}

pub struct GroupProcessNode {
    id: NodeId,
    group: Group,
    capabilities: Vec<GroupCapability>,
    object_store: SharedObjectStore,
    // Input values indexed by internal channel.
    // Layout: [dimmer] [color_r, color_g, color_b] [pos_pan, pos_tilt]
    // depending on which capabilities exist.
    input_values: Vec<f32>,
    inputs: Vec<PortDef>,
}

impl GroupProcessNode {
    pub fn new(id: NodeId, group: Group, capabilities: Vec<GroupCapability>, object_store: SharedObjectStore) -> Self {
        let inputs = Self::build_inputs(&capabilities);
        let num_channels = Self::total_input_channels(&capabilities);
        Self {
            id,
            group,
            capabilities,
            object_store,
            input_values: vec![0.0; num_channels],
            inputs,
        }
    }

    fn build_inputs(caps: &[GroupCapability]) -> Vec<PortDef> {
        let mut inputs = Vec::new();
        for cap in caps {
            match cap {
                GroupCapability::Dimmer => {
                    inputs.push(PortDef::new("dimmer", PortType::Untyped));
                }
                GroupCapability::Color => {
                    inputs.push(PortDef::new("color", PortType::Color));
                }
                GroupCapability::Position => {
                    inputs.push(PortDef::new("position", PortType::Position));
                }
            }
        }
        inputs
    }

    fn total_input_channels(caps: &[GroupCapability]) -> usize {
        caps.iter().map(|c| match c {
            GroupCapability::Dimmer => 1,
            GroupCapability::Color => 3,
            GroupCapability::Position => 2,
        }).sum()
    }

    /// Get the internal channel base index for a capability.
    fn cap_base_index(&self, cap: GroupCapability) -> Option<usize> {
        let mut base = 0;
        for c in &self.capabilities {
            if *c == cap { return Some(base); }
            base += match c {
                GroupCapability::Dimmer => 1,
                GroupCapability::Color => 3,
                GroupCapability::Position => 2,
            };
        }
        None
    }
}

impl ProcessNode for GroupProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Group" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, channel: usize, value: f32) {
        if channel < self.input_values.len() {
            self.input_values[channel] = value;
        }
    }

    fn read_input(&self, channel: usize) -> f32 {
        self.input_values.get(channel).copied().unwrap_or(0.0)
    }

    fn process(&mut self) {
        // Write input values to all member objects.
        let mut store = self.object_store.lock().unwrap();

        for oid in &self.group.object_ids {
            let obj = match store.objects.iter_mut().find(|o| o.id == *oid) {
                Some(o) => o,
                None => continue,
            };

            // Apply dimmer if this object has it and the group provides it.
            if let Some(base) = self.cap_base_index(GroupCapability::Dimmer) {
                if let Some(ch) = obj.channels.iter_mut().find(|c| matches!(c.kind, ChannelKind::Dimmer)) {
                    ch.set_dimmer(self.input_values[base]);
                }
            }

            // Apply color.
            if let Some(base) = self.cap_base_index(GroupCapability::Color) {
                if let Some(ch) = obj.channels.iter_mut().find(|c| matches!(c.kind, ChannelKind::Color { .. })) {
                    ch.set_color(Rgb::new(
                        self.input_values[base],
                        self.input_values[base + 1],
                        self.input_values[base + 2],
                    ));
                }
            }

            // Apply position.
            if let Some(base) = self.cap_base_index(GroupCapability::Position) {
                if let Some(ch) = obj.channels.iter_mut().find(|c| matches!(c.kind, ChannelKind::PanTilt { .. })) {
                    ch.set_pan_tilt(
                        self.input_values[base],
                        self.input_values[base + 1],
                    );
                }
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(GroupNodeDisplay {
            group_name: self.group.name.clone(),
            capabilities: self.capabilities.clone(),
            object_count: self.group.object_ids.len(),
        }));
    }
}
