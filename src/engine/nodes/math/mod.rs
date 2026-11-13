pub mod bipolar;
pub mod change_detect;
pub mod color_modifier;
pub mod color_components;
pub mod color_reshape;
pub mod counter;
pub mod compare;
pub mod constant;
pub mod flipflop;
pub mod formula;
pub mod gain;
pub mod scale_offset;
pub mod gradient_source;
pub mod logic_gate;
pub mod lookup;
pub mod math_op;
pub mod multiplex;
pub mod palette_select;
pub mod palette_to_gradient;
pub mod phase_accumulator;
pub mod port_hub;
pub mod random;
pub mod position_ops;
pub mod scaler;
pub mod schmitt;
pub mod toggle_bank;
pub mod trigger_bank;

#[cfg(test)]
mod operation_switch_tests {
    use super::math_op::{MathOp, MathProcessNode};
    use super::compare::{CompareOp, CompareProcessNode};
    use super::constant::ConstantProcessNode;
    use super::logic_gate::{LogicGateProcessNode, LogicOp};
    use crate::engine::types::*;

    #[test]
    fn a_math_node_becomes_its_sibling_and_keeps_its_inputs() {
        let mut n = MathProcessNode::new(NodeId(1), MathOp::Add);
        n.write_input(0, 6.0);
        n.write_input(1, 3.0);
        n.process();
        assert_eq!(n.read_output(0), 9.0);
        n.set_param(0, ParamValue::Choice(3));
        n.process();
        assert_eq!(n.type_name(), "Div");
        assert_eq!(n.read_output(0), 2.0);
        assert_eq!(n.inputs().len(), 2);
    }

    #[test]
    fn a_comparison_becomes_its_sibling() {
        let mut n = CompareProcessNode::new(NodeId(1), CompareOp::Gte);
        n.write_input(0, 1.0);
        n.write_input(1, 2.0);
        n.process();
        assert_eq!(n.read_output(0), 0.0);
        n.set_param(0, ParamValue::Choice(CompareOp::Lte.to_index()));
        n.process();
        assert_eq!(n.type_name(), "<=");
        assert_eq!(n.read_output(0), 1.0);
    }

    #[test]
    fn the_gates_switch_among_themselves_but_not_is_left_out() {
        let mut n = LogicGateProcessNode::new(NodeId(1), LogicOp::And);
        n.write_input(0, 1.0);
        n.write_input(1, 0.0);
        n.process();
        assert_eq!(n.read_output(0), 0.0);
        n.set_param(0, ParamValue::Choice(1));
        n.process();
        assert_eq!(n.type_name(), "OR");
        assert_eq!(n.read_output(0), 1.0);
        let mut not = LogicGateProcessNode::new(NodeId(2), LogicOp::Not);
        assert!(not.params().is_empty());
        not.set_param(0, ParamValue::Choice(1));
        assert_eq!(not.type_name(), "NOT");
    }

    #[test]
    fn a_constant_changes_the_type_of_its_output_and_keeps_its_value_in_range() {
        let mut c = ConstantProcessNode::new(NodeId(1), PortType::Untyped, 0.0);
        c.set_param(0, ParamValue::Float(5.0));
        c.set_param(1, ParamValue::Choice(2));
        assert_eq!(c.type_name(), "Const Phase");
        assert_eq!(c.outputs()[0].port_type, PortType::Phase);
        assert_eq!(c.read_output(0), 1.0);
        c.set_param(1, ParamValue::Choice(1));
        assert_eq!(c.type_name(), "Const Logic");
    }
}
