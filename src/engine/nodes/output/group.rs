use crate::color::{Gradient, Rgb};
use crate::dmx_io::SharedObjectStore;
use crate::engine::types::*;
use crate::objects::channel::ChannelKind;
use crate::objects::group::StripLayout;

/// Output mode for the Group Output node.
/// - `Flood`: writes a palette color + dimmer to every object each tick.
/// - `Triggered`: writes a gradient across a sub-range of the group's
///   objects only on the rising edge of a trigger input. The sampled
///   alpha per object is used to blend against the object's current color,
///   so stops with alpha < 1 produce soft edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupMode {
    Flood,
    Triggered,
}

impl GroupMode {
    pub fn to_index(self) -> usize { match self { GroupMode::Flood => 0, GroupMode::Triggered => 1 } }
    pub fn from_index(i: usize) -> Self { match i { 1 => GroupMode::Triggered, _ => GroupMode::Flood } }
}

pub const GROUP_MODE_NAMES: &[&str] = &["Flood", "Triggered"];

/// Input buffer size, sized for the largest mode (Triggered:
/// 1 trigger + 1 select + 1 width + 40 gradient = 43).
const INPUT_BUF_LEN: usize = 3 + GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS;

/// Display state for group node widget.
pub struct GroupNodeDisplay {
    pub group_ids: Vec<u32>,
    pub group_names: Vec<String>,
    pub object_count: usize,
    pub mode: GroupMode,
}

pub struct GroupProcessNode {
    id: NodeId,
    mode: GroupMode,
    /// Which group IDs this node targets.
    group_ids: Vec<u32>,
    /// Object IDs collected from all targeted groups.
    object_ids: Vec<u32>,
    /// Per-strip logical-axis mapping (from the group's StripLayout).
    /// In Triggered mode each strip pixel's position on the 0..1 group axis
    /// is derived from this, and the gradient is sampled per pixel.
    strip_layouts: Vec<StripLayout>,
    object_store: SharedObjectStore,
    inputs: Vec<PortDef>,
    input_values: [f32; INPUT_BUF_LEN],
    /// Previous trigger level for rising-edge detection.
    prev_trigger: f32,
    /// `[lo, hi]` range painted by the most recent trigger. The next trigger
    /// erases this region before writing the new range.
    last_written_range: Option<(f32, f32)>,
    /// True when the Flood-mode `dimmer` input port is wired. When not
    /// wired, the dimmer defaults to 1.0 so strip-only groups don't
    /// silently go black.
    dimmer_connected: bool,
    /// Group names for display.
    group_names: Vec<String>,
}

impl GroupProcessNode {
    pub fn new(id: NodeId, object_store: SharedObjectStore) -> Self {
        let mut node = Self {
            id,
            mode: GroupMode::Flood,
            group_ids: Vec::new(),
            object_ids: Vec::new(),
            strip_layouts: Vec::new(),
            object_store,
            inputs: Vec::new(),
            input_values: [0.0; INPUT_BUF_LEN],
            prev_trigger: 0.0,
            last_written_range: None,
            dimmer_connected: false,
            group_names: Vec::new(),
        };
        node.rebuild_inputs();
        node
    }

    fn rebuild_inputs(&mut self) {
        self.inputs = match self.mode {
            GroupMode::Flood => vec![
                PortDef::new("palette", PortType::Palette),
                PortDef::new("dimmer", PortType::Untyped),
            ],
            GroupMode::Triggered => vec![
                PortDef::new("trigger", PortType::Logic),
                PortDef::new("select", PortType::Untyped),
                PortDef::new("width", PortType::Untyped),
                PortDef::new("gradient", PortType::Gradient),
            ],
        };
        self.input_values = [0.0; INPUT_BUF_LEN];
        self.prev_trigger = 0.0;
        self.last_written_range = None;
    }

    fn reconfigure(&mut self, group_ids: Vec<u32>, group_names: Vec<String>,
                   object_ids: Vec<u32>, strip_layouts: Vec<StripLayout>) {
        self.group_ids = group_ids;
        self.group_names = group_names;
        self.object_ids = object_ids;
        self.strip_layouts = strip_layouts;
    }

