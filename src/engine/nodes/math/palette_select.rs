use crate::color::Rgb;
use crate::engine::nodes::math::multiplex::SelectMode;
use crate::engine::types::*;
use crate::objects::color_palette::{ColorPalette, PALETTE_SIZE};

pub struct PaletteSelectDisplay {
    pub group_ids: Vec<u32>,
    pub group_names: Vec<String>,
    pub current_group_index: usize,
    pub current_palette_index: usize,
    pub current_colors: [Rgb; PALETTE_SIZE],
    pub palette_count: usize,
    pub select_mode: SelectMode,
}

pub struct PaletteSelectProcessNode {
    id: NodeId,
    group_index_in: f32,
    palette_index_in: f32,
    select_mode: SelectMode,
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
        let mut node = Self {
            id,
            group_index_in: 0.0,
            palette_index_in: 0.0,
            select_mode: SelectMode::Index,
            group_ids: Vec::new(),
            groups: Vec::new(),
            current_group: 0,
            current_palette: 0,
            current_colors: [Rgb::BLACK; PALETTE_SIZE],
            inputs: Vec::new(),
            outputs: vec![PortDef::new("palette", PortType::Palette)],
        };
        node.rebuild_ports();
        node
    }

    fn rebuild_ports(&mut self) {
        let pt = self.select_mode.select_port_type();
        self.inputs = vec![
            PortDef::new("group", pt),
            PortDef::new("palette", pt),
        ];
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

        let gi = resolve_index(self.group_index_in, self.select_mode, self.groups.len());
        self.current_group = gi;

        let (_, palettes) = &self.groups[gi];
        if palettes.is_empty() {
            self.current_colors = [Rgb::BLACK; PALETTE_SIZE];
            self.current_palette = 0;
            return;
        }

        let pi = resolve_index(self.palette_index_in, self.select_mode, palettes.len());
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

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Choice {
            name: "Select Mode".into(),
            value: self.select_mode.to_index(),
            options: SelectMode::ALL.iter().map(|m| m.label().to_string()).collect(),
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 {
            let new_mode = SelectMode::from_index(value.as_usize());
            if new_mode != self.select_mode {
                self.select_mode = new_mode;
                self.rebuild_ports();
            }
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
            select_mode: self.select_mode,
        }));
    }
}

/// Resolve a `select` input value (raw float) into a slot index `[0, n)`
/// under the given `mode`. `Index` rounds + wraps; `Phase` maps 0..1
/// onto [0, n) and wraps elsewhere.
fn resolve_index(sel: f32, mode: SelectMode, n: usize) -> usize {
    if n == 0 { return 0; }
    match mode {
        SelectMode::Index => (sel.round() as i64).rem_euclid(n as i64) as usize,
        SelectMode::Phase => {
            let p = sel.rem_euclid(1.0);
            ((p * n as f32).floor() as i64)
                .clamp(0, n as i64 - 1) as usize
        }
    }
}
