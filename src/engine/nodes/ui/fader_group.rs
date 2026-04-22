use crate::engine::nodes::ui::common::MouseOverrideMode;
use crate::engine::types::*;

pub struct FaderGroupDisplay {
    pub name: String,
    pub rows: usize,
    pub cols: usize,
    /// Current output values, one per cell.
    pub outputs: Vec<f32>,
    /// Incoming input values, one per cell.
    pub inputs: Vec<f32>,
    /// Whether each cell is currently overridden.
    pub override_active: Vec<bool>,
    /// Override values per cell (meaningful only when override_active).
    pub override_values: Vec<f32>,
    /// Per-cell config.
    pub inputs_enabled: Vec<bool>,
    pub outputs_enabled: Vec<bool>,
    pub mouse_override: Vec<MouseOverrideMode>,
    pub bipolar: Vec<bool>,
    /// True if at least one cell has its input enabled — the widget uses this
    /// to decide whether to expose any input ports at all.
    pub any_input_enabled: bool,
    /// Same idea for outputs.
    pub any_output_enabled: bool,
}

pub struct FaderGroupProcessNode {
    id: NodeId,
    /// User-given label shown as the node title and in inspector. Round-trips
    /// through save_data; empty means "use the default Fader Group title".
    name: String,
    rows: usize,
    cols: usize,
    /// Local mouse values when inputs disabled.
    mouse_values: Vec<f32>,
    /// Incoming input signals.
    input_values: Vec<f32>,
    prev_input_values: Vec<f32>,
    /// Per-cell override state.
    override_values: Vec<Option<f32>>,
    /// Computed output each tick.
    output_values: Vec<f32>,

