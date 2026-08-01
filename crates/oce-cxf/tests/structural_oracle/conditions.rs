//! Upstream conditional extraction and evaluation.
//!
//! Extraction reads the vendored `.mo` source of the declaring class, finds the
//! component's declaration statement, and takes the `if <cond>` that sits OUTSIDE
//! parens, brackets, and strings — the conditional-component form, never the ternary
//! that lives inside a binding.
//!
//! Evaluation runs a condition against the fixture's own parameter values. Two failure
//! shapes are deliberately distinct (a refuter-mandated hardening — the Python
//! reference originally truncated unknown spellings silently, and that hazard was
//! observed exercised by the fixtures' `!` negation):
//!
//! - a syntax problem (unlexable input, trailing unconsumed tokens, ordering on
//!   non-numbers) is a **hard error** that fails the audit with the condition text;
//! - a reference to a parameter the fixture does not carry is [`EvalError::Unevaluable`]
//!   and feeds the UNKNOWNS accounting instead.

use std::collections::{BTreeMap, HashMap};

use super::mo_path;

/// A fixture parameter value, as the comparison sees it.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    Bool(bool),
    Num(f64),
    /// Enum literal, canonicalized to its `Types.`-suffix spelling.
    Enum(String),
}

/// Canonical enum spelling: `EnergyStandard.ASHRAE90_1` no matter how fully the `.mo`
/// or the fixture spelled the literal.
pub fn enum_suffix(x: &str) -> &str {
    match x.rfind("Types.") {
        Some(i) => &x[i + "Types.".len()..],
        None => x,
    }
}

#[derive(Debug)]
pub enum EvalError {
    /// A referenced parameter is absent from the fixture (or non-scalar). The payload
    /// names the missing key; it is read through `Debug` in assertion messages only.
    Unevaluable(#[allow(dead_code)] String),
    /// The condition text itself is malformed for this grammar — always a hard error.
    Syntax(String),
}

/// Memoized `.mo` reader and declaration-condition extractor.
#[derive(Default)]
pub struct MoIndex {
    sources: HashMap<String, Option<String>>,
    conds: HashMap<(String, String), Option<String>>,
}

impl MoIndex {
    fn source(&mut self, class: &str) -> Option<&str> {
        if !self.sources.contains_key(class) {
            let text = std::fs::read_to_string(mo_path(class)).ok();
            self.sources.insert(class.to_owned(), text);
        }
        self.sources[class].as_deref()
    }

    /// The `if <cond>` text of a component declaration: `Some("...")` when conditional,
    /// `Some("")` when declared unconditionally, `None` when the declaration was not
    /// found in the class's own source file.
    pub fn conditional_of(&mut self, class: &str, name: &str) -> Option<String> {
        let key = (class.to_owned(), name.to_owned());
        if let Some(hit) = self.conds.get(&key) {
            return hit.clone();
        }
        let out = self
            .source(class)
            .map(str::to_owned)
            .and_then(|src| src.split(';').find_map(|stmt| declaration_if(stmt, name)));
        self.conds.insert(key, out.clone());
        out
    }
}

/// If `stmt` declares component `name` (a capitalized dotted type, whitespace, then the
/// name followed by `[ ( { ; "` whitespace or end), return its top-level `if` condition
/// (`""` when unconditional).
fn declaration_if(stmt: &str, name: &str) -> Option<String> {
    let bytes = stmt.as_bytes();
    let mut from = 0;
    while let Some(rel) = stmt[from..].find(name) {
        let start = from + rel;
        let end = start + name.len();
        from = start + 1;
        // word boundaries around the name
        if start > 0 && is_word(bytes[start - 1]) {
            continue;
        }
        if end < bytes.len() && is_word(bytes[end]) {
            continue;
        }
        // the character after the name (if any) must open a modifier/array/comment or
        // terminate the declaration
        if end < bytes.len()
            && !matches!(
                bytes[end],
                b' ' | b'\t' | b'\n' | b'\r' | b'[' | b'(' | b'{' | b'"'
            )
        {
            continue;
        }
        // at least one whitespace before the name, preceded by a capitalized dotted type
        let mut t_end = start;
        while t_end > 0 && matches!(bytes[t_end - 1], b' ' | b'\t' | b'\n' | b'\r') {
            t_end -= 1;
        }
        if t_end == start {
            continue; // no separating whitespace: not a declaration form
        }
        let mut t_start = t_end;
        while t_start > 0 && (is_word(bytes[t_start - 1]) || bytes[t_start - 1] == b'.') {
            t_start -= 1;
        }
        let ty = &stmt[t_start..t_end];
        if ty.chars().next().is_none_or(|c| !c.is_ascii_uppercase()) {
            continue;
        }
        return Some(toplevel_if(&stmt[end..]));
    }
    None
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Find ` if ` outside parens/brackets/braces/strings; return the whitespace-normalized
/// condition text up to any `annotation`, or `""` when the declaration is unconditional.
fn toplevel_if(rest: &str) -> String {
    let bytes = rest.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'i' if depth == 0
                && rest[i..].starts_with("if")
                && (i == 0 || !is_word(bytes[i - 1]))
                && i + 2 < bytes.len()
                && !is_word(bytes[i + 2]) =>
            {
                let cond = &rest[i + 2..];
                let cond = cond.split("annotation").next().unwrap_or(cond);
                return cond.split_whitespace().collect::<Vec<_>>().join(" ");
            }
            _ => {}
        }
        i += 1;
    }
    String::new()
}

