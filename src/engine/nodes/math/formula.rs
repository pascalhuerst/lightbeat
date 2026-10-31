use meval::{ContextProvider, Expr, FuncEvalError};

use crate::engine::types::*;

/// What the widget shows.
pub struct FormulaDisplay {
    pub formula: String,
    /// Why the formula cannot be used, if it cannot.
    pub error: Option<String>,
}

/// Names accepted before an `=` in front of the formula (`y = 2*x`).
const OUTPUT_NAMES: [&str; 3] = ["y", "o", "out"];

/// The expression of a formula, without an `y =` / `o =` in front of it.
fn expression_of(formula: &str) -> &str {
    let formula = formula.trim();
    if let Some((lhs, rhs)) = formula.split_once('=')
        && OUTPUT_NAMES.contains(&lhs.trim().to_ascii_lowercase().as_str())
        && !rhs.starts_with('=')
    {
        return rhs.trim();
    }
    formula
}

/// What a formula can refer to: the input as `x` or `i`, the constants, and
/// the functions. (`meval` parses; it brings its own functions, but a context
/// of its own is not `Send`, so they are listed here.)
struct Env {
    input: f64,
}

impl ContextProvider for Env {
    fn get_var(&self, name: &str) -> Option<f64> {
        use std::f64::consts::{E, PI, TAU};
        match name {
            "x" | "i" => Some(self.input),
            "PI" | "pi" => Some(PI),
            "TAU" | "tau" => Some(TAU),
            "E" | "e" => Some(E),
            _ => None,
        }
    }

    fn eval_func(&self, name: &str, args: &[f64]) -> Result<f64, FuncEvalError> {
        let one: Option<fn(f64) -> f64> = match name {
            "sin" => Some(f64::sin),
            "cos" => Some(f64::cos),
            "tan" => Some(f64::tan),
            "asin" => Some(f64::asin),
            "acos" => Some(f64::acos),
            "atan" => Some(f64::atan),
            "sinh" => Some(f64::sinh),
            "cosh" => Some(f64::cosh),
            "tanh" => Some(f64::tanh),
            "sqrt" => Some(f64::sqrt),
            "cbrt" => Some(f64::cbrt),
            "exp" => Some(f64::exp),
            "ln" => Some(f64::ln),
            "log2" => Some(f64::log2),
            "log10" => Some(f64::log10),
            "abs" => Some(f64::abs),
            "signum" => Some(f64::signum),
            "floor" => Some(f64::floor),
            "ceil" => Some(f64::ceil),
            "round" => Some(f64::round),
            "trunc" => Some(f64::trunc),
            "fract" => Some(f64::fract),
            _ => None,
        };
        if let Some(f) = one {
            return match args {
                [a] => Ok(f(*a)),
                _ => Err(FuncEvalError::NumberArgs(1)),
            };
        }
        match (name, args) {
            ("atan2", [a, b]) => Ok(a.atan2(*b)),
            ("atan2", _) => Err(FuncEvalError::NumberArgs(2)),
            ("pow", [a, b]) => Ok(a.powf(*b)),
            ("pow", _) => Err(FuncEvalError::NumberArgs(2)),
            ("clamp", [a, lo, hi]) => Ok(a.max(*lo).min(*hi)),
            ("clamp", _) => Err(FuncEvalError::NumberArgs(3)),
            ("max", [first, rest @ ..]) => Ok(rest.iter().fold(*first, |m, v| m.max(*v))),
            ("min", [first, rest @ ..]) => Ok(rest.iter().fold(*first, |m, v| m.min(*v))),
            ("max" | "min", []) => Err(FuncEvalError::TooFewArguments),
            _ => Err(FuncEvalError::UnknownFunction),
        }
    }
}

/// Parse a formula and make sure it only uses names that exist, so a typo is
/// reported when it is typed and not on every tick.
fn compile(formula: &str) -> Result<Expr, String> {
    let source = expression_of(formula);
    if source.is_empty() {
        return Err("empty formula".to_string());
    }
    let expr: Expr = source.parse().map_err(|e: meval::Error| e.to_string())?;
    match expr.eval_with_context(Env { input: 0.5 }) {
        Err(meval::Error::UnknownVariable(name)) => {
            Err(format!("unknown name '{name}': the input is x (or i)"))
        }
        Err(meval::Error::Function(name, FuncEvalError::UnknownFunction)) => {
            Err(format!("unknown function '{name}'"))
        }
        Err(meval::Error::Function(name, FuncEvalError::NumberArgs(n))) => {
            Err(format!("{name} takes {n} argument(s)"))
        }
        Err(meval::Error::Function(name, _)) => Err(format!("wrong number of arguments to {name}")),
        Err(other) => Err(other.to_string()),
        Ok(_) => Ok(expr),
    }
}

