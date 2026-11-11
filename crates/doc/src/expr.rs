//! Variables and the expressions typed into value fields.
//!
//! A field takes a plain number or an expression: + − × ÷ ^ and parentheses, units on any number (5 mm + 2 cm,
//! 1' 6", 1 1/2", 30 deg), functions (sqrt, sin, avg, round…) and variable names ("width / 2 + 3 mm"). Every value
//! knows whether it is a length, an angle or a plain number, so a number without a unit next to a length takes the
//! workspace unit (2 in + 1 → 3 in), a length times a number stays a length, and a length over a length is a number.
//! Lengths come out in millimeters, angles in degrees.

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

/// A variable's type, as the Variables window offers them; also what a value field wants.
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

/// Function names that can't be variable names (kept as they were so saved projects still load). Newer function
/// names only count as functions before a '(' and may also name a variable.
const RESERVED: [&str; 22] = [
    "sqrt", "abs", "sin", "cos", "tan", "asin", "acos", "atan", "round", "floor", "ceil", "exp", "ln", "log", "sign", "trunc", "cbrt",
    "min", "max", "pow", "atan2", "mod",
];

/// Units a number can carry: (spelling, kind, factor to mm or degrees). Longest spelling wins.
const UNITS: [(&str, Dim, f64); 29] = [
    ("mm", Dim::LEN, 1.0),
    ("millimeter", Dim::LEN, 1.0),
    ("millimeters", Dim::LEN, 1.0),
    ("millimetre", Dim::LEN, 1.0),
    ("millimetres", Dim::LEN, 1.0),
    ("cm", Dim::LEN, 10.0),
    ("centimeter", Dim::LEN, 10.0),
    ("centimeters", Dim::LEN, 10.0),
    ("centimetre", Dim::LEN, 10.0),
    ("centimetres", Dim::LEN, 10.0),
    ("m", Dim::LEN, 1000.0),
    ("meter", Dim::LEN, 1000.0),
    ("meters", Dim::LEN, 1000.0),
    ("metre", Dim::LEN, 1000.0),
    ("metres", Dim::LEN, 1000.0),
    ("um", Dim::LEN, 0.001),
    ("in", Dim::LEN, 25.4),
    ("inch", Dim::LEN, 25.4),
    ("inches", Dim::LEN, 25.4),
    ("\"", Dim::LEN, 25.4),
    ("ft", Dim::LEN, 304.8),
    ("foot", Dim::LEN, 304.8),
    ("feet", Dim::LEN, 304.8),
    ("'", Dim::LEN, 304.8),
    ("yd", Dim::LEN, 914.4),
    ("deg", Dim::ANGLE, 1.0),
    ("degrees", Dim::ANGLE, 1.0),
    ("°", Dim::ANGLE, 1.0),
    ("rad", Dim::ANGLE, 180.0 / std::f64::consts::PI),
];

/// The unit spellings a variable may not take (the short ones, unchanged so saved projects still load).
const RESERVED_UNITS: [&str; 9] = ["mm", "cm", "m", "um", "in", "ft", "yd", "deg", "rad"];

/// Whether a name can be a variable's: a letter or underscore, then letters, digits or underscores, and not a
/// short unit or function name.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.len() <= 64
        && !RESERVED.contains(&name)
        && !RESERVED_UNITS.contains(&name)
        && name != "pi"
}

/// How an unknown name is reported; [`missing_variable`] reads it back.
const NO_VARIABLE: &str = "there's no variable ";

/// The name the text uses that isn't a variable (yet), if that is why it doesn't evaluate: "20+w" → "w".
pub fn missing_variable(text: &str, vars: &[Variable], want: VarKind, unit: f64) -> Option<String> {
    let e = eval_as(text, vars, want, unit).err()?;
    let name = e.strip_prefix(NO_VARIABLE)?.trim_matches('\'');
    valid_name(name).then(|| name.to_string())
}

/// Whether text is more than a plain number (so it should be kept as an expression).
pub fn is_expression(text: &str) -> bool {
    text.trim().parse::<f64>().is_err()
}

/// Whether the text names a variable (so a field keeps it as an expression that follows the variable).
pub fn uses_variables(text: &str, vars: &[Variable]) -> bool {
    tokenize(&normalize(text)).is_ok_and(|toks| {
        toks.iter().enumerate().any(|(i, t)| {
            matches!(&t.tok, Tok::Name(n) if vars.iter().any(|v| &v.name == n) && toks.get(i + 1).is_none_or(|t| t.tok != Tok::Op('(')))
        })
    })
}

/// Evaluates as stored in projects: numbers without a unit are millimeters where a length is meant.
pub fn eval(text: &str, vars: &[Variable]) -> Result<f64, String> {
    eval_value(text, vars, 1.0).map(|v| v.v)
}

