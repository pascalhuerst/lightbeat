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

use serde::{Deserialize, Serialize};

use crate::engine::types::*;

/// What the index input means.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LookupInput {
    /// A row number: 0 is the first row. A clock's step counter goes here.
    Index,
    /// 0..1 spread over the whole table, so a phase or a fader walks through it.
    Phase,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LookupEdge {
    Wrap,
    Clamp,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LookupBlend {
    /// The nearest row.
    Step,
    /// A position between two rows mixes them (not a Logic column).
    Linear,
}

/// A single column in the lookup table.
#[derive(Debug, Clone)]
pub struct LookupColumn {
    pub name: String,
    pub port_type: PortType,
}

/// Display state the widget reads each frame.
pub struct LookupDisplay {
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
    /// How `index_in` is read (see `LookupInput`), what happens past the ends
    /// of the table and whether a position between rows blends them.
    input: LookupInput,
    edge: LookupEdge,
    blend: LookupBlend,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
    output_values: Vec<f32>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports_for(columns: &[LookupColumn]) -> (Vec<PortDef>, Vec<PortDef>) {
    // "rows" stays at index 0 so wires from it survive schema edits.
    (
        vec![PortDef::new("index", PortType::Untyped)],
        std::iter::once(PortDef::new("rows", PortType::Untyped))
            .chain(columns.iter().map(|c| PortDef::new(c.name.clone(), c.port_type)))
            .collect(),
    )
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
            input: LookupInput::Index,
            edge: LookupEdge::Wrap,
            blend: LookupBlend::Step,
            inputs: ports_for(&[]).0,
            outputs: Vec::new(),
            output_values: Vec::new(),
        };
        n.rebuild_ports();
        n
    }

    fn row_stride(&self) -> usize {
        self.columns.iter().map(|c| c.port_type.channel_count()).sum()
    }

    fn rebuild_ports(&mut self) {
        // Outputs[0] = "rows" (the table's row count, Untyped). Keeping
        // it first means adding/removing columns never shifts its index,
        // so wires from `rows` (typically into Counter.max) survive
        // schema edits.
        self.outputs = ports_for(&self.columns).1;
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
    fn type_name(&self) -> &'static str { "Lookup Table" }
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
        let n = self.row_count as i64;
        let edge = |i: i64| match self.edge {
            LookupEdge::Wrap => i.rem_euclid(n) as usize,
            LookupEdge::Clamp => i.clamp(0, n - 1) as usize,
        };
        // Where in the table the input points, in rows (0.0 is the first row).
        let pos = match (self.input, self.blend, self.edge) {
            (LookupInput::Index, _, _) => self.index_in,
            // Clamped and blended, the phase's end is the last row itself.
            (LookupInput::Phase, LookupBlend::Linear, LookupEdge::Clamp) => {
                self.index_in.clamp(0.0, 1.0) * (n - 1) as f32
            }
            (LookupInput::Phase, _, _) => self.index_in.clamp(0.0, 1.0) * n as f32,
        };
        let (row_a, row_b, t) = match self.blend {
            LookupBlend::Step => {
                let i = match self.input {
                    LookupInput::Index => pos.round(),
                    LookupInput::Phase => pos.floor(),
                } as i64;
                (edge(i), edge(i), 0.0)
            }
            LookupBlend::Linear => {
                let f = pos.floor();
                (edge(f as i64), edge(f as i64 + 1), pos - f)
            }
        };
        self.current_row = if t >= 0.5 { row_b } else { row_a };
        let mut offset = 0;
        for col in &self.columns {
            let width = col.port_type.channel_count();
            let mix = if col.port_type == PortType::Logic { if t >= 0.5 { 1.0 } else { 0.0 } } else { t };
            for c in offset..offset + width {
                let a = self.data.get(row_a * stride + c).copied().unwrap_or(0.0);
                let b = self.data.get(row_b * stride + c).copied().unwrap_or(0.0);
                self.output_values[1 + c] = a + (b - a) * mix;
            }
            offset += width;
        }
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Choice {
                name: "Select by".into(),
                value: match self.input { LookupInput::Index => 0, LookupInput::Phase => 1 },
                options: vec!["Index".into(), "Phase".into()],
            },
            ParamDef::Choice {
                name: "Edge".into(),
                value: match self.edge { LookupEdge::Wrap => 0, LookupEdge::Clamp => 1 },
                options: vec!["Wrap around".into(), "Hold the ends".into()],
            },
            ParamDef::Choice {
                name: "Blend".into(),
                value: match self.blend { LookupBlend::Step => 0, LookupBlend::Linear => 1 },
                options: vec!["Step".into(), "Linear".into()],
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        let v = value.as_usize();
        match index {
            0 => self.input = if v == 1 { LookupInput::Phase } else { LookupInput::Index },
            1 => self.edge = if v == 1 { LookupEdge::Clamp } else { LookupEdge::Wrap },
            2 => self.blend = if v == 1 { LookupBlend::Linear } else { LookupBlend::Step },
            _ => {}
        }
    }

    fn read_output(&self, channel: usize) -> f32 {
        self.output_values.get(channel).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(LookupConfig::of(&self.columns, self.row_count, &self.data).to_value())
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        let cfg = LookupConfig::from_value(data);
        if let Some(cols) = cfg.columns() {
            self.columns = cols;
        }
        if let Some(n) = cfg.row_count {
            self.row_count = n;
        }
        if let Some(d) = cfg.data {
            self.data = d;
        }
        self.rebuild_ports();
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(LookupDisplay {
            current_row: self.current_row,
        }));
    }
}

