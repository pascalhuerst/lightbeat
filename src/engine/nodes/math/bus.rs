//! Bus Merge, Bus Split and Bus Pick: the three tools that turn single wires
//! into a bus (one wire of N lanes) and back.
//!
//! The bus port keeps its lanes in the node's flat channel buffer, lane `k`
//! at `k * channels_of_the_type`, which is also how the plain ports of the
//! same node are laid out one after the other. Merge and Split therefore only
//! pass the buffer through; Pick reads one lane out of it.

use crate::engine::nodes::math::multiplex::{MUX_TYPE_NAMES, type_from_index, type_to_index};
use crate::engine::types::*;

pub const BUS_DEFAULT_LANES: usize = 4;
pub const BUS_MIN_MERGE_INPUTS: usize = 2;
/// Gradient is the widest type (40 floats).
const MAX_TYPE_CHANNELS: usize = GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS;

pub fn clamp_merge_inputs(n: usize) -> usize {
    n.clamp(BUS_MIN_MERGE_INPUTS, MAX_LANES)
}

pub fn clamp_lanes(n: usize) -> usize {
    n.clamp(1, MAX_LANES)
}

/// N inputs of one type, one output that is a bus of them.
pub fn merge_ports(inputs: usize, port_type: PortType) -> (Vec<PortDef>, Vec<PortDef>) {
    (
        (0..inputs).map(|i| PortDef::new(format!("in {}", i + 1), port_type)).collect(),
        vec![PortDef::new("bus", port_type).with_lanes(inputs)],
    )
}

/// One bus input, N outputs of its type.
pub fn split_ports(lanes: usize, port_type: PortType) -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![PortDef::new("bus", port_type).with_lanes(lanes)],
        (0..lanes).map(|i| PortDef::new(format!("out {}", i + 1), port_type)).collect(),
    )
}

pub fn pick_ports(lanes: usize, port_type: PortType) -> (Vec<PortDef>, Vec<PortDef>) {
    (
        vec![PortDef::new("bus", port_type).with_lanes(lanes)],
        vec![PortDef::new("out", port_type)],
    )
}

/// What the engine publishes for the widget: the type and width the node
/// settled on.
pub struct BusDisplay {
    pub port_type: PortType,
    pub count: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BusKind {
    Merge,
    Split,
}

/// Merge and Split: the same buffer seen through two port layouts.
pub struct BusReshapeProcessNode {
    id: NodeId,
    kind: BusKind,
    port_type: PortType,
    count: usize,
    values: Vec<f32>,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl BusReshapeProcessNode {
    pub fn merge(id: NodeId) -> Self {
        Self::new(id, BusKind::Merge)
    }

    pub fn split(id: NodeId) -> Self {
        Self::new(id, BusKind::Split)
    }

    fn new(id: NodeId, kind: BusKind) -> Self {
        let mut node = Self {
            id,
            kind,
            port_type: PortType::Any,
            count: BUS_DEFAULT_LANES,
            values: vec![0.0; MAX_LANES * MAX_TYPE_CHANNELS],
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        node.rebuild();
        node
    }

    fn rebuild(&mut self) {
        (self.inputs, self.outputs) = match self.kind {
            BusKind::Merge => merge_ports(self.count, self.port_type),
            BusKind::Split => split_ports(self.count, self.port_type),
        };
        self.values.iter_mut().for_each(|v| *v = 0.0);
    }
}

impl ProcessNode for BusReshapeProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str {
        match self.kind {
            BusKind::Merge => "Bus Merge",
            BusKind::Split => "Bus Split",
        }
    }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, channel: usize, v: f32) {
        if let Some(slot) = self.values.get_mut(channel) {
            *slot = v;
        }
    }
    fn read_input(&self, channel: usize) -> f32 {
        self.values.get(channel).copied().unwrap_or(0.0)
    }
    fn read_output(&self, channel: usize) -> f32 {
        self.values.get(channel).copied().unwrap_or(0.0)
    }

    fn process(&mut self) {}

    fn non_promotable_params(&self) -> &'static [&'static str] {
        &["Type", "Inputs", "Lanes"]
    }

    fn params(&self) -> Vec<ParamDef> {
        let (name, min) = match self.kind {
            BusKind::Merge => ("Inputs", BUS_MIN_MERGE_INPUTS),
            BusKind::Split => ("Lanes", 1),
        };
        vec![
            ParamDef::Choice {
                name: "Type".into(),
                value: type_to_index(self.port_type),
                options: MUX_TYPE_NAMES.iter().map(|s| (*s).into()).collect(),
            },
            ParamDef::Int { name: name.into(), value: self.count as i64, min: min as i64, max: MAX_LANES as i64 },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            0 => {
                let t = type_from_index(value.as_usize());
                if t != self.port_type {
                    self.port_type = t;
                    self.rebuild();
                }
            }
            1 => {
                let n = match self.kind {
                    BusKind::Merge => clamp_merge_inputs(value.as_usize()),
                    BusKind::Split => clamp_lanes(value.as_usize()),
                };
                if n != self.count {
                    self.count = n;
                    self.rebuild();
                }
            }
            _ => {}
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(BusDisplay { port_type: self.port_type, count: self.count }));
    }
}