/// Evaluates what was typed into a field that wants `want`. `unit` is the workspace unit in millimeters (25.4 for
/// inches): a bare number meant as a length is in that unit. Lengths come back in mm, angles in degrees.
pub fn eval_as(text: &str, vars: &[Variable], want: VarKind, unit: f64) -> Result<f64, String> {
    let r = eval_value(text, vars, unit)?;
    finish(r, want, unit)
}

/// The text to store for a field so it means the same when evaluated later in millimeters ([`eval`]): in another
/// workspace unit, every bare number that was taken as a length gets the unit written after it
/// ("width + 2" → "width + 2 in"). Errors as [`eval_as`] does.
pub fn with_units(text: &str, vars: &[Variable], want: VarKind, unit: f64, suffix: &str) -> Result<String, String> {
    if (unit - 1.0).abs() < 1e-12 {
        let t = normalize(text);
        finish(Parser::new(&t, vars, unit, 0)?.whole(&t)?, want, unit)?;
        return Ok(text.trim().to_string());
    }
    spell_units(text, vars, want, unit, suffix)
}

/// [`with_units`] even in millimeters: every bare number taken as a length gets `suffix` written after it. A
/// stored "2" (mm) shown in an inch project reads "2 mm", so it keeps its meaning where a bare number means inches.
pub fn spell_units(text: &str, vars: &[Variable], want: VarKind, unit: f64, suffix: &str) -> Result<String, String> {
    let t = normalize(text);
    let mut p = Parser::new(&t, vars, unit, 0)?;
    let r = p.whole(&t)?;
    finish(r, want, unit)?;
    let mut out: Vec<char> = t.chars().collect();
    let mut spans = p.promoted;
    // The whole result is a plain number taken as a length.
    if want == VarKind::Length && r.dim == Dim::NUM {
        spans.push((0, out.len(), r.span.is_some_and(|s| s.2)));
    }
    spans.sort_unstable();
    spans.dedup();
    // Back to front so earlier positions stay put; promoted pieces never overlap.
    for (start, end, single) in spans.into_iter().rev() {
        if single {
            out.splice(end..end, format!(" {suffix}").chars());
        } else {
            out.splice(end..end, format!(") * 1 {suffix}").chars());
            out.insert(start, '(');
        }
    }
    Ok(out.into_iter().collect::<String>().trim().to_string())
}

fn finish(r: Val, want: VarKind, unit: f64) -> Result<f64, String> {
    match (want, r.dim) {
        (VarKind::Length, Dim::NUM) => Ok(r.v * unit),
        (VarKind::Length, d) if d == Dim::LEN => Ok(r.v),
        (VarKind::Length, d) if d.angle => Err("that's an angle, not a length".into()),
        (VarKind::Length, _) => Err("that isn't a length".into()),
        (VarKind::Angle, d) if d == Dim::NUM || d == Dim::ANGLE => Ok(r.v),
        (VarKind::Angle, _) => Err("that's a length, not an angle".into()),
        (VarKind::Number, d) if d == Dim::NUM => Ok(r.v),
        (VarKind::Number, _) => Err("that should be a plain number".into()),
    }
}

fn eval_value(text: &str, vars: &[Variable], unit: f64) -> Result<Val, String> {
    eval_depth(text, vars, unit, 0)
}

fn eval_depth(text: &str, vars: &[Variable], unit: f64, depth: usize) -> Result<Val, String> {
    if depth > 32 {
        return Err("a variable refers back to itself".into());
    }
    let t = normalize(text);
    Parser::new(&t, vars, unit, depth)?.whole(&t)
}

/// Typing stand-ins made plain: − → -, and a comma between digits outside any function call is a decimal point
/// ("2,5" → "2.5"; "max(2,5)" keeps its comma).
fn normalize(text: &str) -> String {
    let t = text.trim().replace(['−', '–'], "-");
    if t.contains('(') {
        return t;
    }
    let c: Vec<char> = t.chars().collect();
    c.iter()
        .enumerate()
        .map(|(i, ch)| {
            let digits = i > 0 && c[i - 1].is_ascii_digit() && c.get(i + 1).is_some_and(char::is_ascii_digit);
            if *ch == ',' && digits { '.' } else { *ch }
        })
        .collect()
}

/// What a value measures: lengths to a power (an area is 2) and whether it is an angle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Dim {
    len: i8,
    angle: bool,
}

impl Dim {
    const NUM: Dim = Dim { len: 0, angle: false };
    const LEN: Dim = Dim { len: 1, angle: false };
    const ANGLE: Dim = Dim { len: 0, angle: true };
}

