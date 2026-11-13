use crate::engine::types::*;

pub struct ConstantProcessNode {
    id: NodeId,
    value: f32,
    port_type: PortType,
    outputs: Vec<PortDef>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports_for(port_type: PortType) -> (Vec<PortDef>, Vec<PortDef>) {
    (vec![], vec![PortDef::new("out", port_type)])
}

impl ConstantProcessNode {
    pub fn new(id: NodeId, port_type: PortType, default: f32) -> Self {
        Self {
            id,
            value: default,
            port_type,
            outputs: ports_for(port_type).1,
        }
    }

    /// The kinds a constant can be switched between, in the order of the
    /// "Operation" dropdown.
    pub const KINDS: [PortType; 3] = [PortType::Untyped, PortType::Logic, PortType::Phase];

    fn value_range(port_type: PortType) -> (f32, f32, f32) {
        match port_type {
            PortType::Logic => (0.0, 1.0, 1.0),
            PortType::Phase => (0.0, 1.0, 0.01),
            // A constant can be anything; the node's own box is not bound by this.
            _ => (-1.0e6, 1.0e6, 0.01),
        }
    }

    pub fn type_name_for(port_type: PortType) -> &'static str {
        match port_type {
            PortType::Logic => "Const Logic",
            PortType::Phase => "Const Phase",
            _ => "Const Value",
        }
    }
}

impl ProcessNode for ConstantProcessNode {
    fn node_id(&self) -> NodeId { self.id }

    fn type_name(&self) -> &'static str {
        Self::type_name_for(self.port_type)
    }

    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn process(&mut self) {}

    fn read_output(&self, port_index: usize) -> f32 {
        match port_index { 0 => self.value, _ => 0.0 }
    }

    fn params(&self) -> Vec<ParamDef> {
        let (min, max, step) = Self::value_range(self.port_type);
        let current = Self::KINDS.iter().position(|k| *k == self.port_type).unwrap_or(0);
        vec![
            ParamDef::Float {
                name: "Value".into(),
                value: self.value,
                min, max, step,
                unit: "",
            },
            ParamDef::Choice {
                name: "Operation".into(),
                value: current,
                options: Self::KINDS.iter().map(|k| Self::type_name_for(*k).trim_start_matches("Const ").to_string()).collect(),
            },
        ]
    }

    fn non_promotable_params(&self) -> &'static [&'static str] { &["Operation"] }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.value = value.as_f32(),
            1 => {
                let kind = Self::KINDS.get(value.as_usize()).copied().unwrap_or(PortType::Untyped);
                if kind != self.port_type {
                    self.port_type = kind;
                    self.outputs = ports_for(kind).1;
                    let (min, max, _) = Self::value_range(kind);
                    self.value = self.value.clamp(min, max);
                }
            }
            _ => {}
        }
    }
}