pub const PICK_PARAM_TYPE: usize = 0;
pub const PICK_PARAM_LANES: usize = 1;
pub const PICK_PARAM_INDEX: usize = 2;

/// One lane of a bus, by index. The index is a setting, so a wire reaches it
/// as a promoted input like any other setting.
pub struct BusPickProcessNode {
    id: NodeId,
    port_type: PortType,
    lanes: usize,
    index: usize,
    values: Vec<f32>,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl BusPickProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mut node = Self {
            id,
            port_type: PortType::Any,
            lanes: BUS_DEFAULT_LANES,
            index: 0,
            values: vec![0.0; MAX_LANES * MAX_TYPE_CHANNELS],
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        node.rebuild();
        node
    }

    fn rebuild(&mut self) {
        (self.inputs, self.outputs) = pick_ports(self.lanes, self.port_type);
        self.values.iter_mut().for_each(|v| *v = 0.0);
    }

    fn lane_base(&self) -> usize {
        self.index.min(self.lanes - 1) * self.port_type.channel_count()
    }
}

impl ProcessNode for BusPickProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Bus Pick" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, channel: usize, v: f32) {
        if let Some(slot) = self.values.get_mut(channel) {
            *slot = v;
        }
    }
    fn read_input(&self, channel: usize) -> f32 {
        self.values.get(channel).copied().unwrap_or(0.0)
    }
    fn read_output(&self, channel: usize) -> f32 {
        self.values.get(self.lane_base() + channel).copied().unwrap_or(0.0)
    }

    fn process(&mut self) {}

    fn non_promotable_params(&self) -> &'static [&'static str] {
        &["Type", "Lanes"]
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![
            ParamDef::Choice {
                name: "Type".into(),
                value: type_to_index(self.port_type),
                options: MUX_TYPE_NAMES.iter().map(|s| (*s).into()).collect(),
            },
            ParamDef::Int { name: "Lanes".into(), value: self.lanes as i64, min: 1, max: MAX_LANES as i64 },
            ParamDef::Int { name: "Index".into(), value: self.index as i64, min: 0, max: MAX_LANES as i64 - 1 },
        ]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        match index {
            PICK_PARAM_TYPE => {
                let t = type_from_index(value.as_usize());
                if t != self.port_type {
                    self.port_type = t;
                    self.rebuild();
                }
            }
            PICK_PARAM_LANES => {
                let n = clamp_lanes(value.as_usize());
                if n != self.lanes {
                    self.lanes = n;
                    self.rebuild();
                }
            }
            PICK_PARAM_INDEX => self.index = value.as_i64().clamp(0, MAX_LANES as i64 - 1) as usize,
            _ => {}
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(BusDisplay { port_type: self.port_type, count: self.lanes }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_has_one_bus_output_as_wide_as_its_inputs() {
        let n = BusReshapeProcessNode::merge(NodeId(1));
        assert_eq!(n.inputs().len(), BUS_DEFAULT_LANES);
        assert_eq!(n.outputs().len(), 1);
        assert_eq!(n.outputs()[0].lanes, BUS_DEFAULT_LANES);
        assert_eq!(total_channels(n.outputs()), BUS_DEFAULT_LANES);
    }

    #[test]
    fn a_bus_of_colors_is_three_floats_per_lane() {
        let mut n = BusReshapeProcessNode::merge(NodeId(1));
        n.set_param(0, ParamValue::Choice(type_to_index(PortType::Color)));
        assert_eq!(total_channels(n.outputs()), BUS_DEFAULT_LANES * 3);
        n.write_input(port_base_index(n.inputs(), 2) + 1, 0.5);
        assert_eq!(n.read_output(2 * 3 + 1), 0.5);
    }

    #[test]
    fn pick_reads_the_lane_the_index_names_and_stops_at_the_last() {
        let mut n = BusPickProcessNode::new(NodeId(1));
        for lane in 0..BUS_DEFAULT_LANES {
            n.write_input(lane, lane as f32 + 10.0);
        }
        n.set_param(PICK_PARAM_INDEX, ParamValue::Int(2));
        assert_eq!(n.read_output(0), 12.0);
        n.set_param(PICK_PARAM_INDEX, ParamValue::Int(50));
        assert_eq!(n.read_output(0), 13.0);
    }

    #[test]
    fn pick_of_colors_returns_all_three_channels_of_one_lane() {
        let mut n = BusPickProcessNode::new(NodeId(1));
        n.set_param(PICK_PARAM_TYPE, ParamValue::Choice(type_to_index(PortType::Color)));
        n.set_param(PICK_PARAM_INDEX, ParamValue::Int(1));
        for ch in 0..3 {
            n.write_input(3 + ch, 0.1 * (ch + 1) as f32);
        }
        assert_eq!([n.read_output(0), n.read_output(1), n.read_output(2)], [0.1, 0.2, 0.3]);
    }
}
