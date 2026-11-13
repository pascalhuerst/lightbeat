use crate::engine::nodes::math::multiplex::SelectMode;
use crate::engine::param_spec::IntParam;
use crate::engine::types::*;

const MAX: IntParam = IntParam::new("Max", 1, 256).at_least(1);

pub struct CounterDisplay {
    pub select_mode: SelectMode,
}

pub struct CounterProcessNode {
    id: NodeId,
    max_value: i64,
    direction: i64, // 1 = up, -1 = down
    select_mode: SelectMode,
    count: i64,
    wrap_out: f32,
    /// Port 0 value — semantics depend on `select_mode`:
    /// - `Index`: trigger (Logic) — rising edge advances the count.
    /// - `Phase`: 0..1 phase — count = floor(phase * max). No advance
    ///   on rising edges; wrap fires when the phase wraps around.
    in0: f32,
    reset_in: f32,
    prev_trigger: bool,
    prev_reset: bool,
    /// Phase-mode wrap detection: previous tick's input phase.
    prev_phase: f32,
    /// Latched so the first tick after mode-switch / load doesn't treat
    /// the initial phase value as a wrap.
    phase_initialized: bool,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports_for(select_mode: SelectMode) -> (Vec<PortDef>, Vec<PortDef>) {
    let port0 = match select_mode {
        SelectMode::Index => PortDef::new("trigger", PortType::Logic),
        SelectMode::Phase => PortDef::new("phase", PortType::Phase),
    };
    (
        vec![
            port0,
            PortDef::new("reset", PortType::Logic),
        ],
        vec![
            PortDef::new("count", PortType::Untyped),
            PortDef::new("wrap", PortType::Logic),
        ],
    )
}

impl CounterProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mut node = Self {
            id,
            max_value: 8,
            direction: 1,
            select_mode: SelectMode::Index,
            count: 0,
            wrap_out: 0.0,
            in0: 0.0,
            reset_in: 0.0,
            prev_trigger: false,
            prev_reset: false,
            prev_phase: 0.0,
            phase_initialized: false,
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        node.rebuild_ports();
        node
    }

    fn rebuild_ports(&mut self) {
        (self.inputs, self.outputs) = ports_for(self.select_mode);
    }
}

impl ProcessNode for CounterProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Counter" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        match pi {
            0 => self.in0 = v,
            1 => self.reset_in = v,
            _ => {}
        }
    }
    fn read_input(&self, pi: usize) -> f32 {
        match pi {
            0 => self.in0,
            1 => self.reset_in,
            _ => 0.0,
        }
    }

    fn process(&mut self) {
        let reset = self.reset_in >= 0.5;
        let max = self.max_value.max(1);
        self.wrap_out = 0.0;

        if reset && !self.prev_reset {
            self.count = if self.direction > 0 { 0 } else { max - 1 };
            // Re-latch prev_phase so Phase mode doesn't spuriously wrap
            // on the tick after reset.
            self.phase_initialized = false;
        }
        self.prev_reset = reset;

        match self.select_mode {
            SelectMode::Index => {
                let trigger = self.in0 >= 0.5;
                if trigger && !self.prev_trigger {
                    self.count += self.direction;
                    if self.count >= max {
                        self.count = 0;
                        self.wrap_out = 1.0;
                    } else if self.count < 0 {
                        self.count = max - 1;
                        self.wrap_out = 1.0;
                    }
                }
                self.prev_trigger = trigger;
            }
            SelectMode::Phase => {
                let phase = self.in0.rem_euclid(1.0);
                let raw = (phase * max as f32).floor() as i64;
                // Up direction: count = raw. Down direction: count counts
                // backward through the same `max` slots so the user gets
                // a reversed sequence on the same input.
                let idx = raw.clamp(0, max - 1);
                self.count = if self.direction > 0 { idx } else { max - 1 - idx };

                // Wrap pulse when phase resets (backwards jump > 0.5).
                if self.phase_initialized {
                    let delta = phase - self.prev_phase;
                    if delta < -0.5 {
                        self.wrap_out = 1.0;
                    }
                } else {
                    self.phase_initialized = true;
                }
                self.prev_phase = phase;
                self.prev_trigger = false;
            }
        }

        // Clamp the held count if the max shrank below the current count
        // (e.g. a signal on the promoted max came down while idle).
        if self.count >= max { self.count = 0; }
        if self.count < 0 { self.count = 0; }
    }

    fn read_output(&self, pi: usize) -> f32 {
        match pi {
            0 => self.count as f32,
            1 => self.wrap_out,
            _ => 0.0,
        }
    }

    fn non_promotable_params(&self) -> &'static [&'static str] {
        &["Select by"]
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            MAX.def(self.max_value),
            ParamDef::Choice {
                name: "Direction".into(),
                value: if self.direction > 0 { 0 } else { 1 },
                options: vec!["Up".into(), "Down".into()],
            },
            ParamDef::Choice {
                name: "Select by".into(),
                value: self.select_mode.to_index(),
                options: SelectMode::ALL.iter().map(|m| m.label().into()).collect(),
            },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => {
                self.max_value = MAX.apply(&value);
                if self.count >= self.max_value { self.count = 0; }
            }
            1 => self.direction = if value.as_usize() == 0 { 1 } else { -1 },
            2 => {
                let new_mode = SelectMode::from_index(value.as_usize());
                if new_mode != self.select_mode {
                    self.select_mode = new_mode;
                    // Reset edge-detection state so the first tick in the
                    // new mode doesn't emit a spurious wrap.
                    self.prev_trigger = false;
                    self.phase_initialized = false;
                    self.rebuild_ports();
                }
            }
            _ => {}
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "count": self.count }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(c) = data.get("count").and_then(|v| v.as_i64()) {
            self.count = c.max(0);
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(CounterDisplay {
            select_mode: self.select_mode,
        }));
    }
}
