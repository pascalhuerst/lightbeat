use crate::color::{Gradient, Rgb};
use crate::dmx_io::SharedObjectStore;
use crate::engine::types::*;
use crate::objects::channel::ChannelKind;
use crate::objects::group::StripLayout;

/// Input buffer size: 1 select + 1 width + 40 gradient = 42.
const INPUT_BUF_LEN: usize = 2 + GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS;

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
    /// Per-strip logical-axis mapping (from the group's StripLayout). Each
    /// strip pixel's position on the 0..1 group axis is derived from this,
    /// and the gradient is sampled per pixel.
    strip_layouts: Vec<StripLayout>,
    object_store: SharedObjectStore,
    inputs: Vec<PortDef>,
    input_values: [f32; INPUT_BUF_LEN],
    /// `[lo, hi]` range painted on the previous tick. Pixels that were in
    /// the previous range but aren't in the current range are cleared, so
    /// a moving window leaves no trail.
    last_written_range: Option<(f32, f32)>,
    /// Group names for display.
    group_names: Vec<String>,
}

impl GroupProcessNode {
    pub fn new(id: NodeId, object_store: SharedObjectStore) -> Self {
        Self {
            id,
            group_ids: Vec::new(),
            object_ids: Vec::new(),
            strip_layouts: Vec::new(),
            object_store,
            inputs: vec![
                PortDef::new("select", PortType::Untyped),
                PortDef::new("width", PortType::Untyped),
                PortDef::new("gradient", PortType::Gradient),
            ],
            input_values: [0.0; INPUT_BUF_LEN],
            last_written_range: None,
            group_names: Vec::new(),
        }
    }

    fn reconfigure(&mut self, group_ids: Vec<u32>, group_names: Vec<String>,
                   object_ids: Vec<u32>, strip_layouts: Vec<StripLayout>) {
        self.group_ids = group_ids;
        self.group_names = group_names;
        self.object_ids = object_ids;
        self.strip_layouts = strip_layouts;
        self.last_written_range = None;
    }
}

impl ProcessNode for GroupProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Group Output" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, channel: usize, value: f32) {
        if channel < INPUT_BUF_LEN { self.input_values[channel] = value; }
    }

    fn read_input(&self, channel: usize) -> f32 {
        if channel < INPUT_BUF_LEN { self.input_values[channel] } else { 0.0 }
    }

    fn process(&mut self) {
        if self.object_ids.is_empty() { return; }

        let select = self.input_values[0].clamp(0.0, 1.0);
        let width = self.input_values[1].clamp(0.0, 1.0);
        let lo = select;
        let hi = (select + width).min(1.0);
        let span = hi - lo;

        let gradient = Gradient::from_channels(
            &self.input_values[2..2 + GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS],
        );

        // The previous tick's range — pixels that were in it but aren't in
        // `[lo, hi]` get cleared so a moving window doesn't smear.
        let clear_range = self.last_written_range;
        let active_range = if hi > lo { Some((lo, hi)) } else { None };
        self.last_written_range = active_range;

        let mut store = self.object_store.lock().unwrap();

        for layout in &self.strip_layouts {
            let obj = match store.objects.iter_mut().find(|o| o.id == layout.object_id) {
                Some(o) => o,
                None => continue,
            };
            for ch in obj.channels.iter_mut() {
                if let ChannelKind::LedStrip { count, .. } = ch.kind {
                    if count == 0 { continue; }
                    for i in 0..count {
                        let t_in_strip = if count > 1 {
                            i as f32 / (count - 1) as f32
                        } else {
                            0.5
                        };
                        let logical = layout.logical_start
                            + t_in_strip * (layout.logical_end - layout.logical_start);

                        let in_current = active_range
                            .is_some_and(|(lo, hi)| logical >= lo && logical <= hi);
                        let in_previous = clear_range
                            .is_some_and(|(lo, hi)| logical >= lo && logical <= hi);

                        // Was lit last tick but not this one → clear.
                        if in_previous && !in_current {
                            ch.set_pixel(i, Rgb::BLACK);
                            continue;
                        }
                        if !in_current { continue; }

                        let (lo, _hi) = active_range.unwrap();
                        let t = if span > 0.0 { (logical - lo) / span } else { 0.0 };
                        let (sampled, alpha) = gradient.sample_with_alpha(t);
                        let cur = ch.pixel(i);
                        ch.set_pixel(i, cur.lerp(sampled, alpha));
                    }
                }
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
        let strip_layouts: Vec<StripLayout> = data.get("strip_layouts")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| {
                Some(StripLayout {
                    object_id: v.get("object_id").and_then(|x| x.as_u64())? as u32,
                    logical_start: v.get("logical_start").and_then(|x| x.as_f64())? as f32,
                    logical_end: v.get("logical_end").and_then(|x| x.as_f64())? as f32,
                })
            }).collect())
            .unwrap_or_default();

        self.reconfigure(group_ids, group_names, object_ids, strip_layouts);
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(GroupNodeDisplay {
            group_ids: self.group_ids.clone(),
            group_names: self.group_names.clone(),
            object_count: self.object_ids.len(),
        }));
    }
}
