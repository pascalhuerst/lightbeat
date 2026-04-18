//! Generic lookup table: each column has its own port type, one row is
//! selected by the `index` input, and the selected row's cells are emitted
//! on per-column output ports.
//!
//! Structure (widget is the source of truth; engine rebuilds from
//! `pending_config` via `load_data`):
//! - `columns: Vec<LookupColumn>` — name + port type per column.
//! - `data: Vec<f32>` — flat row-major cells. Row stride = sum of each
//!   column's `port_type.channel_count()`. Row r, column c's first channel
//!   is at `data[r * stride + column_channel_offset(c)]`.

use crate::engine::types::*;

/// A single column in the lookup table.
#[derive(Debug, Clone)]
pub struct LookupColumn {
    pub name: String,
    pub port_type: PortType,
}

/// Display state the widget reads each frame.
pub struct LookupDisplay {
    pub columns: Vec<(String, PortType)>,
    /// Flat row-major data (same stride as the engine).
    pub data: Vec<f32>,
    pub row_count: usize,
    pub current_row: usize,
}

pub struct LookupProcessNode {
    id: NodeId,
    columns: Vec<LookupColumn>,
    /// Row-major flat data, sized `row_count * row_stride`.
    data: Vec<f32>,
    row_count: usize,
    current_row: usize,
    index_in: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
    output_values: Vec<f32>,
}

impl LookupProcessNode {
    pub fn new(id: NodeId) -> Self {
        // Default: 4 rows × 1 Untyped column with a simple ramp.
        let columns = vec![LookupColumn {
            name: "value".into(),
            port_type: PortType::Untyped,
        }];
        let data = vec![0.0, 0.25, 0.5, 1.0];
        let mut n = Self {
            id,
            columns,
            data,
            row_count: 4,
            current_row: 0,
            index_in: 0.0,
            inputs: vec![PortDef::new("index", PortType::Untyped)],
            outputs: Vec::new(),
            output_values: Vec::new(),
        };
        n.rebuild_ports();
        n
    }

    fn row_stride(&self) -> usize {
        self.columns.iter().map(|c| c.port_type.channel_count()).sum()
    }

    fn column_channel_offset(&self, col_idx: usize) -> usize {
        self.columns.iter().take(col_idx).map(|c| c.port_type.channel_count()).sum()
    }

    fn rebuild_ports(&mut self) {
        // Outputs[0] = "rows" (the table's row count, Untyped). Keeping
        // it first means adding/removing columns never shifts its index,
        // so wires from `rows` (typically into Counter.max) survive
        // schema edits.
        self.outputs = std::iter::once(PortDef::new("rows", PortType::Untyped))
            .chain(self.columns.iter().map(|c| PortDef::new(c.name.clone(), c.port_type)))
            .collect();
        let stride = self.row_stride();
        // output_values: [row_count][row_data ...]
        self.output_values = vec![0.0; 1 + stride];
        // Ensure data vec matches row_count × stride (pad or truncate).
        let expected = self.row_count * stride;
        if self.data.len() < expected {
            self.data.resize(expected, 0.0);
        } else if self.data.len() > expected {
            self.data.truncate(expected);
        }
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
        // output_values[0] is always the row count, regardless of row_count
        // being zero (so a fresh node still emits 0 instead of NaN/garbage).
        self.output_values[0] = self.row_count as f32;
        if self.row_count == 0 {
            for v in self.output_values.iter_mut().skip(1) { *v = 0.0; }
            return;
        }
        let stride = self.row_stride();
        if stride == 0 { return; }
        // Round + wrap (same semantics as before).
        let idx = (self.index_in.round() as i64).rem_euclid(self.row_count as i64) as usize;
        self.current_row = idx;
        let row_start = idx * stride;
        for c in 0..stride {
            self.output_values[1 + c] = self.data.get(row_start + c).copied().unwrap_or(0.0);
        }
    }

    fn read_output(&self, channel: usize) -> f32 {
        self.output_values.get(channel).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "columns": self.columns.iter().map(|c| serde_json::json!({
                "name": c.name,
                "type": port_type_to_str(c.port_type),
            })).collect::<Vec<_>>(),
            "row_count": self.row_count,
            "data": self.data,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        // Columns.
        if let Some(arr) = data.get("columns").and_then(|v| v.as_array()) {
            let mut cols = Vec::with_capacity(arr.len());
            for entry in arr {
                let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("col").to_string();
                let port_type = entry.get("type")
                    .and_then(|v| v.as_str())
                    .and_then(port_type_from_str)
                    .unwrap_or(PortType::Untyped);
                cols.push(LookupColumn { name, port_type });
            }
            self.columns = cols;
        }
        if let Some(n) = data.get("row_count").and_then(|v| v.as_u64()) {
            self.row_count = n as usize;
        }
        if let Some(arr) = data.get("data").and_then(|v| v.as_array()) {
            self.data = arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect();
        }
        self.rebuild_ports();
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(LookupDisplay {
            columns: self.columns.iter().map(|c| (c.name.clone(), c.port_type)).collect(),
            data: self.data.clone(),
            row_count: self.row_count,
            current_row: self.current_row,
        }));
    }
}

/// Column-type labels also recognized by `port_type_from_str` for
/// round-tripping. Kept separate from the subgraph helpers so this module
/// stays self-contained.
pub fn port_type_to_str(pt: PortType) -> &'static str {
    match pt {
        PortType::Logic => "logic",
        PortType::Phase => "phase",
        PortType::Untyped => "untyped",
        PortType::Any => "any",
        PortType::Color => "color",
        PortType::Position => "position",
        PortType::Palette => "palette",
        PortType::Gradient => "gradient",
    }
}

pub fn port_type_from_str(s: &str) -> Option<PortType> {
    Some(match s {
        "logic" => PortType::Logic,
        "phase" => PortType::Phase,
        "untyped" => PortType::Untyped,
        "any" => PortType::Any,
        "color" => PortType::Color,
        "position" => PortType::Position,
        "palette" => PortType::Palette,
        "gradient" => PortType::Gradient,
        _ => return None,
    })
}
