use crate::color::{Gradient, Rgb};
use crate::dmx_io::SharedObjectStore;
use crate::engine::types::*;
use crate::objects::channel::ChannelKind;
use crate::objects::group::StripLayout;

/// Same input buffer size as `GroupProcessNode` — 1 select + 1 width + 40
/// gradient channels.
const INPUT_BUF_LEN: usize = 2 + GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS;

/// Display state for the preview widget. Mirrors the strip layout order so
/// the widget can render one row per strip, LEDs as columns.
pub struct GroupPreviewDisplay {
    pub group_ids: Vec<u32>,
    pub group_names: Vec<String>,
    /// One pixel buffer per strip, in `strip_layouts` order. Empty strips
    /// (object missing or zero LEDs) appear as an empty `Vec`.
    pub strips: Vec<Vec<Rgb>>,
    /// Number of strips laid out per preview row — from the group's
    /// `preview_columns`. `1` stacks all strips vertically.
    pub preview_columns: usize,
}

pub struct GroupPreviewProcessNode {
    id: NodeId,
    group_ids: Vec<u32>,
    object_ids: Vec<u32>,
    strip_layouts: Vec<StripLayout>,
    object_store: SharedObjectStore,
    inputs: Vec<PortDef>,
    input_values: [f32; INPUT_BUF_LEN],
    group_names: Vec<String>,
    /// Strips-per-row hint from the configured group. Defaults to 1.
    preview_columns: usize,
    /// Per-strip pixel buffer computed each tick. Sized from the DMX store
    /// so the widget can draw dots without touching object state itself.
    strip_pixels: Vec<Vec<Rgb>>,
}

impl GroupPreviewProcessNode {
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
            group_names: Vec::new(),
            preview_columns: 1,
            strip_pixels: Vec::new(),
        }
    }

    fn reconfigure(&mut self, group_ids: Vec<u32>, group_names: Vec<String>,
                   object_ids: Vec<u32>, strip_layouts: Vec<StripLayout>,
                   preview_columns: usize) {
        self.group_ids = group_ids;
        self.group_names = group_names;
        self.object_ids = object_ids;
        self.strip_layouts = strip_layouts;
        self.preview_columns = preview_columns.max(1);
        self.strip_pixels.clear();
    }
}

impl ProcessNode for GroupPreviewProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Group Output Preview" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, channel: usize, value: f32) {
        if channel < INPUT_BUF_LEN { self.input_values[channel] = value; }
    }

    fn read_input(&self, channel: usize) -> f32 {
        if channel < INPUT_BUF_LEN { self.input_values[channel] } else { 0.0 }
    }

    fn process(&mut self) {
        if self.strip_layouts.is_empty() {
            self.strip_pixels.clear();
            return;
        }

        let select = self.input_values[0].clamp(0.0, 1.0);
        let width = self.input_values[1].clamp(0.0, 1.0);
        let lo = select;
        let hi = (select + width).min(1.0);
        let span = hi - lo;

        let gradient = Gradient::from_channels(
            &self.input_values[2..2 + GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS],
        );

        // Look up LED counts per configured strip from the DMX store. We
        // don't write to the store — this node only renders a preview.
        let store = self.object_store.lock().unwrap();

        self.strip_pixels.clear();
        for layout in &self.strip_layouts {
            let obj = store.objects.iter().find(|o| o.id == layout.object_id);
            let count = obj.and_then(|o| {
                o.channels.iter().find_map(|ch| {
                    if let ChannelKind::LedStrip { count, .. } = ch.kind { Some(count) } else { None }
                })
            }).unwrap_or(0);

            if count == 0 {
                self.strip_pixels.push(Vec::new());
                continue;
            }

            let mut pixels = vec![Rgb::BLACK; count];
            if hi > lo {
                for i in 0..count {
                    let t_in_strip = if count > 1 {
                        i as f32 / (count - 1) as f32
                    } else {
                        0.5
                    };
                    let logical = layout.logical_start
                        + t_in_strip * (layout.logical_end - layout.logical_start);
                    if logical < lo || logical > hi { continue; }
                    let t = if span > 0.0 { (logical - lo) / span } else { 0.0 };
                    let (sampled, alpha) = gradient.sample_with_alpha(t);
                    // No composite with live DMX state — preview shows this
                    // node's contribution against a black background.
                    pixels[i] = Rgb::BLACK.lerp(sampled, alpha);
                }
            }
            self.strip_pixels.push(pixels);
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
            "preview_columns": self.preview_columns,
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
        let preview_columns = data.get("preview_columns")
            .and_then(|v| v.as_u64()).map(|n| n as usize).unwrap_or(1);

        self.reconfigure(group_ids, group_names, object_ids, strip_layouts, preview_columns);
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(GroupPreviewDisplay {
            group_ids: self.group_ids.clone(),
            group_names: self.group_names.clone(),
            strips: self.strip_pixels.clone(),
            preview_columns: self.preview_columns,
        }));
    }
}
