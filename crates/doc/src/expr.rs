//! Variables and the expressions typed into value fields: numbers with optional units, + − × ÷ ^, parentheses,
//! a few functions and variable names ("width / 2 + 3 mm"). Lengths come out in millimeters, angles in degrees.

use serde::{Deserialize, Serialize};

/// A named value. Its expression may use other variables.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub expr: String,
    /// What it measures: a length (mm), an angle (degrees) or a plain number.
    #[serde(default, skip_serializing_if = "VarKind::is_length")]
    pub kind: VarKind,
}

/// A variable's type, as the Variables window offers them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarKind {
    #[default]
    Length,
    Angle,
    Number,
}

impl VarKind {
    fn is_length(&self) -> bool {
        *self == VarKind::Length
    }

    pub fn label(self) -> &'static str {
        match self {
            VarKind::Length => "Length",
            VarKind::Angle => "Angle",
            VarKind::Number => "Number",
        }
    }
}

/// One-argument functions (angles in degrees), then two-argument ones.
const FUNCTIONS: [&str; 17] =
    ["sqrt", "abs", "sin", "cos", "tan", "asin", "acos", "atan", "round", "floor", "ceil", "exp", "ln", "log", "sign", "trunc", "cbrt"];
const FUNCTIONS2: [&str; 5] = ["min", "max", "pow", "atan2", "mod"];
const UNITS: [(&str, f64); 11] = [
    ("mm", 1.0),
    ("cm", 10.0),
    ("m", 1000.0),
    ("um", 0.001),
    ("in", 25.4),
    ("ft", 304.8),
    ("yd", 914.4),
    ("deg", 1.0),
    ("rad", 180.0 / std::f64::consts::PI),
    ("°", 1.0),
    ("\"", 25.4),
];

/// Whether a name can be a variable's: a letter or underscore, then letters, digits or underscores, and not a
/// unit or function name.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.len() <= 64
        && !FUNCTIONS.contains(&name)
        && !FUNCTIONS2.contains(&name)
        && !UNITS.iter().any(|(u, _)| *u == name)
        && name != "pi"
}

/// Whether text is more than a plain number (so it should be kept as an expression).
pub fn is_expression(text: &str) -> bool {
    text.trim().parse::<f64>().is_err()
}

pub fn eval(text: &str, vars: &[Variable]) -> Result<f64, String> {
    eval_depth(text, vars, 0)
}

fn eval_depth(text: &str, vars: &[Variable], depth: usize) -> Result<f64, String> {
    if depth > 32 {
        return Err("a variable refers back to itself".into());
    }
    let tokens = tokenize(text)?;
    let mut p = Parser { tokens, pos: 0, vars, depth };
    let v = p.sum()?;
    if p.pos != p.tokens.len() {
        return Err(format!("couldn't read '{}'", text.trim()));
    }
    if v.is_finite() { Ok(v) } else { Err("that's not a finite number".into()) }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Name(String),
    Op(char),
}

fn tokenize(text: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            out.push(Tok::Num(s.parse().map_err(|_| format!("'{s}' isn't a number"))?));
            // A unit after a number (spaces allowed) scales it.
            let mut j = i;
            while j < chars.len() && chars[j] == ' ' {
                j += 1;
            }
            let rest: String = chars[j..].iter().collect();
            if let Some((u, f)) = UNITS
                .iter()
                .filter(|(u, _)| rest.starts_with(u) && !rest[u.len()..].starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_'))
                .max_by_key(|(u, _)| u.len())
            {
                if let Some(Tok::Num(n)) = out.last_mut() {
                    *n *= f;
                }
                i = j + u.chars().count();
            }
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Tok::Name(chars[start..i].iter().collect()));
        } else if "+-*/^(),×÷".contains(c) {
            out.push(Tok::Op(match c {
                '×' => '*',
                '÷' => '/',
                other => other,
            }));
            i += 1;
        } else if c == '°' {
            i += 1;
        } else {
            return Err(format!("'{c}' can't be used here"));
        }
    }
    Ok(out)
}

