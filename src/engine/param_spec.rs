//! Plain numeric params described once. A node keeps its specs as consts and
//! uses each for both halves of the pair: `def` builds the `ParamDef` for
//! `params()`, `apply` reads a `ParamValue` back for `set_param()`, with the
//! clamp the node always had. Saved settings go by index and snapshots by
//! name, so a spec is only ever a more compact way to say the same thing.

use super::types::{ParamDef, ParamValue};

/// A Float param. `lo` / `hi` are what `apply` clamps to; many nodes accept
/// values beyond the slider range (a wired or loaded value), so the clamp is
/// opt-in and separate from `min` / `max`.
#[derive(Clone, Copy)]
pub struct FloatParam {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub unit: &'static str,
    lo: Option<f32>,
    hi: Option<f32>,
}

impl FloatParam {
    /// A param whose `apply` passes the value through unchanged.
    pub const fn new(name: &'static str, min: f32, max: f32, step: f32, unit: &'static str) -> Self {
        Self { name, min, max, step, unit, lo: None, hi: None }
    }

    /// Clamp to the slider range.
    pub const fn clamped(mut self) -> Self {
        self.lo = Some(self.min);
        self.hi = Some(self.max);
        self
    }

    /// Clamp from below only.
    pub const fn at_least(mut self, lo: f32) -> Self {
        self.lo = Some(lo);
        self
    }

    pub fn def(&self, value: f32) -> ParamDef {
        ParamDef::Float { name: self.name.into(), value, min: self.min, max: self.max, step: self.step, unit: self.unit }
    }

    pub fn apply(&self, value: &ParamValue) -> f32 {
        let v = value.as_f32();
        match (self.lo, self.hi) {
            (Some(lo), Some(hi)) => v.clamp(lo, hi),
            (Some(lo), None) => v.max(lo),
            (None, Some(hi)) => v.min(hi),
            (None, None) => v,
        }
    }
}

/// An Int param, with the same opt-in clamp.
#[derive(Clone, Copy)]
pub struct IntParam {
    pub name: &'static str,
    pub min: i64,
    pub max: i64,
    lo: Option<i64>,
    hi: Option<i64>,
}

impl IntParam {
    pub const fn new(name: &'static str, min: i64, max: i64) -> Self {
        Self { name, min, max, lo: None, hi: None }
    }

    pub const fn at_least(mut self, lo: i64) -> Self {
        self.lo = Some(lo);
        self
    }

    pub fn def(&self, value: i64) -> ParamDef {
        ParamDef::Int { name: self.name.into(), value, min: self.min, max: self.max }
    }

    pub fn apply(&self, value: &ParamValue) -> i64 {
        let v = value.as_i64();
        match (self.lo, self.hi) {
            (Some(lo), Some(hi)) => v.clamp(lo, hi),
            (Some(lo), None) => v.max(lo),
            (None, Some(hi)) => v.min(hi),
            (None, None) => v,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn def_carries_the_range_and_apply_only_clamps_when_asked() {
        const OPEN: FloatParam = FloatParam::new("A", -1.0, 1.0, 0.1, "x");
        const TIGHT: FloatParam = FloatParam::new("B", 0.0, 1.0, 0.1, "").clamped();
        const FLOOR: FloatParam = FloatParam::new("C", 0.1, 5.0, 0.1, "").at_least(0.0);
        assert_eq!(OPEN.apply(&ParamValue::Float(5.0)), 5.0);
        assert_eq!(TIGHT.apply(&ParamValue::Float(5.0)), 1.0);
        assert_eq!(FLOOR.apply(&ParamValue::Float(-2.0)), 0.0);
        assert_eq!(FLOOR.apply(&ParamValue::Float(9.0)), 9.0);
        match OPEN.def(0.5) {
            ParamDef::Float { name, value, min, max, step, unit } => {
                assert_eq!((name.as_ref(), value, min, max, step, unit), ("A", 0.5, -1.0, 1.0, 0.1, "x"));
            }
            _ => panic!("not a float"),
        }
    }

    #[test]
    fn an_int_param_converts_and_clamps() {
        const N: IntParam = IntParam::new("N", 1, 64).at_least(1);
        assert_eq!(N.apply(&ParamValue::Int(-3)), 1);
        assert_eq!(N.apply(&ParamValue::Float(7.0)), 7);
        assert!(matches!(N.def(4), ParamDef::Int { value: 4, min: 1, max: 64, .. }));
    }
}
