use crate::engine::nodes::ui::button::ButtonMode;
use crate::engine::nodes::ui::common::MouseOverrideMode;
use crate::engine::types::*;

/// One cell's transient state.
#[derive(Debug, Clone, Copy, Default)]
struct CellState {
    /// Toggle mode persistent state (no input).
    toggled: bool,
    /// Override state for Toggle + inputs + override.
    override_state: Option<bool>,
    /// Trigger mode: set by click, consumed next tick.
    trigger_pending: bool,
    /// Last click id received from widget (monotonic).
    last_click_id: u64,
    /// Input value snapshot.
    input_value: f32,
    prev_input_value: f32,
}

pub struct ButtonGroupDisplay {
    pub name: String,
    pub rows: usize,
    pub cols: usize,
    pub mode: ButtonMode,
    pub states: Vec<bool>,
    pub input_values: Vec<f32>,
    pub override_active: Vec<bool>,
    pub inputs_enabled: bool,
    pub override_enabled: bool,
    pub reset_mode: MouseOverrideMode,
}

pub struct ButtonGroupProcessNode {
    id: NodeId,
    name: String,
    rows: usize,
    cols: usize,
    mode: ButtonMode,
    cells: Vec<CellState>,
    inputs_enabled: bool,
    override_enabled: bool,
    reset_mode: MouseOverrideMode,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
    output_values: Vec<f32>,
}

impl ButtonGroupProcessNode {
    pub fn new(id: NodeId) -> Self {
        let rows = 2;
        let cols = 2;
        let mut node = Self {
            id, name: String::new(), rows, cols,
            mode: ButtonMode::Trigger,
            cells: vec![CellState::default(); rows * cols],
            inputs_enabled: false,
            override_enabled: false,
            reset_mode: MouseOverrideMode::ClearOnReset,
            inputs: Vec::new(),
            outputs: Vec::new(),
            output_values: Vec::new(),
        };
        node.rebuild_ports();
        node
    }

    fn cell_count(&self) -> usize { self.rows * self.cols }

    fn rebuild_ports(&mut self) {
        let n = self.cell_count();
        self.outputs = (0..self.rows)
            .flat_map(|r| (0..self.cols).map(move |c| PortDef::new(
                format!("{},{}", r, c), PortType::Logic,
            )))
            .collect();
        self.inputs = if self.inputs_enabled {
            (0..self.rows)
                .flat_map(|r| (0..self.cols).map(move |c| PortDef::new(
                    format!("{},{}", r, c), PortType::Logic,
                )))
                .collect()
        } else {
            Vec::new()
        };
        self.output_values.resize(n, 0.0);
        self.cells.resize(n, CellState::default());
    }

    fn handle_click(&mut self, idx: usize) {
        if idx >= self.cells.len() { return; }
        match self.mode {
            ButtonMode::Trigger => { self.cells[idx].trigger_pending = true; }
            ButtonMode::Toggle => {
                if self.inputs_enabled && self.override_enabled {
                    let cur = self.cells[idx].override_state
                        .unwrap_or(self.cells[idx].input_value >= 0.5);
                    self.cells[idx].override_state = Some(!cur);
                } else {
                    self.cells[idx].toggled = !self.cells[idx].toggled;
                }
            }
        }
    }
}