#[derive(Clone, Copy, Debug)]
struct Val {
    v: f64,
    dim: Dim,
    /// Where it was written in the text (chars), while it is still a plain number: if it meets a length it takes
    /// the workspace unit, and stored text writes that unit after it.
    span: Option<(usize, usize, bool)>,
}

impl Val {
    fn num(v: f64) -> Val {
        Val { v, dim: Dim::NUM, span: None }
    }

    fn of(v: f64, dim: Dim) -> Val {
        Val { v, dim, span: None }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    /// A number; with a unit it is already in mm or degrees.
    Num(f64, Dim, bool),
    Name(String),
    Op(char),
}

#[derive(Clone, Debug)]
struct Spanned {
    tok: Tok,
    start: usize,
    /// Where the token ends in the text, in chars.
    end: usize,
}

fn number_at(chars: &[char], i: &mut usize) -> Result<f64, String> {
    let start = *i;
    while *i < chars.len() && (chars[*i].is_ascii_digit() || chars[*i] == '.') {
        *i += 1;
    }
    let s: String = chars[start..*i].iter().collect();
    s.parse().map_err(|_| format!("'{s}' isn't a number"))
}

fn skip_spaces(chars: &[char], mut j: usize) -> usize {
    while j < chars.len() && chars[j] == ' ' {
        j += 1;
    }
    j
}

/// The unit spelled at `j`, if any: (kind, factor, where it ends).
fn unit_at(chars: &[char], j: usize) -> Option<(Dim, f64, usize)> {
    let rest: String = chars[j.min(chars.len())..].iter().collect();
    UNITS
        .iter()
        .filter(|(u, ..)| {
            rest.starts_with(u)
                && (!u.chars().all(|c| c.is_ascii_alphabetic())
                    || !rest[u.len()..].starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_'))
        })
        .max_by_key(|(u, ..)| u.len())
        .map(|(u, d, f)| (*d, *f, j + u.chars().count()))
}

/// An inch spelling at `j` ('"', in, inch, inches), and where it ends.
fn inch_at(chars: &[char], j: usize) -> Option<usize> {
    unit_at(chars, j).filter(|(d, f, _)| *d == Dim::LEN && (*f - 25.4).abs() < 1e-12).map(|(.., e)| e)
}

/// "1 1/2"" or "1/2"" (a whole and a fraction of an inch) at `i`, as mm, and where it ends.
fn fraction_inches(chars: &[char], i: usize) -> Option<(f64, usize)> {
    let digits = |j: usize| {
        let mut k = j;
        while k < chars.len() && chars[k].is_ascii_digit() {
            k += 1;
        }
        (k > j).then(|| (chars[j..k].iter().collect::<String>().parse::<f64>().ok(), k))
    };
    let (a, mut j) = digits(i)?;
    let a = a?;
    let mut whole = 0.0;
    let (num, den, end);
    let k = skip_spaces(chars, j);
    if k > j && chars.get(k).is_some_and(char::is_ascii_digit) {
        // "1 1/2"
        whole = a;
        let (n, e) = digits(k)?;
        j = e;
        num = n?;
    } else {
        num = a;
    }
    let s = skip_spaces(chars, j);
    if chars.get(s) != Some(&'/') {
        return None;
    }
    let (d, e) = digits(skip_spaces(chars, s + 1))?;
    den = d?;
    end = inch_at(chars, skip_spaces(chars, e))?;
    (den != 0.0).then_some(((whole + num / den) * 25.4, end))
}

fn tokenize(text: &str) -> Result<Vec<Spanned>, String> {
    let mut out: Vec<Spanned> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) {
            // Feet then inches with nothing between ("1' 6"") add up.
            let after_feet = matches!(out.last(), Some(Spanned { tok: Tok::Num(_, Dim::LEN, true), .. }))
                && chars[..i].iter().rev().find(|c| **c != ' ') == Some(&'\'');
            if let Some((mm, end)) = fraction_inches(&chars, i) {
                if after_feet {
                    out.push(Spanned { tok: Tok::Op('+'), start: i, end: i });
                }
                out.push(Spanned { tok: Tok::Num(mm, Dim::LEN, true), start: i, end });
                i = end;
                continue;
            }
            let start = i;
            let n = number_at(&chars, &mut i)?;
            let num_end = i;
            if let Some((dim, f, end)) = unit_at(&chars, skip_spaces(&chars, i)) {
                if after_feet && inch_at(&chars, skip_spaces(&chars, num_end)).is_some() {
                    out.push(Spanned { tok: Tok::Op('+'), start: num_end, end: num_end });
                }
                out.push(Spanned { tok: Tok::Num(n * f, dim, true), start, end });
                i = end;
            } else {
                out.push(Spanned { tok: Tok::Num(n, Dim::NUM, false), start, end: num_end });
            }
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Spanned { tok: Tok::Name(chars[start..i].iter().collect()), start, end: i });
        } else if "+-*/^(),×÷·".contains(c) {
            i += 1;
            out.push(Spanned {
                start: i - 1,
                tok: Tok::Op(match c {
                    '×' | '·' => '*',
                    '÷' => '/',
                    other => other,
                }),
                end: i,
            });
            // A unit after a group applies to all of it ("(1 + 1/2) in").
            if c == ')'
                && let Some((dim, f, end)) = unit_at(&chars, skip_spaces(&chars, i))
            {
                out.push(Spanned { tok: Tok::Op('*'), start: i, end: i });
                out.push(Spanned { tok: Tok::Num(f, dim, true), start: i, end });
                i = end;
            }
        } else if c == '°' {
            // A degree sign after something that isn't a number (a typed "°" on a variable) changes nothing.
            i += 1;
        } else {
            return Err(format!("'{c}' can't be used here"));
        }
    }
    Ok(out)
}

