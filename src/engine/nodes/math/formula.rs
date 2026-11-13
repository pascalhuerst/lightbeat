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
    fn operators_keep_their_precedence_and_associativity() {
        assert_eq!(eval("1 + 2 * 3", 0.0), 7.0);
        assert_eq!(eval("(1 + 2) * 3", 0.0), 9.0);
        assert_eq!(eval("10 - 4 - 3", 0.0), 3.0, "minus is left associative");
        assert_eq!(eval("64 / 8 / 2", 0.0), 4.0, "divide is left associative");
        assert_eq!(eval("2 ^ 3 ^ 2", 0.0), 512.0, "power is right associative");
        assert_eq!(eval("2 * 3 ^ 2", 0.0), 18.0, "power binds tighter than times");
        assert_eq!(eval("7 % 4 * 2", 0.0), 6.0, "rem and times are equals, left to right");
        assert_eq!(eval("1 + 7 % 4", 0.0), 4.0);
        assert_eq!(eval("1/2", 0.0), 0.5, "literals are floats");
        assert_eq!(eval("-7 % 4", 0.0), -3.0, "rem keeps the sign of the left side");
        assert_eq!(eval("7.5 % 2", 0.0), 1.5);
    }

    #[test]
    fn unary_signs_bind_between_times_and_power() {
        assert_eq!(eval("-x", 3.0), -3.0);
        assert_eq!(eval("--x", 3.0), 3.0);
        assert_eq!(eval("+x", 3.0), 3.0);
        assert_eq!(eval("-x^2", 3.0), -9.0, "power first, then the sign");
        assert_eq!(eval("(-x)^2", 3.0), 9.0);
        assert_eq!(eval("2^-x", 3.0), 0.125);
        assert_eq!(eval("2^-x^2", 1.0), 0.5, "the sign covers the whole power on its right");
        assert_eq!(eval("2*-x", 3.0), -6.0);
        assert_eq!(eval("5 - -x", 3.0), 8.0);
        assert_eq!(eval("-2*3", 0.0), -6.0);
        assert_eq!(eval("-x + 1", 3.0), -2.0);
        assert_eq!(eval("-(x + 1)", 3.0), -4.0);
        assert_eq!(eval("-abs(x)", 3.0), -3.0);
    }

    #[test]
    fn numbers_are_read_like_before() {
        assert_eq!(eval("1.", 0.0), 1.0);
        assert_eq!(eval("1.5e2", 0.0), 150.0);
        assert_eq!(eval("2E-2", 0.0), 0.02);
        assert_eq!(eval("1e+1", 0.0), 10.0);
        assert_eq!(eval("0.25 * 4", 0.0), 1.0);
        assert_eq!(eval("007", 0.0), 7.0);
        assert!(compile(".5").is_err(), "a number needs a digit before the point");
        assert!(compile("1e").is_err());
        assert!(compile("1.2.3").is_err());
        assert!(compile("2x").is_err(), "no implicit multiplication");
    }

    #[test]
    fn whitespace_is_free_between_tokens() {
        assert_eq!(eval("  2 *\t( x\n+ 1 )  ", 3.0), 8.0);
        assert_eq!(eval("max (1, 2)", 0.0), 2.0, "a space before the bracket of a call");
        assert_eq!(eval("max(  1 ,2  )", 0.0), 2.0);
        assert_eq!(eval("y=x", 2.0), 2.0);
        assert_eq!(eval("Y = x", 2.0), 2.0, "the output name is not case sensitive");
    }

    #[test]
    fn constants_have_both_spellings_and_are_case_sensitive() {
        use std::f32::consts::{E, PI, TAU};
        assert!((eval("PI", 0.0) - PI).abs() < 1e-6);
        assert!((eval("pi", 0.0) - PI).abs() < 1e-6);
        assert!((eval("TAU", 0.0) - TAU).abs() < 1e-6);
        assert!((eval("tau", 0.0) - TAU).abs() < 1e-6);
        assert!((eval("E", 0.0) - E).abs() < 1e-6);
        assert!((eval("e", 0.0) - E).abs() < 1e-6);
        assert!((eval("e^1", 0.0) - E).abs() < 1e-6);
        assert_eq!(eval("2e", 0.0), 0.0, "2e is not a product");
        let Err(msg) = compile("Pi") else { panic!("Pi is not a constant") };
        assert!(msg.contains("unknown name 'Pi'"), "{msg}");
        let Err(msg) = compile("X") else { panic!("X is not the input") };
        assert!(msg.contains("unknown name 'X'"), "{msg}");
    }

    #[test]
    fn every_function_means_what_it_says() {
        let one: [(&str, f64, f64); 22] = [
            ("sin", 0.5, 0.5f64.sin()),
            ("cos", 0.5, 0.5f64.cos()),
            ("tan", 0.5, 0.5f64.tan()),
            ("asin", 0.5, 0.5f64.asin()),
            ("acos", 0.5, 0.5f64.acos()),
            ("atan", 0.5, 0.5f64.atan()),
            ("sinh", 0.5, 0.5f64.sinh()),
            ("cosh", 0.5, 0.5f64.cosh()),
            ("tanh", 0.5, 0.5f64.tanh()),
            ("sqrt", 2.25, 1.5),
            ("cbrt", 27.0, 3.0),
            ("exp", 1.0, std::f64::consts::E),
            ("ln", 8.0, 8f64.ln()),
            ("log2", 8.0, 3.0),
            ("log10", 1000.0, 3.0),
            ("abs", -2.5, 2.5),
            ("signum", -2.5, -1.0),
            ("floor", -2.5, -3.0),
            ("ceil", -2.5, -2.0),
            ("round", -2.5, -3.0),
            ("trunc", -2.5, -2.0),
            ("fract", -2.5, -0.5),
        ];
        for (name, arg, want) in one {
            let got = eval(&format!("{name}(x)"), arg as f32);
            assert!((f64::from(got) - want).abs() < 1e-5, "{name}({arg}) = {got}, want {want}");
        }
        assert_eq!(eval("signum(0)", 0.0), 1.0, "signum of zero follows f64::signum");
        assert_eq!(eval("round(2.5)", 0.0), 3.0);
        assert!((eval("atan2(1, 2)", 0.0) - 1f32.atan2(2.0)).abs() < 1e-6);
        assert_eq!(eval("pow(2, 10)", 0.0), 1024.0);
        assert_eq!(eval("pow(x, 0.5)", 16.0), 4.0);
        assert_eq!(eval("clamp(x, 0, 1)", -3.0), 0.0);
        assert_eq!(eval("clamp(x, 0, 1)", 0.5), 0.5);
        assert_eq!(eval("clamp(x, 0, 1)", 3.0), 1.0);
        assert_eq!(eval("max(1)", 0.0), 1.0);
        assert_eq!(eval("max(1, 5, 3)", 0.0), 5.0);
        assert_eq!(eval("min(4, 2, 3, 9)", 0.0), 2.0);
        assert_eq!(eval("max(min(x, 2), -1) * 2", 7.0), 4.0, "calls nest and take expressions");
        assert_eq!(eval("sqrt(abs(x)) ^ 2", -9.0), 9.0);
    }

    #[test]
    fn functions_check_their_argument_counts() {
        for (formula, why) in [
            ("sin()", ""),
            ("atan2(1)", "atan2 takes 2"),
            ("atan2(1, 2, 3)", "atan2 takes 2"),
            ("pow(1)", "pow takes 2"),
            ("clamp(1, 2, 3, 4)", "clamp takes 3"),
            ("abs(1, 2)", "abs takes 1"),
            ("max()", ""),
            ("min()", ""),
            ("max(1,)", ""),
            ("max(,1)", ""),
        ] {
            let Err(msg) = compile(formula) else { panic!("{formula:?} should not compile") };
            assert!(msg.contains(why), "{formula:?}: {msg}");
            assert!(!msg.is_empty());
        }
    }

    #[test]
    fn names_are_case_sensitive_and_checked_in_evaluation_order() {
        let Err(msg) = compile("SIN(x)") else { panic!("SIN is not sin") };
        assert!(msg.contains("unknown function 'SIN'"), "{msg}");
        let Err(msg) = compile("frob(foo)") else { panic!("should not compile") };
        assert!(msg.contains("unknown name 'foo'"), "arguments are looked at first: {msg}");
        let Err(msg) = compile("a + b") else { panic!("should not compile") };
        assert!(msg.contains("unknown name 'a'"), "left to right: {msg}");
        let Err(msg) = compile("x + frob(1) + foo") else { panic!("should not compile") };
        assert!(msg.contains("unknown function 'frob'"), "left to right: {msg}");
        let Err(msg) = compile("foo + (") else { panic!("should not compile") };
        assert!(!msg.contains("unknown name"), "a syntax error is found before a name: {msg}");
        let Err(msg) = compile("x (2)") else { panic!("should not compile") };
        assert!(msg.contains("unknown function 'x'"), "a name before a bracket is a call: {msg}");
        assert_eq!(eval("_a", 1.0), 0.0);
        let Err(msg) = compile("x_1") else { panic!("should not compile") };
        assert!(msg.contains("unknown name 'x_1'"), "{msg}");
    }

    #[test]
    fn syntax_errors_are_errors() {
        for formula in [
            "2*", "*2", "(", ")", "(1", "1)", "()", "1 2", "x x", "1 +* 2", "2 ** 3", "sin(", "sin(x", "sin x",
            "1,2", "max(1 2)", "(1,2)", "x!", "$", "1 = 2", "x = = 2", "2(3)", "x y = 3", "3 = x", "+", "-", "^2",
        ] {
            let Err(msg) = compile(formula) else { panic!("{formula:?} should not compile") };
            assert!(!msg.is_empty(), "{formula:?}");
            assert_eq!(eval(formula, 5.0), 0.0, "{formula:?}");
        }
    }

    #[test]
    fn a_double_equals_is_not_an_assignment() {
        assert!(compile("y == 1").is_err());
        assert!(compile("a = x").is_err(), "only y, o and out may stand before the equals sign");
    }

    #[test]
    fn results_that_are_not_numbers_output_zero() {
        assert_eq!(eval("1/0", 0.0), 0.0);
        assert_eq!(eval("-1/0", 0.0), 0.0);
        assert_eq!(eval("0/0", 0.0), 0.0);
        assert_eq!(eval("x % 0", 3.0), 0.0);
        assert_eq!(eval("sqrt(-1)", 0.0), 0.0);
        assert_eq!(eval("ln(0)", 0.0), 0.0);
        assert_eq!(eval("log10(-5)", 0.0), 0.0);
        assert_eq!(eval("asin(2)", 0.0), 0.0);
        assert_eq!(eval("exp(1000)", 0.0), 0.0);
        assert_eq!(eval("pow(0, -1)", 0.0), 0.0);
        assert_eq!(eval("(-8)^0.5", 0.0), 0.0);
        assert_eq!(eval("1e300 * 1e300", 0.0), 0.0);
        assert_eq!(eval("0 * (1/0)", 0.0), 0.0);
        assert_eq!(eval("max(1/0, 5)", 0.0), 0.0, "the infinity wins and is then dropped");
        assert_eq!(eval("1/x", 4.0), 0.25);
        assert_eq!(eval("x", f32::NAN), 0.0);
    }

    #[test]
    fn a_formula_can_be_replaced_and_repaired() {
        let mut n = FormulaProcessNode::new(NodeId(1));
        n.write_input(0, 2.0);
        n.load_data(&serde_json::json!({ "formula": "x * (" }));
        n.process();
        assert_eq!(n.read_output(0), 0.0);
        let mut shared = NodeSharedState::new(1, 1);
        n.update_display(&mut shared);
        let shown = shared.display.as_ref().and_then(|d| d.downcast_ref::<FormulaDisplay>()).unwrap();
        assert!(shown.error.is_some());
        n.load_data(&serde_json::json!({ "formula": "x * 4" }));
        n.process();
        assert_eq!(n.read_output(0), 8.0);
        n.write_input(0, 3.0);
        n.process();
        assert_eq!(n.read_output(0), 12.0);
        n.update_display(&mut shared);
        let shown = shared.display.as_ref().and_then(|d| d.downcast_ref::<FormulaDisplay>()).unwrap();
        assert!(shown.error.is_none());
        assert_eq!(shown.formula, "x * 4");
    }

    #[test]
    fn the_formula_is_saved() {
        let mut n = FormulaProcessNode::new(NodeId(1));
        n.load_data(&serde_json::json!({ "formula": "x*3" }));
        assert_eq!(n.save_data().unwrap()["formula"], "x*3");
    }
}
