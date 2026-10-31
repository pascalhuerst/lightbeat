//! Level Meter and Gauge: two ways of showing where a value sits between a
//! minimum and a maximum. They share everything but how they are drawn.

use std::any::Any;

use crate::engine::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeterKind {
    Level,
    Gauge,
}

impl MeterKind {
    pub fn type_name(self) -> &'static str {
        match self {
            MeterKind::Level => "Level Meter",
            MeterKind::Gauge => "Gauge",
        }
    }
}

/// What the widget draws.
pub struct MeterData {
    pub name: String,
    pub unit: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    /// Level meter only: fill from the bottom instead of from the left.
    pub vertical: bool,
}

pub struct MeterProcessNode {
    id: NodeId,
    kind: MeterKind,
    name: String,
    unit: String,
    value: f32,
    min: f32,
    max: f32,
    vertical: bool,
    inputs: Vec<PortDef>,
}

impl MeterProcessNode {
    pub fn new(id: NodeId, kind: MeterKind) -> Self {
        Self {
            id,
            kind,
            name: String::new(),
            unit: String::new(),
            value: 0.0,
            min: 0.0,
            max: 1.0,
            vertical: kind == MeterKind::Level,
            inputs: vec![PortDef::new("in", PortType::Any)],
        }
    }
}

impl ProcessNode for MeterProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { self.kind.type_name() }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.value = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.value } else { 0.0 }
    }
    fn process(&mut self) {}

    fn params(&self) -> Vec<ParamDef> {
        let mut params = vec![
            ParamDef::Float { name: "Min".into(), value: self.min, min: -100_000.0, max: 100_000.0, step: 0.01, unit: "" },
            ParamDef::Float { name: "Max".into(), value: self.max, min: -100_000.0, max: 100_000.0, step: 0.01, unit: "" },
        ];
        if self.kind == MeterKind::Level {
            params.push(ParamDef::Bool { name: "Vertical".into(), value: self.vertical });
        }
        params
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => self.min = value.as_f32(),
            1 => self.max = value.as_f32(),
            2 => self.vertical = value.as_f32() >= 0.5,
            _ => {}
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "name": self.name, "unit": self.unit }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(n) = data.get("name").and_then(|v| v.as_str()) {
            self.name = n.to_string();
        }
        if let Some(u) = data.get("unit").and_then(|v| v.as_str()) {
            self.unit = u.to_string();
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(MeterData {
            name: self.name.clone(),
            unit: self.unit.clone(),
            value: self.value,
            min: self.min,
            max: self.max,
            vertical: self.vertical,
        }));
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_level_meter_has_an_orientation_and_a_range_can_be_set() {
        let level = MeterProcessNode::new(NodeId(1), MeterKind::Level);
        let mut gauge = MeterProcessNode::new(NodeId(2), MeterKind::Gauge);
        assert_eq!(level.params().len(), 3);
        assert_eq!(gauge.params().len(), 2);
        gauge.set_param(0, ParamValue::Float(-50.0));
        gauge.set_param(1, ParamValue::Float(250.0));
        gauge.write_input(0, 10.0);
        let mut shared = NodeSharedState::new(1, 0);
        gauge.update_display(&mut shared);
        let d = shared.display.as_ref().unwrap().downcast_ref::<MeterData>().unwrap();
        assert_eq!((d.min, d.max, d.value), (-50.0, 250.0, 10.0));
    }

    #[test]
    fn the_name_and_unit_are_saved() {
        let mut n = MeterProcessNode::new(NodeId(1), MeterKind::Gauge);
        n.load_data(&serde_json::json!({ "name": "Speed", "unit": "km/h" }));
        let saved = n.save_data().unwrap();
        assert_eq!((saved["name"].as_str(), saved["unit"].as_str()), (Some("Speed"), Some("km/h")));
    }
}