/// One-argument functions (angles in degrees), with their other names.
fn function1(name: &str) -> Option<&'static str> {
    Some(match name {
        "sqrt" => "sqrt",
        "abs" | "absolute" => "abs",
        "sin" => "sin",
        "cos" => "cos",
        "tan" | "tg" => "tan",
        "cotan" | "cot" | "ctg" => "cot",
        "sec" => "sec",
        "cosec" | "csc" => "csc",
        "asin" | "arcsin" => "asin",
        "acos" | "arccos" => "acos",
        "atan" | "arctan" => "atan",
        "acot" | "arccotan" | "actg" => "acot",
        "asec" | "arcsec" => "asec",
        "acsc" | "arccosec" => "acsc",
        "round" => "round",
        "floor" => "floor",
        "ceil" | "ceiling" => "ceil",
        "trunc" => "trunc",
        "sign" | "sgn" => "sign",
        "exp" => "exp",
        "ln" => "ln",
        "log" => "log",
        "cbrt" => "cbrt",
        "radians" => "radians",
        "degrees" => "degrees",
        _ => return None,
    })
}

fn function2(name: &str) -> Option<&'static str> {
    Some(match name {
        "min" | "minimum" => "min",
        "max" | "maximum" => "max",
        "avg" | "average" | "mean" => "avg",
        "mod" | "remainder" => "mod",
        "pow" => "pow",
        "atan2" | "arctan2" => "atan2",
        _ => return None,
    })
}

struct Parser<'a> {
    tokens: Vec<Spanned>,
    pos: usize,
    vars: &'a [Variable],
    unit: f64,
    depth: usize,
    /// Plain numbers and groups taken as lengths: (start, end, a single number).
    promoted: Vec<(usize, usize, bool)>,
}