/// The table as saved and as the widget pushes it through `pending_config`.
/// Every key is optional so that a partial update only touches what it names.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LookupConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    columns: Option<Vec<ColumnConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<f32>>,
}

#[derive(Serialize, Deserialize)]
struct ColumnConfig {
    #[serde(default = "default_column_name")]
    name: String,
    #[serde(rename = "type", default)]
    port_type: Option<String>,
}

fn default_column_name() -> String { "col".to_string() }

impl LookupConfig {
    pub fn of(columns: &[LookupColumn], row_count: usize, data: &[f32]) -> Self {
        Self {
            columns: Some(columns.iter().map(|c| ColumnConfig {
                name: c.name.clone(),
                port_type: Some(port_type_to_str(c.port_type).to_string()),
            }).collect()),
            row_count: Some(row_count),
            data: Some(data.to_vec()),
        }
    }

    /// Malformed data loads as an empty update, which changes nothing.
    pub fn from_value(v: &serde_json::Value) -> Self {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    pub fn columns(&self) -> Option<Vec<LookupColumn>> {
        self.columns.as_ref().map(|cols| cols.iter().map(|c| LookupColumn {
            name: c.name.clone(),
            port_type: c.port_type.as_deref().and_then(port_type_from_str).unwrap_or(PortType::Untyped),
        }).collect())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A table of the numbers 0, 10, 100, 1000 in one number column.
    fn table() -> LookupProcessNode {
        let mut n = LookupProcessNode::new(NodeId(1));
        n.load_data(&serde_json::json!({
            "columns": [{"name": "value", "type": "untyped"}],
            "row_count": 4,
            "data": [0.0, 10.0, 100.0, 1000.0],
        }));
        n
    }

    fn out(n: &mut LookupProcessNode, v: f32) -> f32 {
        n.write_input(0, v);
        n.process();
        n.read_output(1)
    }

    #[test]
    fn a_step_counter_picks_the_rows() {
        let mut n = table();
        let got: Vec<f32> = (0..4).map(|i| out(&mut n, i as f32)).collect();
        assert_eq!(got, vec![0.0, 10.0, 100.0, 1000.0]);
        assert_eq!(out(&mut n, 5.0), 10.0);
    }

    #[test]
    fn clamping_holds_the_ends() {
        let mut n = table();
        n.set_param(1, ParamValue::Choice(1));
        assert_eq!(out(&mut n, 9.0), 1000.0);
        assert_eq!(out(&mut n, -3.0), 0.0);
    }

    #[test]
    fn a_phase_spreads_over_the_table() {
        let mut n = table();
        n.set_param(0, ParamValue::Choice(1));
        assert_eq!(out(&mut n, 0.0), 0.0);
        assert_eq!(out(&mut n, 0.30), 10.0);
        assert_eq!(out(&mut n, 0.99), 1000.0);
    }

    #[test]
    fn linear_blends_between_rows() {
        let mut n = table();
        n.set_param(2, ParamValue::Choice(1));
        assert_eq!(out(&mut n, 1.5), 55.0);
        // Wrapped, the last row blends back into the first.
        assert_eq!(out(&mut n, 3.5), 500.0);
        n.set_param(1, ParamValue::Choice(1));
        n.set_param(0, ParamValue::Choice(1));
        assert_eq!(out(&mut n, 1.0), 1000.0);
    }

    #[test]
    fn the_hand_written_save_shape_still_loads_and_saves() {
        let saved = serde_json::json!({
            "columns": [{"name": "a", "type": "phase"}, {"name": "b", "type": "color"}],
            "row_count": 2,
            "data": [0.0, 0.5, 1.0, 0.25, 0.0, 0.0, 1.0, 0.5],
        });
        let mut n = LookupProcessNode::new(NodeId(1));
        n.load_data(&saved);
        assert_eq!(n.columns.len(), 2);
        assert_eq!(n.columns[1].port_type, PortType::Color);
        assert_eq!(n.row_count, 2);
        assert_eq!(n.data.len(), 8);
        assert_eq!(n.save_data(), Some(saved));
    }

    #[test]
    fn a_column_without_a_name_or_a_known_type_gets_the_defaults() {
        let mut n = LookupProcessNode::new(NodeId(1));
        n.load_data(&serde_json::json!({"columns": [{"type": "nonsense"}]}));
        assert_eq!(n.columns[0].name, "col");
        assert_eq!(n.columns[0].port_type, PortType::Untyped);
    }
}
