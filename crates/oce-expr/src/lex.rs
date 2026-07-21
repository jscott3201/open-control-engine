//! Lexer for the CDL binding subset (`02` §6): opaque binding text → a [`Tok`] stream.
//!
//! The token set is closed: numbers (Modelica `UNSIGNED_NUMBER`), string literals with the
//! simple escapes, identifiers/dotted qualified names (keywords such as `true`/`and`/`not` —
//! and the *contextual* comprehension words `for`/`in` — lex as ordinary [`Tok::Name`]s; the
//! parser decides what they mean by position), and the fixed operator/delimiter set. Any
//! character outside the subset is a typed [`ExprError::Parse`], never a panic. A bare `=` is
//! rejected here with a message pointing at `==` — a binding's `=` never reaches the value
//! expression this crate parses. The grammar over these tokens lives in [`mod@crate::parse`].

use std::sync::Arc;

use crate::ExprError;

/// A lexical token. `Real` carries an `f64`, so the enum is `PartialEq` but not `Eq`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Tok {
    /// An integer literal (no `.`/exponent).
    Int(i64),
    /// A real literal (had a `.` or an exponent).
    Real(f64),
    /// A string literal (quotes stripped, simple escapes resolved).
    Str(Arc<str>),
    /// An identifier or dotted qualified name (also `true`/`false`/`and`/`or`/`not`, and the
    /// contextual comprehension keywords `for`/`in`).
    Name(String),
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `==`
    EqEq,
    /// `<>`
    Ne,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `,`
    Comma,
    /// `{` — opens a brace array literal.
    LBrace,
    /// `}` — closes a brace array literal.
    RBrace,
    /// `:` — the range separator.
    Colon,
    /// `[` — opens a postfix subscript after a primary; in primary position it is the
    /// deferred matrix constructor and is rejected with a typed error.
    LBracket,
    /// `]` — closes a postfix subscript.
    RBracket,
}

/// Build the [`ExprError::Parse`] variant from any message — the one error constructor the
/// lexer and parser share.
pub(crate) fn parse_err(msg: impl Into<String>) -> ExprError {
    ExprError::Parse(msg.into())
}

/// Tokenize `text`, rejecting characters outside the scalar subset with a typed error.
pub(crate) fn lex(text: &str) -> Result<Vec<Tok>, ExprError> {
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            ' ' | '\t' | '\r' | '\n' => i += 1,
            '0'..='9' => {
                let (tok, next) = lex_number(&bytes, i)?;
                out.push(tok);
                i = next;
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let (tok, next) = lex_name(&bytes, i);
                out.push(tok);
                i = next;
            }
            '"' => {
                let (tok, next) = lex_string(&bytes, i)?;
                out.push(tok);
                i = next;
            }
            '+' => {
                out.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                out.push(Tok::Minus);
                i += 1;
            }
            '*' => {
                out.push(Tok::Star);
                i += 1;
            }
            '/' => {
                out.push(Tok::Slash);
                i += 1;
            }
            '>' => {
                if bytes.get(i + 1) == Some(&'=') {
                    out.push(Tok::Ge);
                    i += 2;
                } else {
                    out.push(Tok::Gt);
                    i += 1;
                }
            }
            '<' => match bytes.get(i + 1) {
                Some('=') => {
                    out.push(Tok::Le);
                    i += 2;
                }
                Some('>') => {
                    out.push(Tok::Ne);
                    i += 2;
                }
                _ => {
                    out.push(Tok::Lt);
                    i += 1;
                }
            },
            '=' => {
                if bytes.get(i + 1) == Some(&'=') {
                    out.push(Tok::EqEq);
                    i += 2;
                } else {
                    return Err(parse_err(
                        "bare '=' is a binding, not a value expression (use '==' for equality)",
                    ));
                }
            }
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '{' => {
                out.push(Tok::LBrace);
                i += 1;
            }
            '}' => {
                out.push(Tok::RBrace);
                i += 1;
            }
            ':' => {
                out.push(Tok::Colon);
                i += 1;
            }
            '[' => {
                out.push(Tok::LBracket);
                i += 1;
            }
            ']' => {
                out.push(Tok::RBracket);
                i += 1;
            }
            other => return Err(parse_err(format!("unexpected character {other:?}"))),
        }
    }
    Ok(out)
}

/// Lex a Modelica `UNSIGNED_NUMBER`: integer part, optional `.frac`, optional `e[+|-]exp`.
/// Integer iff it has neither a fractional part nor an exponent.
fn lex_number(bytes: &[char], start: usize) -> Result<(Tok, usize), ExprError> {
    let mut i = start;
    let mut is_real = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == '.' {
        is_real = true;
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i < bytes.len() && (bytes[i] == 'e' || bytes[i] == 'E') {
        is_real = true;
        i += 1;
        if i < bytes.len() && (bytes[i] == '+' || bytes[i] == '-') {
            i += 1;
        }
        let exp_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp_start {
            return Err(parse_err("malformed number: exponent has no digits"));
        }
    }
    let text: String = bytes[start..i].iter().collect();
    let tok = if is_real {
        Tok::Real(
            text.parse::<f64>()
                .map_err(|_| parse_err(format!("malformed real literal {text:?}")))?,
        )
    } else {
        Tok::Int(
            text.parse::<i64>()
                .map_err(|_| parse_err(format!("integer literal out of range {text:?}")))?,
        )
    };
    Ok((tok, i))
}

/// Lex an identifier or dotted qualified name (`Modelica.Constants.pi`). A `.` continues the
/// name only when followed by an identifier start; `.` before a digit cannot occur (numbers
/// never start with `.`), so there is no ambiguity with member/array syntax.
fn lex_name(bytes: &[char], start: usize) -> (Tok, usize) {
    let mut i = start;
    loop {
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == '_') {
            i += 1;
        }
        if i + 1 < bytes.len()
            && bytes[i] == '.'
            && (bytes[i + 1].is_ascii_alphabetic() || bytes[i + 1] == '_')
        {
            i += 1; // consume '.', continue the qualified name
        } else {
            break;
        }
    }
    let text: String = bytes[start..i].iter().collect();
    (Tok::Name(text), i)
}

/// Lex a `"..."` string literal, resolving the `\"`, `\\`, `\n`, `\t` escapes.
fn lex_string(bytes: &[char], start: usize) -> Result<(Tok, usize), ExprError> {
    let mut i = start + 1; // skip opening quote
    let mut s = String::new();
    while i < bytes.len() {
        match bytes[i] {
            '"' => return Ok((Tok::Str(Arc::from(s.as_str())), i + 1)),
            '\\' => {
                i += 1;
                match bytes.get(i) {
                    Some('"') => s.push('"'),
                    Some('\\') => s.push('\\'),
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some(other) => s.push(*other),
                    None => return Err(parse_err("string ends with a dangling backslash")),
                }
                i += 1;
            }
            other => {
                s.push(other);
                i += 1;
            }
        }
    }
    Err(parse_err("unterminated string literal"))
}