impl<'a> Parser<'a> {
    fn new(text: &str, vars: &'a [Variable], unit: f64, depth: usize) -> Result<Parser<'a>, String> {
        Ok(Parser { tokens: tokenize(text)?, pos: 0, vars, unit, depth, promoted: Vec::new() })
    }

    fn whole(&mut self, text: &str) -> Result<Val, String> {
        if self.tokens.is_empty() {
            return Err("type a value".into());
        }
        let v = self.sum()?;
        if self.pos != self.tokens.len() {
            return Err(format!("couldn't read '{}'", text.trim()));
        }
        if v.v.is_finite() { Ok(v) } else { Err("that's not a finite number".into()) }
    }

    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos).map(|t| &t.tok)
    }

    fn eat(&mut self, op: char) -> bool {
        if self.peek() == Some(&Tok::Op(op)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// A plain-number result covering the text from `start` up to the last token read.
    fn spanned(&self, v: Val, start: usize) -> Val {
        let end = self.pos.checked_sub(1).and_then(|i| self.tokens.get(i)).map_or(start, |t| t.end);
        Val { span: (v.dim == Dim::NUM).then_some((start, end, false)), ..v }
    }

    fn joined(&self, v: Val, start: Option<usize>) -> Val {
        match start {
            Some(s) => self.spanned(v, s),
            None => Val { span: None, ..v },
        }
    }

    /// A bare number meeting a length becomes one in the workspace unit.
    fn promote(&mut self, a: &mut Val, to: Dim) {
        if a.dim == Dim::NUM && to.len == 1 && !to.angle {
            a.v *= self.unit;
            a.dim = Dim::LEN;
            if let Some(span) = a.span.take() {
                self.promoted.push(span);
            }
        } else if a.dim == Dim::NUM && to == Dim::ANGLE {
            a.dim = Dim::ANGLE;
        }
    }

    /// Two values that must measure the same thing (adding, min, max…).
    fn alike(&mut self, mut a: Val, mut b: Val, what: &str) -> Result<(Val, Val), String> {
        self.promote(&mut a, b.dim);
        self.promote(&mut b, a.dim);
        if a.dim != b.dim {
            return Err(format!("can't {what} {} and {}", describe(a.dim), describe(b.dim)));
        }
        Ok((a, b))
    }

    fn sum(&mut self) -> Result<Val, String> {
        let mut v = self.product()?;
        loop {
            let sign = if self.eat('+') {
                1.0
            } else if self.eat('-') {
                -1.0
            } else {
                return Ok(v);
            };
            let start = v.span.map(|s| s.0);
            let rhs = self.product()?;
            let (a, b) = self.alike(v, rhs, "add")?;
            v = self.joined(Val::of(a.v + sign * b.v, a.dim), start);
        }
    }

    fn product(&mut self) -> Result<Val, String> {
        let mut v = self.power()?;
        loop {
            let start = v.span.map(|s| s.0);
            if self.eat('*') {
                let b = self.power()?;
                v = self.joined(times(v, b, false)?, start);
            } else if self.eat('/') {
                let b = self.power()?;
                if b.v == 0.0 {
                    return Err("division by zero".into());
                }
                v = self.joined(times(v, b, true)?, start);
            } else {
                return Ok(v);
            }
        }
    }

    fn power(&mut self) -> Result<Val, String> {
        let base = self.unary()?;
        if self.eat('^') {
            let start = base.span.map(|s| s.0);
            let e = self.power()?;
            return Ok(self.joined(raise(base, e)?, start));
        }
        Ok(base)
    }

    fn unary(&mut self) -> Result<Val, String> {
        let start = self.tokens.get(self.pos).map(|t| t.start);
        if self.eat('-') {
            let v = self.unary()?;
            return Ok(self.joined(Val { v: -v.v, ..v }, start));
        }
        if self.eat('+') {
            return self.unary();
        }
        self.atom()
    }

    fn args(&mut self, name: &str, n: usize) -> Result<Vec<Val>, String> {
        if !self.eat('(') {
            return Err(format!("{name} needs ( )"));
        }
        let mut out = Vec::new();
        if n == 0 {
            if !self.eat(')') {
                return Err(format!("{name}() takes nothing inside"));
            }
            return Ok(out);
        }
        out.push(self.sum()?);
        while self.eat(',') {
            out.push(self.sum()?);
        }
        if !self.eat(')') {
            return Err("a ')' is missing".into());
        }
        if out.len() != n {
            return Err(if n == 1 { format!("{name} takes one value") } else { format!("{name} takes two values: {name}(a, b)") });
        }
        Ok(out)
    }

    fn atom(&mut self) -> Result<Val, String> {
        let Some(Spanned { tok, start, end }) = self.tokens.get(self.pos).cloned() else { return Err("a value is missing".into()) };
        self.pos += 1;
        match tok {
            Tok::Num(n, dim, unit) => Ok(Val { v: n, dim, span: (!unit).then_some((start, end, true)) }),
            Tok::Op('(') => {
                let v = self.sum()?;
                if !self.eat(')') {
                    return Err("a ')' is missing".into());
                }
                Ok(self.spanned(v, start))
            }
            Tok::Name(name) => {
                let call = self.peek() == Some(&Tok::Op('('));
                if call && let Some(f) = function2(&name) {
                    let a = self.args(&name, 2)?;
                    let r = self.call2(f, a[0], a[1])?;
                    return Ok(self.spanned(r, start));
                }
                if call && let Some(f) = function1(&name) {
                    let a = self.args(&name, 1)?;
                    let r = self.call1(f, a[0])?;
                    return Ok(self.spanned(r, start));
                }
                if name == "pi" {
                    if call {
                        self.args("pi", 0)?;
                    }
                    return Ok(self.spanned(Val::num(std::f64::consts::PI), start));
                }
                if let Some(var) = self.vars.iter().find(|v| v.name == name) {
                    let r = eval_depth(&var.expr, self.vars, 1.0, self.depth + 1)?;
                    let want = match var.kind {
                        VarKind::Length => Dim::LEN,
                        VarKind::Angle => Dim::ANGLE,
                        VarKind::Number => Dim::NUM,
                    };
                    // A variable is what its type says; a bare "30" in a Length variable is 30 mm (as stored).
                    let v = Val::of(r.v, if r.dim == Dim::NUM { want } else { r.dim });
                    return Ok(self.spanned(v, start));
                }
                if name == "e" {
                    return Ok(self.spanned(Val::num(std::f64::consts::E), start));
                }
                if call && (function1(&name).is_some() || function2(&name).is_some()) {
                    return Err(format!("{name} needs ( )"));
                }
                Err(format!("{NO_VARIABLE}'{name}'"))
            }
            Tok::Op(c) => Err(format!("'{c}' needs a value before it")),
        }
    }

    fn call1(&mut self, f: &str, x: Val) -> Result<Val, String> {
        let unit = if x.dim == Dim::LEN { self.unit } else { 1.0 };
        let shown = |g: fn(f64) -> f64| Val::of(g(x.v / unit) * unit, x.dim);
        let angle_in = |x: Val| {
            if x.dim.len != 0 { Err(format!("{f} needs an angle, not a length")) } else { Ok(x.v.to_radians()) }
        };
        let plain = |x: Val| if x.dim == Dim::NUM { Ok(x.v) } else { Err(format!("{f} needs a plain number")) };
        let deg = |r: f64| Val::of(r.to_degrees(), Dim::ANGLE);
        Ok(match f {
            "sqrt" | "cbrt" => {
                let k = if f == "sqrt" { 2 } else { 3 };
                if x.dim.angle || x.dim.len % k != 0 {
                    return Err(format!("{f} of {} isn't something we can measure", describe(x.dim)));
                }
                if f == "sqrt" && x.v < 0.0 {
                    return Err("square root of a negative number".into());
                }
                Val::of(if f == "sqrt" { x.v.sqrt() } else { x.v.cbrt() }, Dim { len: x.dim.len / k, angle: false })
            }
            "abs" => Val::of(x.v.abs(), x.dim),
            // Rounding works in the unit you see (round(2.4 in) is 2 in).
            "round" => shown(f64::round),
            "floor" => shown(f64::floor),
            "ceil" => shown(f64::ceil),
            "trunc" => shown(f64::trunc),
            "sign" => Val::num(if x.v == 0.0 { 0.0 } else { x.v.signum() }),
            "sin" => Val::num(angle_in(x)?.sin()),
            "cos" => Val::num(angle_in(x)?.cos()),
            "tan" => Val::num(angle_in(x)?.tan()),
            "cot" => Val::num(1.0 / angle_in(x)?.tan()),
            "sec" => Val::num(1.0 / angle_in(x)?.cos()),
            "csc" => Val::num(1.0 / angle_in(x)?.sin()),
            "asin" => deg(plain(x)?.asin()),
            "acos" => deg(plain(x)?.acos()),
            "atan" => deg(plain(x)?.atan()),
            "acot" => deg((1.0 / plain(x)?).atan()),
            "asec" => deg((1.0 / plain(x)?).acos()),
            "acsc" => deg((1.0 / plain(x)?).asin()),
            "radians" => Val::num(angle_in(x)?),
            "degrees" => Val::of(plain(x)?.to_degrees(), Dim::ANGLE),
            "exp" => Val::num(plain(x)?.exp()),
            "ln" => Val::num(plain(x)?.ln()),
            _ => Val::num(plain(x)?.log10()),
        })
    }

    fn call2(&mut self, f: &str, a: Val, b: Val) -> Result<Val, String> {
        match f {
            "pow" => raise(a, b),
            "atan2" => {
                let (a, b) = self.alike(a, b, "compare")?;
                Ok(Val::of(a.v.atan2(b.v).to_degrees(), Dim::ANGLE))
            }
            _ => {
                let (a, b) = self.alike(a, b, "compare")?;
                Ok(Val::of(
                    match f {
                        "min" => a.v.min(b.v),
                        "max" => a.v.max(b.v),
                        "avg" => (a.v + b.v) / 2.0,
                        _ if b.v == 0.0 => return Err("division by zero".into()),
                        _ => a.v.rem_euclid(b.v),
                    },
                    a.dim,
                ))
            }
        }
    }
}

fn describe(d: Dim) -> &'static str {
    match (d.len, d.angle) {
        (0, false) => "a number",
        (0, true) => "an angle",
        (1, false) => "a length",
        (2, false) => "an area",
        (3, false) => "a volume",
        _ => "that",
    }
}

