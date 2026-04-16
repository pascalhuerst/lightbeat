use crate::color::Rgb;
use crate::dmx_io::SharedObjectStore;
use crate::engine::types::*;
use crate::objects::channel::ChannelKind;
use crate::objects::group::StripLayout;

/// Display state for the Bar pattern widget.
pub struct BarPatternDisplay {
    pub group_ids: Vec<u32>,
    pub group_names: Vec<String>,
    pub strip_count: usize,
}

/// Snapshot of the strip layout this node writes to (synced from main on group change).
#[derive(Clone)]
struct StripTarget {
    object_id: u32,
    logical_start: f32,
    logical_end: f32,
}

/// "Bar" — renders a moving bar onto LED strips in a group.
///
/// Inputs:
///   - position (Untyped, 0..1) — centre of the bar in the group's logical axis
///   - width    (Untyped, 0..1) — total width of the bar as fraction of axis
///   - color    (Color)         — bar color
///
/// The node writes pixel values directly to every LED strip object listed
/// in the group's `strip_layout`.
pub struct BarPatternProcessNode {
    id: NodeId,
    group_ids: Vec<u32>,
    group_names: Vec<String>,
    /// Flat list of strips across all selected groups.
    strips: Vec<StripTarget>,
    object_store: SharedObjectStore,
    /// 13 input channels: 1 (position) + 1 (width) + 3 (color)
    input_values: [f32; 5],
    inputs: Vec<PortDef>,
}

impl BarPatternProcessNode {
    pub fn new(id: NodeId, object_store: SharedObjectStore) -> Self {
        Self {
            id,
            group_ids: Vec::new(),
            group_names: Vec::new(),
            strips: Vec::new(),
            object_store,
            input_values: [0.0; 5],
            inputs: vec![
                PortDef::new("position", PortType::Untyped),
                PortDef::new("width", PortType::Untyped),
                PortDef::new("color", PortType::Color),
            ],
        }
    }
}

impl ProcessNode for BarPatternProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Bar" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, ch: usize, v: f32) {
        if ch < 5 { self.input_values[ch] = v; }
    }
    fn read_input(&self, ch: usize) -> f32 {
        if ch < 5 { self.input_values[ch] } else { 0.0 }
    }

    fn process(&mut self) {
        if self.strips.is_empty() { return; }

        let pos = self.input_values[0].clamp(0.0, 1.0);
        let width = self.input_values[1].clamp(0.0, 1.0);
        let color = Rgb::new(self.input_values[2], self.input_values[3], self.input_values[4]);
        let bar_lo = pos - width * 0.5;
        let bar_hi = pos + width * 0.5;

        let mut store = self.object_store.lock().unwrap();

        for strip in &self.strips {
            let obj = match store.objects.iter_mut().find(|o| o.id == strip.object_id) {
                Some(o) => o,
                None => continue,
            };
            // Find the LED strip channel.
            let ch = match obj.channels.iter_mut().find(|c| matches!(c.kind, ChannelKind::LedStrip { .. })) {
                Some(c) => c,
                None => continue,
            };
            let pixel_count = ch.pixel_count();
            if pixel_count == 0 { continue; }

            // Clear all pixels, then light the ones inside the bar.
            ch.clear_pixels();

            // Walk every LED on the strip; check whether its logical position
            // falls inside the bar. We compute the strip's per-LED logical coord
            // from the inverse of StripLayout::logical_to_led.
            let layout = StripLayout {
                object_id: strip.object_id,
                logical_start: strip.logical_start,
                logical_end: strip.logical_end,
            };
            let span = layout.logical_end - layout.logical_start;
            for px in 0..pixel_count {
                let t = px as f32 / (pixel_count as f32 - 1.0).max(1.0);
                let logical = layout.logical_start + t * span;
                if logical >= bar_lo && logical <= bar_hi {
                    ch.set_pixel(px, color);
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
            "strips": self.strips.iter().map(|s| serde_json::json!({
                "object_id": s.object_id,
                "logical_start": s.logical_start,
                "logical_end": s.logical_end,
            })).collect::<Vec<_>>(),
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        self.group_ids = data.get("group_ids")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_u64().map(|n| n as u32)).collect())
            .unwrap_or_default();
        self.group_names = data.get("group_names")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        self.strips = data.get("strips")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|entry| {
                Some(StripTarget {
                    object_id: entry.get("object_id")?.as_u64()? as u32,
                    logical_start: entry.get("logical_start")?.as_f64()? as f32,
                    logical_end: entry.get("logical_end")?.as_f64()? as f32,
                })
            }).collect())
            .unwrap_or_default();
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(BarPatternDisplay {
            group_ids: self.group_ids.clone(),
            group_names: self.group_names.clone(),
            strip_count: self.strips.len(),
        }));
    }
}