// ---- evaluator ----

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    LParen,
    RParen,
    Op(&'static str),
    Not,
    And,
    Or,
    Ident(String),
    Num(f64),
}

fn lex(cond: &str) -> Result<Vec<Tok>, EvalError> {
    let mut toks = Vec::new();
    let mut rest = cond.trim_start();
    while !rest.is_empty() {
        if rest.starts_with('"') {
            break; // trailing description string ends the condition
        }
        let (tok, len) = if let Some(r) = rest.strip_prefix("==") {
            let _ = r;
            (Tok::Op("=="), 2)
        } else if rest.starts_with("<>") {
            (Tok::Op("<>"), 2)
        } else if rest.starts_with("<=") {
            (Tok::Op("<="), 2)
        } else if rest.starts_with(">=") {
            (Tok::Op(">="), 2)
        } else if rest.starts_with('<') {
            (Tok::Op("<"), 1)
        } else if rest.starts_with('>') {
            (Tok::Op(">"), 1)
        } else if rest.starts_with('(') {
            (Tok::LParen, 1)
        } else if rest.starts_with(')') {
            (Tok::RParen, 1)
        } else if rest.starts_with('!') {
            (Tok::Not, 1)
        } else {
            let word_len = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
                .unwrap_or(rest.len());
            if word_len == 0 {
                // unlexable input is a hard error, never a silent truncation
                return Err(EvalError::Syntax(format!(
                    "cannot lex {cond:?} at {rest:?}"
                )));
            }
            let word = &rest[..word_len];
            let tok = match word {
                "not" => Tok::Not,
                "and" => Tok::And,
                "or" => Tok::Or,
                _ => match word.parse::<f64>() {
                    Ok(n) => Tok::Num(n),
                    Err(_) => Tok::Ident(word.to_owned()),
                },
            };
            (tok, word_len)
        };
        toks.push(tok);
        rest = rest[len..].trim_start();
    }
    if toks.is_empty() {
        return Err(EvalError::Syntax(format!("empty condition {cond:?}")));
    }
    Ok(toks)
}

#[derive(Debug, Clone, PartialEq)]
enum Val {
    B(bool),
    N(f64),
    E(String),
}

/// Evaluate a conditional-component expression against the fixture's flattened
/// parameter values; `scope` is the declaring instance's path prefix (`""` or
/// `"setPoi."`). Trailing unconsumed tokens are a hard error.
pub fn evaluate(
    cond: &str,
    env: &BTreeMap<String, ParamValue>,
    scope: &str,
) -> Result<bool, EvalError> {
    let toks = lex(cond)?;
    let mut pos = 0usize;
    let out = expr_or(&toks, &mut pos, env, scope, cond)?;
    if pos != toks.len() {
        return Err(EvalError::Syntax(format!(
            "trailing unconsumed input in {cond:?} at token {pos}"
        )));
    }
    match out {
        Val::B(b) => Ok(b),
        other => Err(EvalError::Syntax(format!(
            "condition {cond:?} is not boolean: {other:?}"
        ))),
    }
}

