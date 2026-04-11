use crate::engine::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupMode {
    Float,
    Color,
}

impl LookupMode {
    pub fn label(&self) -> &'static str {
        match self {
            LookupMode::Float => "Float",
            LookupMode::Color => "Color",
        }
    }

    pub fn output_type(&self) -> PortType {
        match self {
            LookupMode::Float => PortType::Untyped,
            LookupMode::Color => PortType::Color,
        }
    }

    pub fn channels_per_entry(&self) -> usize {
        match self {
            LookupMode::Float => 1,
            LookupMode::Color => 3,
        }
    }

    pub fn from_index(i: usize) -> Self {
        match i { 1 => LookupMode::Color, _ => LookupMode::Float }
    }

    pub fn to_index(&self) -> usize {
        match self { LookupMode::Float => 0, LookupMode::Color => 1 }
    }
}

/// Display state for the widget.
pub struct LookupDisplay {
    pub mode: LookupMode,
    /// Flat table data: for Float mode, one f32 per entry; for Color, 3 f32s per entry.
    pub table: Vec<f32>,
    pub current_index: usize,
    pub entry_count: usize,
}

pub struct LookupProcessNode {
    id: NodeId,
    mode: LookupMode,
    /// Flat storage: entries × channels_per_entry.
    table: Vec<f32>,
    entry_count: usize,
    index_in: f32,
    current_index: usize,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl LookupProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mode = LookupMode::Float;
        // Default: 4 float entries.
        let table = vec![0.0, 0.25, 0.5, 1.0];
        Self {
            id,
            mode,
            table,
            entry_count: 4,
            index_in: 0.0,
            current_index: 0,
            inputs: vec![PortDef::new("index", PortType::Untyped)],
            outputs: vec![PortDef::new("out", PortType::Untyped)],
        }
    }

    fn reconfigure_mode(&mut self, mode: LookupMode) {
        if mode == self.mode { return; }
        let cpe = mode.channels_per_entry();
        // Reset table with default entries.
        self.entry_count = 4;
        self.table = vec![0.0; self.entry_count * cpe];
        self.mode = mode;
        self.outputs = vec![PortDef::new("out", mode.output_type())];
    }
}

impl ProcessNode for LookupProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Lookup" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.index_in = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.index_in } else { 0.0 }
    }

    fn process(&mut self) {
        if self.entry_count == 0 { return; }
        // Snap to nearest integer index, wrap.
        let idx = (self.index_in.round() as i64).rem_euclid(self.entry_count as i64) as usize;
        self.current_index = idx;
    }

    fn read_output(&self, channel: usize) -> f32 {
        let cpe = self.mode.channels_per_entry();
        let base = self.current_index * cpe;
        self.table.get(base + channel).copied().unwrap_or(0.0)
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Choice {
                name: "Mode".into(),
                value: self.mode.to_index(),
                options: vec!["Float".into(), "Color".into()],
            },
            ParamDef::Int {
                name: "Entries".into(),
                value: self.entry_count as i64,
                min: 1,
                max: 64,
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match (index, value) {
            (0, ParamValue::Choice(v)) => {
                self.reconfigure_mode(LookupMode::from_index(v));
            }
            (1, ParamValue::Int(v)) => {
                let new_count = v.max(1) as usize;
                let cpe = self.mode.channels_per_entry();
                self.table.resize(new_count * cpe, 0.0);
                self.entry_count = new_count;
            }
            // Indices 100+ are table value edits from the widget.
            // Format: index = 100 + entry_index * cpe + channel
            (i, ParamValue::Float(v)) if i >= 100 => {
                let table_idx = i - 100;
                if table_idx < self.table.len() {
                    self.table[table_idx] = v;
                }
            }
            _ => {}
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "mode": self.mode.to_index(),
            "table": self.table,
            "entry_count": self.entry_count,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(m) = data.get("mode").and_then(|v| v.as_u64()) {
            self.mode = LookupMode::from_index(m as usize);
            self.outputs = vec![PortDef::new("out", self.mode.output_type())];
        }
        if let Some(n) = data.get("entry_count").and_then(|v| v.as_u64()) {
            self.entry_count = n as usize;
        }
        if let Some(arr) = data.get("table").and_then(|v| v.as_array()) {
            self.table = arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect();
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(LookupDisplay {
            mode: self.mode,
            table: self.table.clone(),
            current_index: self.current_index,
            entry_count: self.entry_count,
        }));
    }
}
