use crate::color::Rgb;
use crate::engine::types::*;
use crate::objects::color_palette::{ColorPalette, PALETTE_SIZE};

pub struct PaletteSelectDisplay {
    pub group_ids: Vec<u32>,
    pub group_names: Vec<String>,
    pub current_group_index: usize,
    pub current_palette_index: usize,
    pub current_colors: [Rgb; PALETTE_SIZE],
    pub palette_count: usize,
}

pub struct PaletteSelectProcessNode {
    id: NodeId,
    group_index_in: f32,
    palette_index_in: f32,
    /// Group IDs (for save/restore).
    group_ids: Vec<u32>,
    /// Groups with their palettes (updated via load_data from widget).
    groups: Vec<(String, Vec<ColorPalette>)>,
    current_group: usize,
    current_palette: usize,
    current_colors: [Rgb; PALETTE_SIZE],
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl PaletteSelectProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            group_index_in: 0.0,
            palette_index_in: 0.0,
            group_ids: Vec::new(),
            groups: Vec::new(),
            current_group: 0,
            current_palette: 0,
            current_colors: [Rgb::BLACK; PALETTE_SIZE],
            inputs: vec![
                PortDef::new("group", PortType::Untyped),
                PortDef::new("palette", PortType::Untyped),
            ],
            outputs: vec![PortDef::new("palette", PortType::Palette)],
        }
    }
}

impl ProcessNode for PaletteSelectProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Palette Select" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        match pi { 0 => self.group_index_in = v, 1 => self.palette_index_in = v, _ => {} }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi { 0 => self.group_index_in, 1 => self.palette_index_in, _ => 0.0 }
    }

    fn process(&mut self) {
        if self.groups.is_empty() {
            self.current_colors = [Rgb::BLACK; PALETTE_SIZE];
            return;
        }

        let gi = (self.group_index_in.round() as i64).rem_euclid(self.groups.len() as i64) as usize;
        self.current_group = gi;

        let (_, palettes) = &self.groups[gi];
        if palettes.is_empty() {
            self.current_colors = [Rgb::BLACK; PALETTE_SIZE];
            self.current_palette = 0;
            return;
        }

        let pi = (self.palette_index_in.round() as i64).rem_euclid(palettes.len() as i64) as usize;
        self.current_palette = pi;
        self.current_colors = palettes[pi].colors;
    }

    fn read_output(&self, channel: usize) -> f32 {
        let slot = channel / 3;
        let component = channel % 3;
        if slot < PALETTE_SIZE {
            let c = self.current_colors[slot];
            match component { 0 => c.r, 1 => c.g, 2 => c.b, _ => 0.0 }
        } else { 0.0 }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        let groups: Vec<serde_json::Value> = self.groups.iter().map(|(name, palettes)| {
            serde_json::json!({
                "name": name,
                "palettes": palettes,
            })
        }).collect();
        Some(serde_json::json!({
            "group_ids": self.group_ids,
            "groups": groups,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(ids) = data.get("group_ids").and_then(|v| v.as_array()) {
            self.group_ids = ids.iter().filter_map(|v| v.as_u64().map(|n| n as u32)).collect();
        }
        if let Some(arr) = data.get("groups").and_then(|v| v.as_array()) {
            self.groups = arr.iter().filter_map(|entry| {
                let name = entry.get("name")?.as_str()?.to_string();
                let palettes: Vec<ColorPalette> = entry.get("palettes")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|s| serde_json::from_value(s.clone()).ok()).collect())
                    .unwrap_or_default();
                Some((name, palettes))
            }).collect();
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        let palette_count = self.groups.get(self.current_group)
            .map(|(_, s)| s.len()).unwrap_or(0);
        shared.display = Some(Box::new(PaletteSelectDisplay {
            group_ids: self.group_ids.clone(),
            group_names: self.groups.iter().map(|(n, _)| n.clone()).collect(),
            current_group_index: self.current_group,
            current_palette_index: self.current_palette,
            current_colors: self.current_colors,
            palette_count,
        }));
    }
}