/// Formula. Writes a function of its input to its output, for example
/// `y = 2*x + 7` or `sin(x * 2 * PI)`.
pub struct FormulaProcessNode {
    id: NodeId,
    formula: String,
    compiled: Result<Expr, String>,
    input: f32,
    out: f32,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl FormulaProcessNode {
    pub fn new(id: NodeId) -> Self {
        let formula = "x".to_string();
        Self {
            id,
            compiled: compile(&formula),
            formula,
            input: 0.0,
            out: 0.0,
            inputs: vec![PortDef::new("x", PortType::Untyped)],
            outputs: vec![PortDef::new("y", PortType::Untyped)],
        }
    }

    fn set_formula(&mut self, formula: &str) {
        self.formula = formula.to_string();
        self.compiled = compile(formula);
    }
}

impl ProcessNode for FormulaProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Formula" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, pi: usize, v: f32) {
        if pi == 0 { self.input = v; }
    }
    fn read_input(&self, pi: usize) -> f32 {
        if pi == 0 { self.input } else { 0.0 }
    }

    fn process(&mut self) {
        let Ok(expr) = &self.compiled else {
            self.out = 0.0;
            return;
        };
        let value = expr.eval_with_context(Env { input: f64::from(self.input) }).unwrap_or(0.0);
        // A result that is not a number (the log of a negative, a division by
        // zero) would poison everything downstream.
        self.out = if value.is_finite() { value as f32 } else { 0.0 };
    }

    fn read_output(&self, pi: usize) -> f32 {
        if pi == 0 { self.out } else { 0.0 }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "formula": self.formula }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(f) = data.get("formula").and_then(|v| v.as_str()) {
            self.set_formula(f);
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(FormulaDisplay {
            formula: self.formula.clone(),
            error: self.compiled.as_ref().err().cloned(),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(formula: &str, x: f32) -> f32 {
        let mut n = FormulaProcessNode::new(NodeId(1));
        n.load_data(&serde_json::json!({ "formula": formula }));
        n.write_input(0, x);
        n.process();
        n.read_output(0)
    }

    #[test]
    fn it_evaluates_a_formula_of_the_input() {
        assert_eq!(eval("2*x+7", 3.0), 13.0);
        assert_eq!(eval("x", 0.25), 0.25, "the default passes the input through");
        assert!((eval("sin(x*2*PI)", 0.25) - 1.0).abs() < 1e-6);
        assert_eq!(eval("x^2", 3.0), 9.0);
        assert_eq!(eval("x % 4", 7.0), 3.0);
        assert_eq!(eval("clamp(x, 0, 1) + log10(1000)", 5.0), 4.0);
        assert_eq!(eval("min(max(x, 0), 1)", 1.7), 1.0);
        assert_eq!(eval("abs(x) + sqrt(16) + floor(2.7)", -1.0), 7.0);
        assert!((eval("atan2(1, 1) * 4", 0.0) - std::f32::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn the_input_can_be_called_x_or_i_and_the_output_y_or_o() {
        assert_eq!(eval("y = 2*x+7", 3.0), 13.0);
        assert_eq!(eval("o=2*i+7", 3.0), 13.0);
        assert_eq!(eval("out = x + i", 3.0), 6.0);
        assert!((eval("o=sin(i*2*PI)", 0.25) - 1.0).abs() < 1e-6);
        assert_eq!(eval("x * i", 3.0), 9.0);
    }

    #[test]
    fn a_broken_formula_says_why_and_outputs_zero() {
        for (formula, why) in [("2*", ""), ("foo + 1", "unknown name 'foo'"), ("", "empty"), ("y = ", "empty")] {
            let c = compile(formula);
            let Err(msg) = c else { panic!("{formula:?} should not compile") };
            assert!(msg.contains(why), "{formula:?}: {msg}");
            assert_eq!(eval(formula, 5.0), 0.0);
        }
        for (formula, why) in [("frob(x)", "unknown function 'frob'"), ("sin(x, 2)", "sin takes 1"), ("clamp(x)", "clamp takes 3")] {
            let Err(msg) = compile(formula) else { panic!("{formula:?} should not compile") };
            assert!(msg.contains(why), "{formula:?}: {msg}");
        }
        assert_eq!(eval("ln(x)", -1.0), 0.0, "not a number becomes zero");
        assert_eq!(eval("1/x", 0.0), 0.0, "infinity too");
    }

    #[test]
    fn the_formula_is_saved() {
        let mut n = FormulaProcessNode::new(NodeId(1));
        n.load_data(&serde_json::json!({ "formula": "x*3" }));
        assert_eq!(n.save_data().unwrap()["formula"], "x*3");
    }
}
