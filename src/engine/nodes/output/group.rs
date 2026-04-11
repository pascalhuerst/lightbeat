use crate::color::Rgb;
use crate::dmx_io::SharedObjectStore;
use crate::engine::types::*;
use crate::objects::channel::ChannelKind;

/// Display state for group node widget.
pub struct GroupNodeDisplay {
    pub group_ids: Vec<u32>,
    pub group_names: Vec<String>,
    pub object_count: usize,
}

pub struct GroupProcessNode {
    id: NodeId,
    /// Which group IDs this node targets.
    group_ids: Vec<u32>,
    /// Object IDs collected from all targeted groups.
    object_ids: Vec<u32>,
    object_store: SharedObjectStore,
    /// ColorStack input: 12 floats (4 × RGB) + 1 dimmer.
    input_values: [f32; 13],
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
            object_store,
            input_values: [0.0; 13],
            inputs: vec![
                PortDef::new("palette", PortType::ColorStack),
                PortDef::new("dimmer", PortType::Untyped),
            ],
            group_names: Vec::new(),
        }
    }

    fn reconfigure(&mut self, group_ids: Vec<u32>, group_names: Vec<String>,
                   object_ids: Vec<u32>) {
        self.group_ids = group_ids;
        self.group_names = group_names;
        self.object_ids = object_ids;
    }
}

impl ProcessNode for GroupProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Group Output" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, channel: usize, value: f32) {
        if channel < 13 { self.input_values[channel] = value; }
    }

    fn read_input(&self, channel: usize) -> f32 {
        if channel < 13 { self.input_values[channel] } else { 0.0 }
    }

    fn process(&mut self) {
        if self.object_ids.is_empty() { return; }

        // For now, apply the primary color (first in palette) to all fixtures.
        // Palette layout: [R0,G0,B0, R1,G1,B1, R2,G2,B2, R3,G3,B3]
        // Dimmer is at channel 12 (after the 12-channel ColorStack).
        let r = self.input_values[0];
        let g = self.input_values[1];
        let b = self.input_values[2];
        let dim = self.input_values[12];

        let mut store = self.object_store.lock().unwrap();

        for oid in &self.object_ids {
            let obj = match store.objects.iter_mut().find(|o| o.id == *oid) {
                Some(o) => o,
                None => continue,
            };

            // Apply dimmer.
            if let Some(ch) = obj.channels.iter_mut().find(|c| matches!(c.kind, ChannelKind::Dimmer)) {
                ch.set_dimmer(dim);
            }

            // Apply color.
            if let Some(ch) = obj.channels.iter_mut().find(|c| matches!(c.kind, ChannelKind::Color { .. })) {
                ch.set_color(Rgb::new(r, g, b));
            }
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        if self.group_ids.is_empty() {
            return None;
        }
        Some(serde_json::json!({
            "group_ids": self.group_ids,
            "group_names": self.group_names,
            "object_ids": self.object_ids,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
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

        self.reconfigure(group_ids, group_names, object_ids);
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(GroupNodeDisplay {
            group_ids: self.group_ids.clone(),
            group_names: self.group_names.clone(),
            object_count: self.object_ids.len(),
        }));
    }
}