type ExprResult = Result<Val, EvalError>;

fn expr_or(
    t: &[Tok],
    p: &mut usize,
    env: &BTreeMap<String, ParamValue>,
    s: &str,
    c: &str,
) -> ExprResult {
    let mut v = as_bool(expr_and(t, p, env, s, c)?, c)?;
    while t.get(*p) == Some(&Tok::Or) {
        *p += 1;
        // eager, like the reference: an Unevaluable side propagates even when the
        // other side would decide — conservative in the unknown direction
        let rhs = as_bool(expr_and(t, p, env, s, c)?, c)?;
        v = v || rhs;
    }
    Ok(Val::B(v))
}

fn expr_and(
    t: &[Tok],
    p: &mut usize,
    env: &BTreeMap<String, ParamValue>,
    s: &str,
    c: &str,
) -> ExprResult {
    let first = expr_cmp(t, p, env, s, c)?;
    if t.get(*p) != Some(&Tok::And) {
        return Ok(first);
    }
    let mut v = as_bool(first, c)?;
    while t.get(*p) == Some(&Tok::And) {
        *p += 1;
        let rhs = as_bool(expr_cmp(t, p, env, s, c)?, c)?;
        v = v && rhs;
    }
    Ok(Val::B(v))
}

fn expr_cmp(
    t: &[Tok],
    p: &mut usize,
    env: &BTreeMap<String, ParamValue>,
    s: &str,
    c: &str,
) -> ExprResult {
    let a = atom(t, p, env, s, c)?;
    if let Some(Tok::Op(op)) = t.get(*p) {
        let op = *op;
        *p += 1;
        let b = atom(t, p, env, s, c)?;
        let out = match op {
            "==" => a == b,
            "<>" => a != b,
            _ => match (&a, &b) {
                (Val::N(x), Val::N(y)) => match op {
                    "<" => x < y,
                    ">" => x > y,
                    "<=" => x <= y,
                    ">=" => x >= y,
                    _ => unreachable!(),
                },
                _ => {
                    return Err(EvalError::Syntax(format!(
                        "ordering on non-numbers in {c:?}: {a:?} {op} {b:?}"
                    )));
                }
            },
        };
        return Ok(Val::B(out));
    }
    Ok(a)
}

fn atom(
    t: &[Tok],
    p: &mut usize,
    env: &BTreeMap<String, ParamValue>,
    s: &str,
    c: &str,
) -> ExprResult {
    let tok = t
        .get(*p)
        .ok_or_else(|| EvalError::Syntax(format!("unexpected end of condition {c:?}")))?
        .clone();
    *p += 1;
    match tok {
        Tok::LParen => {
            let v = expr_or(t, p, env, s, c)?;
            if t.get(*p) != Some(&Tok::RParen) {
                return Err(EvalError::Syntax(format!("missing ')' in {c:?}")));
            }
            *p += 1;
            Ok(v)
        }
        // Modelica 'not' (and the fixtures' '!') binds looser than relationals
        Tok::Not => Ok(Val::B(!as_bool(expr_cmp(t, p, env, s, c)?, c)?)),
        Tok::Num(n) => Ok(Val::N(n)),
        Tok::Ident(word) => {
            if word.contains("Types.") {
                return Ok(Val::E(enum_suffix(&word).to_owned()));
            }
            match word.as_str() {
                "true" => Ok(Val::B(true)),
                "false" => Ok(Val::B(false)),
                _ => {
                    let key = format!("{s}{word}");
                    match env.get(&key) {
                        None => Err(EvalError::Unevaluable(key)),
                        Some(ParamValue::Bool(b)) => Ok(Val::B(*b)),
                        Some(ParamValue::Num(n)) => Ok(Val::N(*n)),
                        Some(ParamValue::Enum(e)) => Ok(Val::E(enum_suffix(e).to_owned())),
                    }
                }
            }
        }
        other => Err(EvalError::Syntax(format!(
            "unexpected token {other:?} in {c:?}"
        ))),
    }
}

fn as_bool(v: Val, cond: &str) -> Result<bool, EvalError> {
    match v {
        Val::B(b) => Ok(b),
        other => Err(EvalError::Syntax(format!(
            "non-boolean operand in {cond:?}: {other:?}"
        ))),
    }
}