    /// Per-cell config.
    inputs_enabled: Vec<bool>,
    outputs_enabled: Vec<bool>,
    mouse_override: Vec<MouseOverrideMode>,
    bipolar: Vec<bool>,

    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl FaderGroupProcessNode {
    pub fn new(id: NodeId) -> Self {
        let rows = 1;
        let cols = 4;
        let n = rows * cols;
        let mut node = Self {
            id,
            name: String::new(),
            rows, cols,
            mouse_values: vec![0.0; n],
            input_values: vec![0.0; n],
            prev_input_values: vec![0.0; n],
            override_values: vec![None; n],
            output_values: vec![0.0; n],
            inputs_enabled: vec![false; n],
            outputs_enabled: vec![true; n],
            mouse_override: vec![MouseOverrideMode::No; n],
            bipolar: vec![false; n],
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        node.rebuild_ports();
        node
    }

    fn cell_count(&self) -> usize { self.rows * self.cols }

    fn any_input_enabled(&self) -> bool {
        self.inputs_enabled.iter().any(|&b| b)
    }

    fn any_output_enabled(&self) -> bool {
        self.outputs_enabled.iter().any(|&b| b)
    }

    fn rebuild_ports(&mut self) {
        let n = self.cell_count();
        // Same gating rule as inputs: if any output is enabled, expose all
        // output ports (widget gray-disables the rest); otherwise none.
        self.outputs = if self.any_output_enabled() {
            (0..self.rows)
                .flat_map(|r| (0..self.cols).map(move |c| PortDef::new(
                    format!("{},{}", r, c), PortType::Untyped,
                )))
                .collect()
        } else {
            Vec::new()
        };
        self.inputs = if self.any_input_enabled() {
            (0..self.rows)
                .flat_map(|r| (0..self.cols).map(move |c| PortDef::new(
                    format!("{},{}", r, c), PortType::Untyped,
                )))
                .collect()
        } else {
            Vec::new()
        };
        self.mouse_values.resize(n, 0.0);
        self.input_values.resize(n, 0.0);
        self.prev_input_values.resize(n, 0.0);
        self.override_values.resize(n, None);
        self.output_values.resize(n, 0.0);
        self.inputs_enabled.resize(n, false);
        self.outputs_enabled.resize(n, true);
        self.mouse_override.resize(n, MouseOverrideMode::No);
        self.bipolar.resize(n, false);
    }
}

impl ProcessNode for FaderGroupProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Fader Group" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if let Some(slot) = self.input_values.get_mut(port_index) {
            *slot = value;
        }
    }

    fn read_input(&self, port_index: usize) -> f32 {
        self.input_values.get(port_index).copied().unwrap_or(0.0)
    }

    fn process(&mut self) {
        let n = self.cell_count();
        for i in 0..n {
            let enabled = self.inputs_enabled[i];
            let mode = self.mouse_override[i];
            if enabled && mode.allows_override()
                && let Some(ov) = self.override_values[i]
                    && mode.should_clear(self.prev_input_values[i], self.input_values[i], ov) {
                        self.override_values[i] = None;
                    }
            self.prev_input_values[i] = self.input_values[i];

            self.output_values[i] = if !enabled {
                self.mouse_values[i]
            } else if let Some(ov) = self.override_values[i] {
                ov
            } else {
                self.input_values[i]
            };
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        // Disabled outputs read as 0 (they shouldn't be wired anyway, but
        // belt-and-braces for any stale connections).
        if !self.outputs_enabled.get(pi).copied().unwrap_or(false) { return 0.0; }
        self.output_values.get(pi).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        let overrides: Vec<serde_json::Value> = self.override_values.iter()
            .map(|ov| match ov {
                Some(v) => serde_json::json!(v),
                None => serde_json::Value::Null,
            })
            .collect();
        let mouse_override_strs: Vec<&str> = self.mouse_override.iter().map(|m| m.as_str()).collect();
        Some(serde_json::json!({
            "name": self.name,
            "rows": self.rows,
            "cols": self.cols,
            "mouse_values": self.mouse_values,
            "inputs_enabled": self.inputs_enabled,
            "outputs_enabled": self.outputs_enabled,
            "mouse_override": mouse_override_strs,
            "bipolar": self.bipolar,
            "override_values": overrides,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
            self.name = n.to_string();
        }
        let mut dims_changed = false;
        if let Some(r) = data.get("rows").and_then(|v| v.as_u64()) {
            let r = (r as usize).clamp(1, 16);
            if r != self.rows { self.rows = r; dims_changed = true; }
        }
        if let Some(c) = data.get("cols").and_then(|v| v.as_u64()) {
            let c = (c as usize).clamp(1, 16);
            if c != self.cols { self.cols = c; dims_changed = true; }
        }
        if dims_changed { self.rebuild_ports(); }

        if let Some(arr) = data.get("inputs_enabled").and_then(|v| v.as_array()) {
            for (i, v) in arr.iter().enumerate() {
                if let Some(b) = v.as_bool()
                    && let Some(slot) = self.inputs_enabled.get_mut(i) { *slot = b; }
            }
        }
        if let Some(arr) = data.get("outputs_enabled").and_then(|v| v.as_array()) {
            for (i, v) in arr.iter().enumerate() {
                if let Some(b) = v.as_bool()
                    && let Some(slot) = self.outputs_enabled.get_mut(i) { *slot = b; }
            }
        }
        if let Some(arr) = data.get("mouse_override").and_then(|v| v.as_array()) {
            for (i, v) in arr.iter().enumerate() {
                if let Some(s) = v.as_str()
                    && let Some(slot) = self.mouse_override.get_mut(i) {
                        *slot = MouseOverrideMode::from_str(s);
                    }
            }
        }
        if let Some(arr) = data.get("bipolar").and_then(|v| v.as_array()) {
            for (i, v) in arr.iter().enumerate() {
                if let Some(b) = v.as_bool()
                    && let Some(slot) = self.bipolar.get_mut(i) { *slot = b; }
            }
        }
        if let Some(vals) = data.get("mouse_values").or_else(|| data.get("values")).and_then(|v| v.as_array()) {
            for (i, v) in vals.iter().enumerate() {
                if let Some(f) = v.as_f64()
                    && let Some(slot) = self.mouse_values.get_mut(i) {
                        *slot = (f as f32).clamp(0.0, 1.0);
                    }
            }
        }
        if let Some(vals) = data.get("override_values").and_then(|v| v.as_array()) {
            for (i, v) in vals.iter().enumerate() {
                if let Some(slot) = self.override_values.get_mut(i) {
                    *slot = v.as_f64().map(|f| (f as f32).clamp(0.0, 1.0));
                }
            }
        }

        // Drop overrides for cells whose inputs are disabled.
        for i in 0..self.cell_count() {
            if !self.inputs_enabled[i] { self.override_values[i] = None; }
        }

        // Rebuild ports in case any_input_enabled changed.
        self.rebuild_ports();
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(FaderGroupDisplay {
            name: self.name.clone(),
            rows: self.rows,
            cols: self.cols,
            outputs: self.output_values.clone(),
            inputs: self.input_values.clone(),
            override_active: self.override_values.iter().map(|o| o.is_some()).collect(),
            override_values: self.override_values.iter().map(|o| o.unwrap_or(0.0)).collect(),
            inputs_enabled: self.inputs_enabled.clone(),
            outputs_enabled: self.outputs_enabled.clone(),
            mouse_override: self.mouse_override.clone(),
            bipolar: self.bipolar.clone(),
            any_input_enabled: self.any_input_enabled(),
            any_output_enabled: self.any_output_enabled(),
        }));
    }
}