impl ProcessNode for ButtonGroupProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Button Group" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, port_index: usize, value: f32) {
        if let Some(cell) = self.cells.get_mut(port_index) {
            cell.input_value = value;
        }
    }

    fn read_input(&self, port_index: usize) -> f32 {
        self.cells.get(port_index).map(|c| c.input_value).unwrap_or(0.0)
    }

    fn process(&mut self) {
        for (i, cell) in self.cells.iter_mut().enumerate() {
            let prev = cell.prev_input_value;
            // Pass-through reset.
            if self.mode == ButtonMode::Toggle && self.inputs_enabled && self.override_enabled
                && let Some(ov_bool) = cell.override_state {
                    let ov = if ov_bool { 1.0 } else { 0.0 };
                    if self.reset_mode.should_clear(prev, cell.input_value, ov) {
                        cell.override_state = None;
                    }
                }
            cell.prev_input_value = cell.input_value;

            let v = match self.mode {
                ButtonMode::Trigger => {
                    let mut fire = cell.trigger_pending;
                    if self.inputs_enabled && cell.input_value >= 0.5 && prev < 0.5 {
                        fire = true;
                    }
                    cell.trigger_pending = false;
                    if fire { 1.0 } else { 0.0 }
                }
                ButtonMode::Toggle => {
                    let on = if self.inputs_enabled {
                        cell.override_state.unwrap_or(cell.input_value >= 0.5)
                    } else {
                        cell.toggled
                    };
                    if on { 1.0 } else { 0.0 }
                }
            };
            if let Some(slot) = self.output_values.get_mut(i) {
                *slot = v;
            }
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        self.output_values.get(pi).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        let states: Vec<bool> = self.cells.iter().map(|c| c.toggled).collect();
        let overrides: Vec<serde_json::Value> = self.cells.iter().map(|c| match c.override_state {
            Some(b) => serde_json::json!(b),
            None => serde_json::Value::Null,
        }).collect();
        Some(serde_json::json!({
            "name": self.name,
            "rows": self.rows,
            "cols": self.cols,
            "mode": self.mode.as_str(),
            "states": states,
            "inputs_enabled": self.inputs_enabled,
            "override_enabled": self.override_enabled,
            "reset_mode": self.reset_mode.as_str(),
            "override_states": overrides,
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
        let mut ports_dirty = dims_changed;
        if let Some(b) = data.get("inputs_enabled").and_then(|v| v.as_bool()) {
            if b != self.inputs_enabled { ports_dirty = true; }
            self.inputs_enabled = b;
        }
        if let Some(b) = data.get("override_enabled").and_then(|v| v.as_bool()) {
            self.override_enabled = b;
        }
        if let Some(s) = data.get("reset_mode").and_then(|v| v.as_str()) {
            self.reset_mode = MouseOverrideMode::from_str(s);
        }
        if ports_dirty { self.rebuild_ports(); }

        if let Some(m) = data.get("mode").and_then(|v| v.as_str()) {
            let new_mode = ButtonMode::from_str(m);
            if new_mode != self.mode {
                self.mode = new_mode;
                for c in &mut self.cells {
                    c.trigger_pending = false;
                }
            }
        }

        if let Some(states) = data.get("states").and_then(|v| v.as_array()) {
            for (i, s) in states.iter().enumerate() {
                if let Some(b) = s.as_bool()
                    && let Some(cell) = self.cells.get_mut(i) { cell.toggled = b; }
            }
        }
        if let Some(overrides) = data.get("override_states").and_then(|v| v.as_array()) {
            for (i, v) in overrides.iter().enumerate() {
                if let Some(cell) = self.cells.get_mut(i) {
                    cell.override_state = v.as_bool();
                }
            }
        }

        if !self.inputs_enabled {
            for cell in &mut self.cells {
                cell.override_state = None;
                cell.input_value = 0.0;
                cell.prev_input_value = 0.0;
            }
        }

        // Per-cell click ids (map of "r,c" -> u64).
        if let Some(clicks) = data.get("clicks").and_then(|v| v.as_object()) {
            for (k, v) in clicks {
                let mut parts = k.split(',');
                let r = parts.next().and_then(|s| s.parse::<usize>().ok());
                let c = parts.next().and_then(|s| s.parse::<usize>().ok());
                if let (Some(r), Some(c)) = (r, c)
                    && r < self.rows && c < self.cols {
                        let idx = r * self.cols + c;
                        if let Some(click_id) = v.as_u64()
                            && click_id != self.cells[idx].last_click_id {
                                self.cells[idx].last_click_id = click_id;
                                self.handle_click(idx);
                            }
                    }
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        let states: Vec<bool> = self.cells.iter().enumerate().map(|(i, c)| {
            match self.mode {
                ButtonMode::Toggle => {
                    if self.inputs_enabled {
                        c.override_state.unwrap_or(c.input_value >= 0.5)
                    } else {
                        c.toggled
                    }
                }
                ButtonMode::Trigger => {
                    self.output_values.get(i).copied().unwrap_or(0.0) >= 0.5
                }
            }
        }).collect();
        shared.display = Some(Box::new(ButtonGroupDisplay {
            name: self.name.clone(),
            rows: self.rows,
            cols: self.cols,
            mode: self.mode,
            states,
            input_values: self.cells.iter().map(|c| c.input_value).collect(),
            override_active: self.cells.iter().map(|c| c.override_state.is_some()).collect(),
            inputs_enabled: self.inputs_enabled,
            override_enabled: self.override_enabled,
            reset_mode: self.reset_mode,
        }));
    }
}
