use crate::engine::nodes::ui::button::ButtonMode;
use crate::engine::types::*;

/// One cell's transient state.
#[derive(Debug, Clone, Copy)]
struct CellState {
    /// Toggle mode persistent state.
    toggled: bool,
    /// Trigger mode: set by click, consumed next tick.
    trigger_pending: bool,
    /// Last click id received from widget (monotonic).
    last_click_id: u64,
}

impl Default for CellState {
    fn default() -> Self {
        Self { toggled: false, trigger_pending: false, last_click_id: 0 }
    }
}

pub struct ButtonGroupDisplay {
    pub rows: usize,
    pub cols: usize,
    pub mode: ButtonMode,
    pub states: Vec<bool>, // rows*cols booleans (toggle states)
}

pub struct ButtonGroupProcessNode {
    id: NodeId,
    rows: usize,
    cols: usize,
    mode: ButtonMode,
    cells: Vec<CellState>,
    outputs: Vec<PortDef>,
    output_values: Vec<f32>,
}

impl ButtonGroupProcessNode {
    pub fn new(id: NodeId) -> Self {
        let rows = 2;
        let cols = 2;
        let mut node = Self {
            id,
            rows,
            cols,
            mode: ButtonMode::Trigger,
            cells: vec![CellState::default(); rows * cols],
            outputs: Vec::new(),
            output_values: Vec::new(),
        };
        node.rebuild_ports();
        node
    }

    fn rebuild_ports(&mut self) {
        let n = self.rows * self.cols;
        self.outputs = (0..self.rows)
            .flat_map(|r| (0..self.cols).map(move |c| PortDef::new(
                format!("{},{}", r, c), PortType::Logic,
            )))
            .collect();
        self.output_values = vec![0.0; n];
        self.cells.resize(n, CellState::default());
    }

    fn handle_click(&mut self, idx: usize) {
        if idx >= self.cells.len() { return; }
        match self.mode {
            ButtonMode::Trigger => { self.cells[idx].trigger_pending = true; }
            ButtonMode::Toggle => { self.cells[idx].toggled = !self.cells[idx].toggled; }
        }
    }
}

impl ProcessNode for ButtonGroupProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Button Group" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn process(&mut self) {
        for (i, cell) in self.cells.iter_mut().enumerate() {
            let v = match self.mode {
                ButtonMode::Trigger => {
                    let v = if cell.trigger_pending { 1.0 } else { 0.0 };
                    cell.trigger_pending = false;
                    v
                }
                ButtonMode::Toggle => {
                    if cell.toggled { 1.0 } else { 0.0 }
                }
            };
            if i < self.output_values.len() {
                self.output_values[i] = v;
            }
        }
    }

    fn read_output(&self, pi: usize) -> f32 {
        self.output_values.get(pi).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        let states: Vec<bool> = self.cells.iter().map(|c| c.toggled).collect();
        Some(serde_json::json!({
            "rows": self.rows,
            "cols": self.cols,
            "mode": self.mode.as_str(),
            "states": states,
        }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
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
                if let Some(b) = s.as_bool() {
                    if let Some(cell) = self.cells.get_mut(i) { cell.toggled = b; }
                }
            }
        }

        // Per-cell click ids (map of "r,c" -> u64).
        if let Some(clicks) = data.get("clicks").and_then(|v| v.as_object()) {
            for (k, v) in clicks {
                let mut parts = k.split(',');
                let r = parts.next().and_then(|s| s.parse::<usize>().ok());
                let c = parts.next().and_then(|s| s.parse::<usize>().ok());
                if let (Some(r), Some(c)) = (r, c) {
                    if r < self.rows && c < self.cols {
                        let idx = r * self.cols + c;
                        if let Some(click_id) = v.as_u64() {
                            if click_id != self.cells[idx].last_click_id {
                                self.cells[idx].last_click_id = click_id;
                                self.handle_click(idx);
                            }
                        }
                    }
                }
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(ButtonGroupDisplay {
            rows: self.rows,
            cols: self.cols,
            mode: self.mode,
            states: self.cells.iter().map(|c| c.toggled).collect(),
        }));
    }
}