fn times(a: Val, b: Val, divide: bool) -> Result<Val, String> {
    let len = if divide { a.dim.len - b.dim.len } else { a.dim.len + b.dim.len };
    // An angle times or over a number stays an angle; over an angle it's a number.
    let angle = match (a.dim.angle, b.dim.angle) {
        (true, true) if divide => false,
        (true, true) => return Err("can't multiply two angles".into()),
        (true, false) | (false, true) if a.dim.len != 0 || b.dim.len != 0 => return Err("can't mix a length and an angle".into()),
        (false, true) if divide => return Err("can't divide by an angle".into()),
        (x, y) => x || y,
    };
    let v = if divide { a.v / b.v } else { a.v * b.v };
    Ok(Val::of(v, Dim { len, angle }))
}

fn raise(base: Val, e: Val) -> Result<Val, String> {
    if e.dim != Dim::NUM {
        return Err("a power must be a plain number".into());
    }
    if base.dim == Dim::NUM {
        return Ok(Val::num(base.v.powf(e.v)));
    }
    if base.dim.angle || e.v.fract() != 0.0 || e.v.abs() > 3.0 {
        return Err(format!("can't raise {} to that power", describe(base.dim)));
    }
    Ok(Val::of(base.v.powf(e.v), Dim { len: base.dim.len * e.v as i8, angle: false }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn var(name: &str, expr: &str) -> Variable {
        Variable { name: name.into(), expr: expr.into(), kind: VarKind::Length }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn more_functions_and_units() {
        let vars = [var("w", "30")];
        assert_eq!(eval("max(w, 40) - min(2, 3)", &vars).unwrap(), 38.0);
        assert_eq!(eval("pow(2, 10)", &vars).unwrap(), 1024.0);
        assert_eq!(eval("mod(17, 5)", &vars).unwrap(), 2.0);
        assert!(close(eval("atan2(1, 1)", &vars).unwrap(), 45.0));
        assert!(close(eval("acos(0)", &vars).unwrap(), 90.0));
        assert!(close(eval("0.5 rad", &vars).unwrap(), 28.64788975654116), "radians come out in degrees");
        assert!(close(eval("ln(e)", &vars).unwrap(), 1.0));
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
        assert!(close(eval("sqrt(16) + sin(90)", &vars).unwrap(), 5.0));
        assert!(eval("loop_a", &vars).is_err());
        assert!(eval("nope + 1", &vars).is_err());
        assert!(eval("1 / 0", &vars).is_err());
        assert!(eval("2 +", &vars).is_err());
        assert!(valid_name("hole_d2") && !valid_name("2x") && !valid_name("mm") && !valid_name("sqrt"));
        assert!(is_expression("width") && !is_expression(" 12.5 "));
    }

    /// The forms people type into a size field (#5).
    #[test]
    fn typed_fields_take_expressions_units_and_fractions() {
        let vars = [var("box_width", "50 mm"), var("wall_thickness", "3 mm")];
        let mm = |t: &str| eval_as(t, &vars, VarKind::Length, 1.0).unwrap();
        let inch = |t: &str| eval_as(t, &vars, VarKind::Length, 25.4).unwrap();
        assert_eq!(mm("(50 + 10) / 2"), 30.0);
        assert_eq!(mm("box_width * 2"), 100.0);
        assert_eq!(mm("box_width - 2 * wall_thickness"), 44.0);
        assert_eq!(mm("5 mm + 2 cm"), 25.0);
        assert!(close(inch("3 in * 2"), 152.4));
        assert!(close(inch("2"), 50.8), "a bare number is in the workspace unit");
        assert!(close(inch("1 + 1 cm"), 35.4), "a bare number added to a length takes the workspace unit");
        assert!(close(mm("1' 6\""), 457.2));
        assert!(close(mm("1 1/2\""), 38.1));
        assert!(close(mm("0 1/2\""), 12.7));
        assert!(close(mm("1/2\""), 12.7), "unlike some apps, a plain fraction of an inch works too");
        assert!(close(mm("1' 1 1/2\""), 342.9));
        assert!(close(mm("2 ft + (1 + 1/2) in"), 647.7));
        assert!(close(mm("2 feet + 3 inches"), 685.8));
        assert!(close(mm("1 meter - 2 millimeters"), 998.0));
        assert!(close(mm("box_width * pi()"), 50.0 * std::f64::consts::PI));
        assert!(close(mm("avg(10, 20) + mean(1, 3)"), 17.0));
        assert!(close(mm("ceiling(2.1) + sgn(-4) + absolute(-1) + remainder(7, 4)"), 6.0));
        assert!(close(mm("2,5"), 2.5), "a decimal comma");
        assert!(close(mm("max(2,5)"), 5.0), "a comma between arguments");
        assert!(close(mm("2 × 3 − 1"), 5.0));
        assert!(close(inch("round(2.4 in)"), 50.8), "rounding works in the unit shown");
        assert!(close(mm("sqrt(9 mm * 4 mm)"), 6.0));
        assert!(close(mm("box_width / wall_thickness * 3"), 50.0), "length over length is a number, times 3 → workspace mm");
    }

    #[test]
    fn angles_and_numbers() {
        let vars = [var("w", "10"), Variable { name: "n".into(), expr: "4".into(), kind: VarKind::Number }];
        let ang = |t: &str| eval_as(t, &vars, VarKind::Angle, 25.4).unwrap();
        assert!(close(ang("30"), 30.0), "angles never take the length unit");
        assert!(close(ang("30 deg + 15"), 45.0));
        assert!(close(ang("90 degrees / 2"), 45.0));
        assert!(close(ang("asin(0.5)"), 30.0));
        assert!(close(ang("atan2(1 in, 1 in)"), 45.0));
        assert!(close(eval_as("radians(180)", &vars, VarKind::Number, 1.0).unwrap(), std::f64::consts::PI));
        assert!(close(eval_as("cos(60 deg) * n", &vars, VarKind::Number, 1.0).unwrap(), 2.0));
        assert!(close(eval_as("n * w", &vars, VarKind::Length, 25.4).unwrap(), 40.0), "a Length variable stays in mm");
        assert!(eval_as("5 mm + 3 deg", &vars, VarKind::Length, 1.0).is_err(), "mixed kinds");
        assert!(eval_as("w", &vars, VarKind::Angle, 1.0).is_err());
        assert!(eval_as("w * w", &vars, VarKind::Length, 1.0).is_err(), "an area isn't a length");
        assert!(eval_as("10 mm", &vars, VarKind::Number, 1.0).is_err());
        assert!(eval_as("", &vars, VarKind::Length, 1.0).is_err());
        assert!(eval_as("sin(5 mm)", &vars, VarKind::Number, 1.0).is_err());
    }

    #[test]
    fn stored_text_keeps_its_meaning_in_other_units() {
        let vars = [var("width", "40")];
        let store = |t: &str| with_units(t, &vars, VarKind::Length, 25.4, "in").unwrap();
        assert_eq!(store("width + 2"), "width + 2 in");
        assert_eq!(store("width * 2"), "width * 2");
        assert_eq!(store("width + 1 cm"), "width + 1 cm");
        assert!(close(eval(&store("width + 2"), &vars).unwrap(), 40.0 + 50.8));
        assert!(close(eval(&store("width / width + 1"), &vars).unwrap(), 50.8));
        assert_eq!(store("(2 + 3) + width"), "((2 + 3)) * 1 in + width");
        assert!(close(eval(&store("(2 + 3) + width"), &vars).unwrap(), 127.0 + 40.0));
        assert!(close(eval(&store("3"), &vars).unwrap(), 76.2));
        assert_eq!(with_units("width + 2", &vars, VarKind::Length, 1.0, "mm").unwrap(), "width + 2");
        assert!(uses_variables("width*2", &vars) && !uses_variables("2 in", &vars) && !uses_variables("max(1, 2)", &vars));
        // Saved names stay loadable: newer function words can still be variables.
        let avg = [var("avg", "3")];
        assert_eq!(eval("avg * 2", &avg).unwrap(), 6.0);
        assert!(valid_name("avg") && valid_name("sec"));
    }

    /// #13: saved bare lengths spelled out in mm for a window that reads bare numbers in another unit.
    #[test]
    fn spell_units_writes_mm_on_saved_bare_lengths() {
        let vars = [Variable { name: "w".into(), expr: "10".into(), kind: VarKind::Length }];
        assert_eq!(spell_units("2", &vars, VarKind::Length, 1.0, "mm").unwrap(), "2 mm");
        let t = spell_units("w + 2", &vars, VarKind::Length, 1.0, "mm").unwrap();
        assert!((eval_as(&t, &vars, VarKind::Length, 25.4).unwrap() - 12.0).abs() < 1e-9, "{t} still means 12 mm");
        assert_eq!(spell_units("5 cm", &vars, VarKind::Length, 1.0, "mm").unwrap(), "5 cm");
        assert_eq!(spell_units("3", &vars, VarKind::Number, 1.0, "mm").unwrap(), "3");
        assert_eq!(spell_units("45", &vars, VarKind::Angle, 1.0, "mm").unwrap(), "45");
    }

    /// #12: the unknown name a value box can offer to create.
    #[test]
    fn missing_variable_names_the_unknown_name() {
        let vars = [Variable { name: "w".into(), expr: "10".into(), kind: VarKind::Length }];
        assert_eq!(missing_variable("20+h", &vars, VarKind::Length, 1.0).as_deref(), Some("h"));
        assert_eq!(missing_variable("20+w", &vars, VarKind::Length, 1.0), None, "it works");
        assert_eq!(missing_variable("20+", &vars, VarKind::Length, 1.0), None, "another reason");
        assert_eq!(missing_variable("sqrt 4", &vars, VarKind::Length, 1.0), None);
    }
}
