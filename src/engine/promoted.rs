//! Params exposed as input ports on demand.
//!
//! The user can promote any param of any node to an input in the inspector.
//! The node itself knows nothing about this: the graph owns the extra ports,
//! which come after the node's native inputs, and turns each incoming value
//! into a `set_param` call. Both `EngineGraph` and `InnerGraph` use this.

use serde::{Deserialize, Serialize};

use super::types::{ParamDef, ParamValue, PortType, ProcessNode};

/// What kind of value a promoted param takes, and so what its port carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromotedKind {
    Float,
    Int,
    Bool,
    Choice,
}

impl PromotedKind {
    pub fn of(def: &ParamDef) -> Self {
        match def {
            ParamDef::Float { .. } => PromotedKind::Float,
            ParamDef::Int { .. } => PromotedKind::Int,
            ParamDef::Bool { .. } => PromotedKind::Bool,
            ParamDef::Choice { .. } => PromotedKind::Choice,
        }
    }

    /// Choice reads a 0..1 signal spread across its options, so a phase or a
    /// fader can select any of them.
    pub fn port_type(self) -> PortType {
        match self {
            PromotedKind::Float | PromotedKind::Int => PortType::Untyped,
            PromotedKind::Bool => PortType::Logic,
            PromotedKind::Choice => PortType::Phase,
        }
    }
}

/// A promoted param as the UI stores and saves it. Referenced by name so
/// saved projects survive a node reordering its params. The list's order is
/// the order of the extra ports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotedParam {
    pub name: String,
    pub kind: PromotedKind,
}

#[derive(Debug, Clone, Copy)]
enum Spec {
    Float { min: f32, max: f32 },
    Int { min: i64, max: i64 },
    Bool,
    Choice { options: usize },
}

impl Spec {
    fn of(def: &ParamDef) -> Self {
        match def {
            ParamDef::Float { min, max, .. } => Spec::Float { min: *min, max: *max },
            ParamDef::Int { min, max, .. } => Spec::Int { min: *min, max: *max },
            ParamDef::Bool { .. } => Spec::Bool,
            ParamDef::Choice { options, .. } => Spec::Choice { options: options.len() },
        }
    }

    fn convert(self, v: f32) -> ParamValue {
        match self {
            Spec::Float { min, max } => ParamValue::Float(v.clamp(min, max)),
            Spec::Int { min, max } => ParamValue::Int((v.round() as i64).clamp(min, max)),
            Spec::Bool => ParamValue::Bool(v >= 0.5),
            Spec::Choice { options } => {
                let last = options.saturating_sub(1);
                ParamValue::Choice(((v.clamp(0.0, 1.0) * options as f32) as usize).min(last))
            }
        }
    }
}

struct Entry {
    param_idx: usize,
    spec: Spec,
    /// Last value pushed, so `set_param` (which some nodes use to reset
    /// state) only runs when the signal actually moves a param.
    last: Option<ParamValue>,
    was_wired: bool,
}

/// Engine-side promoted ports of one node, in port order.
#[derive(Default)]
pub struct PromotedInputs {
    wanted: Vec<PromotedParam>,
    /// `None` where the node no longer has a param of that name; its port
    /// still exists so wire indices stay put.
    entries: Vec<Option<Entry>>,
}

impl PromotedInputs {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Adopt the UI's list, looking each name up in the node's current params.
    pub fn adopt(&mut self, wanted: &[PromotedParam], node: &dyn ProcessNode) {
        let params = node.params();
        self.wanted = wanted.to_vec();
        self.entries = wanted
            .iter()
            .map(|w| {
                let idx = params.iter().position(|p| p.name() == w.name)?;
                Some(Entry {
                    param_idx: idx,
                    spec: Spec::of(&params[idx]),
                    last: None,
                    was_wired: false,
                })
            })
            .collect();
    }

    /// Record which promoted ports have a wire this tick (`wired[slot]`).
    /// A param edited by hand while unwired may differ from `last`, so the
    /// cache is dropped whenever a wire comes (back).
    pub fn note_wiring(&mut self, wired: &[bool]) {
        for (entry, &w) in self.entries.iter_mut().zip(wired) {
            if let Some(e) = entry {
                if w && !e.was_wired {
                    e.last = None;
                }
                e.was_wired = w;
            }
        }
    }

    /// Feed one tick's value for promoted port `slot`.
    pub fn apply(&mut self, node: &mut dyn ProcessNode, slot: usize, value: f32) {
        let Some(Some(entry)) = self.entries.get_mut(slot) else { return };
        let converted = entry.spec.convert(value);
        let unchanged = match (&entry.last, &converted) {
            (Some(ParamValue::Float(a)), ParamValue::Float(b)) => a == b,
            (Some(ParamValue::Int(a)), ParamValue::Int(b)) => a == b,
            (Some(ParamValue::Bool(a)), ParamValue::Bool(b)) => a == b,
            (Some(ParamValue::Choice(a)), ParamValue::Choice(b)) => a == b,
            _ => false,
        };
        if !unchanged {
            entry.last = Some(converted.clone());
            node.set_param(entry.param_idx, converted);
        }
    }

    /// Re-read ranges and indices, which can change with a node's mode.
    /// Called at UI sync rate rather than every tick since `params()` allocates.
    pub fn refresh(&mut self, node: &dyn ProcessNode) {
        if self.wanted.is_empty() {
            return;
        }
        let params = node.params();
        for (entry, w) in self.entries.iter_mut().zip(&self.wanted) {
            if let Some(e) = entry
                && let Some(idx) = params.iter().position(|p| p.name() == w.name)
            {
                e.param_idx = idx;
                e.spec = Spec::of(&params[idx]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_is_clamped_to_its_range() {
        let spec = Spec::Float { min: 0.0, max: 2.0 };
        assert!(matches!(spec.convert(5.0), ParamValue::Float(v) if v == 2.0));
        assert!(matches!(spec.convert(-1.0), ParamValue::Float(v) if v == 0.0));
    }

    #[test]
    fn int_rounds_and_clamps() {
        let spec = Spec::Int { min: -6, max: 6 };
        assert!(matches!(spec.convert(2.6), ParamValue::Int(3)));
        assert!(matches!(spec.convert(99.0), ParamValue::Int(6)));
    }

    #[test]
    fn bool_switches_at_half() {
        assert!(matches!(Spec::Bool.convert(0.49), ParamValue::Bool(false)));
        assert!(matches!(Spec::Bool.convert(0.5), ParamValue::Bool(true)));
    }

    #[test]
    fn choice_spreads_zero_to_one_over_the_options() {
        let spec = Spec::Choice { options: 5 };
        let pick = |v| match spec.convert(v) {
            ParamValue::Choice(i) => i,
            _ => unreachable!(),
        };
        assert_eq!(pick(0.0), 0);
        assert_eq!(pick(0.5), 2);
        assert_eq!(pick(0.99), 4);
        // The top of the range must not run past the last option.
        assert_eq!(pick(1.0), 4);
        assert_eq!(pick(3.0), 4);
    }
}