struct Parser<'a> {
    tokens: Vec<Tok>,
    pos: usize,
    vars: &'a [Variable],
    depth: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }

    fn eat(&mut self, op: char) -> bool {
        if self.peek() == Some(&Tok::Op(op)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn sum(&mut self) -> Result<f64, String> {
        let mut v = self.product()?;
        loop {
            if self.eat('+') {
                v += self.product()?;
            } else if self.eat('-') {
                v -= self.product()?;
            } else {
                return Ok(v);
            }
        }
    }

    fn product(&mut self) -> Result<f64, String> {
        let mut v = self.power()?;
        loop {
            if self.eat('*') {
                v *= self.power()?;
            } else if self.eat('/') {
                let d = self.power()?;
                if d == 0.0 {
                    return Err("division by zero".into());
                }
                v /= d;
            } else {
                return Ok(v);
            }
        }
    }

    fn power(&mut self) -> Result<f64, String> {
        let base = self.unary()?;
        if self.eat('^') { Ok(base.powf(self.power()?)) } else { Ok(base) }
    }

    fn unary(&mut self) -> Result<f64, String> {
        if self.eat('-') {
            return Ok(-self.unary()?);
        }
        if self.eat('+') {
            return self.unary();
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<f64, String> {
        match self.tokens.get(self.pos).cloned() {
            Some(Tok::Num(n)) => {
                self.pos += 1;
                Ok(n)
            }
            Some(Tok::Op('(')) => {
                self.pos += 1;
                let v = self.sum()?;
                if !self.eat(')') {
                    return Err("a ')' is missing".into());
                }
                Ok(v)
            }
            Some(Tok::Name(name)) => {
                self.pos += 1;
                if FUNCTIONS2.contains(&name.as_str()) {
                    if !self.eat('(') {
                        return Err(format!("{name} needs ( , )"));
                    }
                    let a = self.sum()?;
                    if !self.eat(',') {
                        return Err(format!("{name} takes two values: {name}(a, b)"));
                    }
                    let b = self.sum()?;
                    if !self.eat(')') {
                        return Err("a ')' is missing".into());
                    }
                    return match name.as_str() {
                        "min" => Ok(a.min(b)),
                        "max" => Ok(a.max(b)),
                        "pow" => Ok(a.powf(b)),
                        "atan2" => Ok(a.atan2(b).to_degrees()),
                        _ if b == 0.0 => Err("division by zero".into()),
                        _ => Ok(a.rem_euclid(b)),
                    };
                }
                if FUNCTIONS.contains(&name.as_str()) {
                    if !self.eat('(') {
                        return Err(format!("{name} needs ( )"));
                    }
                    let x = self.sum()?;
                    if !self.eat(')') {
                        return Err("a ')' is missing".into());
                    }
                    return Ok(match name.as_str() {
                        "sqrt" => x.sqrt(),
                        "abs" => x.abs(),
                        "sin" => x.to_radians().sin(),
                        "cos" => x.to_radians().cos(),
                        "tan" => x.to_radians().tan(),
                        "asin" => x.asin().to_degrees(),
                        "acos" => x.acos().to_degrees(),
                        "atan" => x.atan().to_degrees(),
                        "round" => x.round(),
                        "floor" => x.floor(),
                        "exp" => x.exp(),
                        "ln" => x.ln(),
                        "log" => x.log10(),
                        "sign" => x.signum(),
                        "trunc" => x.trunc(),
                        "cbrt" => x.cbrt(),
                        _ => x.ceil(),
                    });
                }
                if name == "pi" {
                    return Ok(std::f64::consts::PI);
                }
                if name == "e" && !self.vars.iter().any(|v| v.name == "e") {
                    return Ok(std::f64::consts::E);
                }
                let var = self.vars.iter().find(|v| v.name == name).ok_or_else(|| format!("there's no variable '{name}'"))?;
                eval_depth(&var.expr, self.vars, self.depth + 1)
            }
            _ => Err("a value is missing".into()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn var(name: &str, expr: &str) -> Variable {
        Variable { name: name.into(), expr: expr.into(), kind: VarKind::Length }
    }

    #[test]
    fn more_functions_and_units() {
        let vars = [var("w", "30")];
        assert_eq!(eval("max(w, 40) - min(2, 3)", &vars).unwrap(), 38.0);
        assert_eq!(eval("pow(2, 10)", &vars).unwrap(), 1024.0);
        assert_eq!(eval("mod(17, 5)", &vars).unwrap(), 2.0);
        assert!((eval("atan2(1, 1)", &vars).unwrap() - 45.0).abs() < 1e-12);
        assert!((eval("acos(0)", &vars).unwrap() - 90.0).abs() < 1e-12);
        assert!((eval("0.5 rad", &vars).unwrap() - 28.64788975654116).abs() < 1e-9, "radians come out in degrees");
        assert!((eval("ln(e)", &vars).unwrap() - 1.0).abs() < 1e-12);
        assert_eq!(eval("log(1000)", &vars).unwrap(), 3.0);
        assert_eq!(eval("500 um", &vars).unwrap(), 0.5);
        assert!(eval("max(1)", &vars).is_err());
        assert!(eval("mod(1, 0)", &vars).is_err());
        assert!(!valid_name("max"));
    }

    #[test]
    fn evaluates_units_operators_and_variables() {
        let vars = [var("width", "40"), var("half", "width / 2"), var("loop_a", "loop_b"), var("loop_b", "loop_a")];
        assert_eq!(eval("2 + 3 * 4", &vars).unwrap(), 14.0);
        assert_eq!(eval("(2 + 3) * 4", &vars).unwrap(), 20.0);
        assert_eq!(eval("1in + 2cm", &vars).unwrap(), 45.4);
        assert_eq!(eval("half + 5mm", &vars).unwrap(), 25.0);
        assert_eq!(eval("3 mm + 1 cm", &vars).unwrap(), 13.0);
        assert_eq!(eval("2^3^2", &vars).unwrap(), 512.0);
        assert_eq!(eval("-width", &vars).unwrap(), -40.0);
        assert!((eval("sqrt(16) + sin(90)", &vars).unwrap() - 5.0).abs() < 1e-12);
        assert!(eval("loop_a", &vars).is_err());
        assert!(eval("nope + 1", &vars).is_err());
        assert!(eval("1 / 0", &vars).is_err());
        assert!(eval("2 +", &vars).is_err());
        assert!(valid_name("hole_d2") && !valid_name("2x") && !valid_name("mm") && !valid_name("sqrt"));
        assert!(is_expression("width") && !is_expression(" 12.5 "));
    }
}