    fn process_flood(&mut self) {
        let r = self.input_values[0];
        let g = self.input_values[1];
        let b = self.input_values[2];
        // Default to full brightness when the dimmer port isn't wired, so
        // a group of LED strips with only a palette input doesn't silently
        // go black.
        let dim = if self.dimmer_connected {
            self.input_values[12].clamp(0.0, 1.0)
        } else {
            1.0
        };
        let color = Rgb::new(r, g, b);

        let mut store = self.object_store.lock().unwrap();
        for oid in &self.object_ids {
            let obj = match store.objects.iter_mut().find(|o| o.id == *oid) {
                Some(o) => o,
                None => continue,
            };

            let has_dimmer_channel = obj.channels.iter()
                .any(|c| matches!(c.kind, ChannelKind::Dimmer));

            if has_dimmer_channel {
                // Dedicated dimmer channel exists — write it and leave the
                // colour channels at full intensity. The fixture's hardware
                // does the multiplication on the DMX side.
                for ch in obj.channels.iter_mut() {
                    if matches!(ch.kind, ChannelKind::Dimmer) {
                        ch.set_dimmer(dim);
                    }
                }
                write_object_color(obj, color);
            } else {
                // No dimmer channel (plain RGB fixtures, LED strips) —
                // pre-scale the colour. Visually identical to a hardware
                // dimmer, just done before the DMX encode.
                let scaled = Rgb::new(color.r * dim, color.g * dim, color.b * dim);
                write_object_color(obj, scaled);
            }
        }
    }

    fn process_triggered(&mut self) {
        let trigger = self.input_values[0];
        let select = self.input_values[1].clamp(0.0, 1.0);
        let width = self.input_values[2].clamp(0.0, 1.0);

        let trigger_edge = self.prev_trigger < 0.5 && trigger >= 0.5;
        self.prev_trigger = trigger;
        if !trigger_edge { return; }

        let lo = select;
        let hi = (select + width).min(1.0);
        if hi <= lo { return; }
        let span = hi - lo;

        let gradient = Gradient::from_channels(
            &self.input_values[3..3 + GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS],
        );

        // Each trigger erases the previous trigger's region before painting,
        // so a moving selection cleans up after itself automatically.
        let clear_range = self.last_written_range;
        self.last_written_range = Some((lo, hi));

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

                        // Erase pixels in the previously written range first.
                        if let Some((clo, chi)) = clear_range
                            && logical >= clo && logical <= chi {
                                ch.set_pixel(i, Rgb::BLACK);
                            }

                        if logical < lo || logical > hi { continue; }

                        let t = (logical - lo) / span;
                        let (sampled, alpha) = gradient.sample_with_alpha(t);
                        let cur = ch.pixel(i);
                        ch.set_pixel(i, cur.lerp(sampled, alpha));
                    }
                }
            }
        }
    }
}

/// Write a solid color to every output surface of the object: the Color
/// channel if present, and every pixel of any LedStrip channel.
fn write_object_color(obj: &mut crate::objects::object::Object, color: Rgb) {
    for ch in obj.channels.iter_mut() {
        match ch.kind {
            ChannelKind::Color { .. } => ch.set_color(color),
            ChannelKind::LedStrip { count, .. } => {
                for i in 0..count { ch.set_pixel(i, color); }
            }
            _ => {}
        }
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

    fn set_input_connections(&mut self, connected: &[bool]) {
        // Only Flood mode has a dimmer port (at logical input index 1).
        // In Triggered mode the dimmer is inherent to the gradient; the
        // flag is simply false.
        self.dimmer_connected = matches!(self.mode, GroupMode::Flood)
            && connected.get(1).copied().unwrap_or(false);
    }

    fn process(&mut self) {
        if self.object_ids.is_empty() { return; }
        match self.mode {
            GroupMode::Flood => self.process_flood(),
            GroupMode::Triggered => self.process_triggered(),
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Choice {
            name: "Mode".into(),
            value: self.mode.to_index(),
            options: GROUP_MODE_NAMES.iter().map(|s| s.to_string()).collect(),
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 {
            let new_mode = GroupMode::from_index(value.as_usize());
            if new_mode != self.mode {
                self.mode = new_mode;
                self.rebuild_inputs();
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
            mode: self.mode,
        }));
    }
}
