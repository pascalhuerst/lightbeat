use super::signal_type::{MAX_CHANNELS, publish_type, type_from_param, type_param};
use crate::engine::types::*;

/// Tick Delay: emits the previous tick's input value. Adds exactly one tick of
/// delay on top of the engine's natural per-connection delay.
///
/// Use this in feedback paths to make the data flow explicit (and
/// independent of any future scheduling changes). Any signal type works; each
/// channel is delayed alike.
pub struct LatchProcessNode {
    id: NodeId,
    port_type: PortType,
    /// Buffered input from this tick's propagation phase.
    pending_in: [f32; MAX_CHANNELS],
    /// What read_output emits — captured from `pending_in` in `process()`.
    held: [f32; MAX_CHANNELS],
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports_for(port_type: PortType) -> (Vec<PortDef>, Vec<PortDef>) {
    (vec![PortDef::new("in", port_type)], vec![PortDef::new("out", port_type)])
}

impl LatchProcessNode {
    pub fn new(id: NodeId) -> Self {
        let mut node = Self {
            id,
            port_type: PortType::Any,
            pending_in: [0.0; MAX_CHANNELS],
            held: [0.0; MAX_CHANNELS],
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        node.rebuild();
        node
    }

    fn rebuild(&mut self) {
        (self.inputs, self.outputs) = ports_for(self.port_type);
        self.pending_in = [0.0; MAX_CHANNELS];
        self.held = [0.0; MAX_CHANNELS];
    }

    fn width(&self) -> usize { self.port_type.channel_count() }
}

impl ProcessNode for LatchProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Tick Delay" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, ch: usize, v: f32) {
        if ch < self.width() { self.pending_in[ch] = v; }
    }
    fn read_input(&self, ch: usize) -> f32 {
        if ch < self.width() { self.pending_in[ch] } else { 0.0 }
    }

    fn process(&mut self) {
        // Anything written this tick will be visible next tick.
        self.held = self.pending_in;
    }

    fn read_output(&self, ch: usize) -> f32 {
        if ch < self.width() { self.held[ch] } else { 0.0 }
    }

    // The type reshapes the ports.
    fn non_promotable_params(&self) -> &'static [&'static str] { &["Type"] }

    fn params(&self) -> Vec<ParamDef> { vec![type_param(self.port_type)] }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 {
            let pt = type_from_param(&value);
            if pt != self.port_type {
                self.port_type = pt;
                self.rebuild();
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        publish_type(shared, self.port_type);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_comes_out_one_tick_late() {
        let mut n = LatchProcessNode::new(NodeId(1));
        n.set_param(0, ParamValue::Choice(5));
        assert_eq!(n.outputs()[0].port_type, PortType::Position);
        n.write_input(0, 0.25);
        n.write_input(1, 0.75);
        assert_eq!(n.read_output(1), 0.0);
        n.process();
        assert_eq!((n.read_output(0), n.read_output(1)), (0.25, 0.75));
    }
}
