use crate::engine::types::*;

pub struct FaderGroupDisplay {
    pub rows: usize,
    pub cols: usize,
    pub values: Vec<f32>,
}

pub struct FaderGroupProcessNode {
    id: NodeId,
    rows: usize,
    cols: usize,
    values: Vec<f32>,
    outputs: Vec<PortDef>,
}

impl FaderGroupProcessNode {
    pub fn new(id: NodeId) -> Self {
        let rows = 1;
        let cols = 4;
        let mut node = Self {
            id,
            rows,
            cols,
            values: vec![0.0; rows * cols],
            outputs: Vec::new(),
        };
        node.rebuild_ports();
        node
    }

    fn rebuild_ports(&mut self) {
        let n = self.rows * self.cols;
        self.outputs = (0..self.rows)
            .flat_map(|r| (0..self.cols).map(move |c| PortDef::new(
                format!("{},{}", r, c), PortType::Untyped,
            )))
            .collect();
        self.values.resize(n, 0.0);
    }
}

impl ProcessNode for FaderGroupProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Fader Group" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn process(&mut self) {}

    fn read_output(&self, pi: usize) -> f32 {
        self.values.get(pi).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "rows": self.rows,
            "cols": self.cols,
            "values": self.values,
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

        if let Some(vals) = data.get("values").and_then(|v| v.as_array()) {
            for (i, v) in vals.iter().enumerate() {
                if let Some(f) = v.as_f64() {
                    if let Some(slot) = self.values.get_mut(i) {
                        *slot = (f as f32).clamp(0.0, 1.0);
                    }
                }
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(FaderGroupDisplay {
            rows: self.rows,
            cols: self.cols,
            values: self.values.clone(),
        }));
    }
}
