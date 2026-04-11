use crate::color::Rgb;
use crate::dmx_io::SharedObjectStore;
use crate::engine::types::*;
use crate::objects::channel::ChannelKind;
use crate::objects::group::GroupCapability;

/// Display state for group node widget.
pub struct GroupNodeDisplay {
    pub group_ids: Vec<u32>,
    pub group_names: Vec<String>,
    pub capabilities: Vec<GroupCapability>,
    pub object_count: usize,
}

pub struct GroupProcessNode {
    id: NodeId,
    /// Which group IDs this node targets.
    group_ids: Vec<u32>,
    /// Object IDs collected from all targeted groups.
    object_ids: Vec<u32>,
    capabilities: Vec<GroupCapability>,
    object_store: SharedObjectStore,
    input_values: Vec<f32>,
    inputs: Vec<PortDef>,
    /// Group names for display.
    group_names: Vec<String>,
}

impl GroupProcessNode {
    pub fn new(id: NodeId, object_store: SharedObjectStore) -> Self {
        Self {
            id,
            group_ids: Vec::new(),
            object_ids: Vec::new(),
            capabilities: Vec::new(),
            object_store,
            input_values: Vec::new(),
            inputs: Vec::new(),
            group_names: Vec::new(),
        }
    }

    fn build_inputs(caps: &[GroupCapability]) -> Vec<PortDef> {
        caps.iter().map(|cap| match cap {
            GroupCapability::Dimmer => PortDef::new("dimmer", PortType::Untyped),
            GroupCapability::Color => PortDef::new("color", PortType::Color),
            GroupCapability::Position => PortDef::new("position", PortType::Position),
        }).collect()
    }

    fn total_channels(caps: &[GroupCapability]) -> usize {
        caps.iter().map(|c| match c {
            GroupCapability::Dimmer => 1,
            GroupCapability::Color => 3,
            GroupCapability::Position => 2,
        }).sum()
    }

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

    /// Reconfigure based on group selection. Called via set_param.
    fn reconfigure(&mut self, group_ids: Vec<u32>, group_names: Vec<String>,
                   object_ids: Vec<u32>, capabilities: Vec<GroupCapability>) {
        self.group_ids = group_ids;
        self.group_names = group_names;
        self.object_ids = object_ids;
        self.capabilities = capabilities.clone();
        self.inputs = Self::build_inputs(&capabilities);
        self.input_values = vec![0.0; Self::total_channels(&capabilities)];
    }
}

impl ProcessNode for GroupProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Group Output" }
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
        if self.object_ids.is_empty() { return; }

        let mut store = self.object_store.lock().unwrap();

        for oid in &self.object_ids {
            let obj = match store.objects.iter_mut().find(|o| o.id == *oid) {
                Some(o) => o,
                None => continue,
            };

            if let Some(base) = self.cap_base_index(GroupCapability::Dimmer) {
                if let Some(ch) = obj.channels.iter_mut().find(|c| matches!(c.kind, ChannelKind::Dimmer)) {
                    ch.set_dimmer(self.input_values[base]);
                }
            }

            if let Some(base) = self.cap_base_index(GroupCapability::Color) {
                if let Some(ch) = obj.channels.iter_mut().find(|c| matches!(c.kind, ChannelKind::Color { .. })) {
                    ch.set_color(Rgb::new(
                        self.input_values[base],
                        self.input_values[base + 1],
                        self.input_values[base + 2],
                    ));
                }
            }

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

    fn set_param(&mut self, index: usize, value: ParamValue) {
        // Params are sent as JSON config via index 0.
        if index == 0 {
            if let ParamValue::Choice(v) = value {
                // Unused, but could toggle individual groups.
                let _ = v;
            }
        }
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        // Parse the reconfiguration JSON from the widget.
        let group_ids = data.get("group_ids")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_u64().map(|n| n as u32)).collect())
            .unwrap_or_default();
        let group_names = data.get("group_names")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let object_ids = data.get("object_ids")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_u64().map(|n| n as u32)).collect())
            .unwrap_or_default();
        let capabilities = data.get("capabilities")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().and_then(|s| match s {
                "Dimmer" => Some(GroupCapability::Dimmer),
                "Color" => Some(GroupCapability::Color),
                "Position" => Some(GroupCapability::Position),
                _ => None,
            })).collect())
            .unwrap_or_default();

        self.reconfigure(group_ids, group_names, object_ids, capabilities);
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(GroupNodeDisplay {
            group_ids: self.group_ids.clone(),
            group_names: self.group_names.clone(),
            capabilities: self.capabilities.clone(),
            object_count: self.object_ids.len(),
        }));
    }
}
