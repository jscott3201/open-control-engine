//! Values: numbers with their unit suffixes, Booleans, strings, enumeration members, references
//! and lists, and the lowering of a list to one assignment per element (decision 15).
//!
//! Numbers. An integer token is `-?(0|[1-9][0-9]*)` and becomes [`Literal::Integer`] (its spelling
//! is kept: an Integer never becomes a Real, and `-0` reads as `0`, as on the wire); a token with a
//! fraction or an exponent is a [`Literal::Real`]. A leading `+`, a leading zero, `1.`, `.5` and a
//! letter glued to the digits are refused ([`MALFORMED_NUMBER`]), never normalized.
//!
//! Suffixes. A base-unit suffix is kept beside the number as written. A scaled time suffix (`1 h`)
//! makes the value the base-unit Real ([`super::super::units::scale`]) and keeps the written
//! number as [`crate::Assignment::scaled_from`]; a scaled value that overflows binary64 is
//! [`NON_FINITE_NUMBER`], at the number (issue #501).

use oce_api::{PARAM_COL_PLACEHOLDER, PARAM_INDEX_PLACEHOLDER, PARAM_ROW_PLACEHOLDER};

use crate::identifier::Identifier;
use crate::location::Location;
use crate::model::{Annotations, Assignment, Literal, MemberRef, ParamValue};
use crate::validate::catalog::{is_template, lookup_class};

use super::super::ParseError;
use super::super::codes::{
    COMMENT_BEFORE_CLOSE, INTEGER_OUT_OF_RANGE, MALFORMED_LIST, MALFORMED_NUMBER, MAX_LIST_DEPTH,
    NESTING_TOO_DEEP, NON_FINITE_NUMBER, STRAY_TOKEN, UNCLOSED_BRACKET,
};
use super::super::lexer::Tok;
use super::super::units::{SuffixKind, check_suffix, scale};
use super::{Parser, room};

/// One scalar value as written.
pub(super) struct Scalar {
    pub(super) value: ParamValue,
    pub(super) suffix: Option<String>,
    /// The number as written, when a scaled suffix made `value` its base-unit value.
    pub(super) scaled_from: Option<Literal>,
    pub(super) at: Location,
    pub(super) annotations: Annotations,
}

/// A value: a scalar, or a list (its `[` and its items).
pub(super) enum Value {
    Scalar(Scalar),
    List(Location, Vec<Value>),
}

/// Whether a word spells a non-finite number (`NaN`, `inf`, `Infinity`, any case).
pub(super) fn non_finite_word(word: &str) -> bool {
    ["nan", "inf", "infinity"]
        .iter()
        .any(|w| w.eq_ignore_ascii_case(word))
}

/// Read a number token (see the module documentation).
pub(super) fn number(text: &str, at: Location) -> Result<Literal, ParseError> {
    let malformed = |why: &str| {
        Err(ParseError::new(
            MALFORMED_NUMBER,
            at,
            format!("`{text}` is not a number: {why}"),
        ))
    };
    if text.starts_with('+') {
        return malformed("a leading `+` is refused; write the number unsigned");
    }
    let body = text.strip_prefix('-').unwrap_or(text).as_bytes();
    let int = body.iter().take_while(|b| b.is_ascii_digit()).count();
    if int == 0 {
        return malformed("it starts with a digit (write `0.5`, not `.5`)");
    }
    if int > 1 && body[0] == b'0' {
        return malformed("a leading zero is refused");
    }
    let mut i = int;
    let mut real = false;
    if body.get(i) == Some(&b'.') {
        let frac = body[i + 1..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        if frac == 0 {
            return malformed("a `.` is followed by digits (write `1.0`, not `1.`)");
        }
        i += 1 + frac;
        real = true;
    }
    if matches!(body.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(body.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let exp = body[i..].iter().take_while(|b| b.is_ascii_digit()).count();
        if exp == 0 {
            return malformed("an exponent has digits");
        }
        i += exp;
        real = true;
    }
    if i != body.len() {
        return malformed(
            "unexpected characters after the number (a unit suffix is separated by a space)",
        );
    }
    if !real {
        return text.parse::<i64>().map(Literal::Integer).map_err(|_| {
            ParseError::new(
                INTEGER_OUT_OF_RANGE,
                at,
                format!("the integer `{text}` is outside the 64-bit range"),
            )
        });
    }
    match text.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(Literal::Real(value)),
        _ => Err(ParseError::new(
            NON_FINITE_NUMBER,
            at,
            format!("`{text}` overflows binary64"),
        )),
    }
}

impl Parser<'_> {
    /// A dotted path `word { "." word }` (a class path, an enum member, an endpoint), with no
    /// space on either side of a dot.
    pub(super) fn dotted(&mut self, what: &str) -> Result<(String, Location), ParseError> {
        let first = self.lx.peek()?;
        let (name, at) = self.name(what)?;
        let mut path = name.as_str().to_owned();
        self.path_rest(&mut path, first.end)?;
        Ok((path, at))
    }

    /// Continue a dotted path whose last word ended at byte `end`: each `.` and the word after it
    /// must touch what precedes them.
    fn path_rest(&mut self, path: &mut String, mut end: usize) -> Result<(), ParseError> {
        loop {
            let dot = self.lx.peek()?;
            if dot.tok != Tok::Punct(b'.') {
                return Ok(());
            }
            if dot.start != end {
                return Err(ParseError::new(
                    STRAY_TOKEN,
                    dot.at,
                    "a dotted name has no space before a `.`",
                ));
            }
            self.lx.bump(&dot);
            let word = self.lx.peek()?;
            if word.tok != Tok::Word || word.start != dot.end {
                return Err(ParseError::new(
                    STRAY_TOKEN,
                    word.at,
                    format!(
                        "expected a name right after `.`, found {}",
                        word.describe(self.lx.src())
                    ),
                ));
            }
            self.lx.bump(&word);
            path.push('.');
            path.push_str(word.text(self.lx.src()));
            end = word.end;
        }
    }

    /// An optional unit suffix after a number (decision 13), with its kind.
    fn suffix(&mut self) -> Result<Option<(String, SuffixKind)>, ParseError> {
        if self.lx.at_value_end() {
            return Ok(None);
        }
        let Some((suffix, at)) = self.lx.raw_run(b"#,)]")? else {
            return Ok(None);
        };
        let kind =
            check_suffix(&suffix).map_err(|(code, detail)| ParseError::new(code, at, detail))?;
        Ok(Some((suffix, kind)))
    }

    /// A value: `[` starts a list (at list depth `depth`), anything else is one scalar.
    /// `pending` holds the comments and blank lines read inside an enclosing list.
    pub(super) fn value(
        &mut self,
        depth: usize,
        pending: &mut Annotations,
    ) -> Result<Value, ParseError> {
        let token = self.lx.next()?;
        if token.tok == Tok::Punct(b'[') {
            return self.list(token.at, depth + 1, pending);
        }
        let annotations = std::mem::take(pending);
        let src = self.lx.src();
        let value = match &token.tok {
            Tok::Number => {
                let text = token.text(src);
                let literal = number(text, token.at)?;
                let (value, suffix, scaled_from) = match self.suffix()? {
                    None => (literal, None, None),
                    Some((suffix, SuffixKind::Base)) => (literal, Some(suffix), None),
                    Some((suffix, SuffixKind::Scaled(row))) => {
                        let Some(base) = scale(&literal, row.factor) else {
                            return Err(ParseError::new(
                                NON_FINITE_NUMBER,
                                token.at,
                                format!(
                                    "`{text} {suffix}` overflows binary64 once scaled to `{}`",
                                    row.base
                                ),
                            ));
                        };
                        (Literal::Real(base), Some(suffix), Some(literal))
                    }
                };
                return Ok(Value::Scalar(Scalar {
                    value: ParamValue::Literal(value),
                    suffix,
                    scaled_from,
                    at: token.at,
                    annotations,
                }));
            }
            Tok::Punct(b'-') => {
                let next = self.lx.peek()?;
                if next.tok == Tok::Word && non_finite_word(next.text(src)) {
                    return Err(ParseError::new(
                        NON_FINITE_NUMBER,
                        token.at,
                        "a value is finite; `-inf` is refused",
                    ));
                }
                return Err(ParseError::new(
                    STRAY_TOKEN,
                    token.at,
                    "expected a value, found `-`",
                ));
            }
            Tok::Str(text) => ParamValue::Literal(Literal::Text(text.clone())),
            Tok::Word => {
                let word = token.text(src);
                if non_finite_word(word) {
                    return Err(ParseError::new(
                        NON_FINITE_NUMBER,
                        token.at,
                        format!("a value is finite; `{word}` is refused"),
                    ));
                }
                match word {
                    "true" => ParamValue::Literal(Literal::Boolean(true)),
                    "false" => ParamValue::Literal(Literal::Boolean(false)),
                    _ => {
                        let mut path = word.to_owned();
                        self.path_rest(&mut path, token.end)?;
                        if path.contains('.') {
                            ParamValue::Literal(Literal::Member(MemberRef::parse(&path)))
                        } else {
                            ParamValue::Ref(Identifier::unchecked(path))
                        }
                    }
                }
            }
            _ => {
                return Err(ParseError::new(
                    STRAY_TOKEN,
                    token.at,
                    format!("expected a value, found {}", token.describe(src)),
                ));
            }
        };
        Ok(Value::Scalar(Scalar {
            value,
            suffix: None,
            scaled_from: None,
            at: token.at,
            annotations,
        }))
    }

    /// The items of a list opened at `open`, at nesting `depth`.
    fn list(
        &mut self,
        open: Location,
        depth: usize,
        pending: &mut Annotations,
    ) -> Result<Value, ParseError> {
        if depth > MAX_LIST_DEPTH {
            return Err(ParseError::new(
                NESTING_TOO_DEEP,
                open,
                format!("lists nest at most {MAX_LIST_DEPTH} deep (vectors and matrices)"),
            ));
        }
        let mut items = Vec::new();
        let mut pending_at: Option<Location> = None;
        let mut expect_item = true;
        loop {
            let token = self.lx.peek()?;
            match &token.tok {
                Tok::Newline => {
                    if self.lx.line_is_empty() {
                        pending.blank_line_before = true;
                    }
                    self.lx.bump(&token);
                }
                Tok::Comment { text, eol } => {
                    // An end-of-line comment belongs to the element before it on its line, as a
                    // statement's does; any other comment waits for the next element.
                    match last_scalar(&mut items).filter(|_| *eol) {
                        Some(scalar) => scalar.annotations.comments.push(text.clone()),
                        None => {
                            pending_at.get_or_insert(token.at);
                            pending.comments.push(text.clone());
                        }
                    }
                    self.lx.bump(&token);
                }
                Tok::Punct(b']') => {
                    if expect_item && !items.is_empty() {
                        return Err(ParseError::new(
                            STRAY_TOKEN,
                            token.at,
                            "expected a value after `,`, found `]`",
                        ));
                    }
                    if let Some(at) = pending_at.filter(|_| !pending.comments.is_empty()) {
                        return Err(ParseError::new(
                            COMMENT_BEFORE_CLOSE,
                            at,
                            "a comment after the last item of a list precedes nothing; move it above an item",
                        ));
                    }
                    self.lx.bump(&token);
                    return Ok(Value::List(open, items));
                }
                Tok::Eof => {
                    return Err(ParseError::new(
                        UNCLOSED_BRACKET,
                        open,
                        "the list's `[` is never closed",
                    ));
                }
                Tok::Punct(b',') if !expect_item => {
                    self.lx.bump(&token);
                    expect_item = true;
                }
                _ if expect_item => {
                    pending_at = None;
                    items.push(self.value(depth, pending)?);
                    expect_item = false;
                }
                _ => {
                    return Err(ParseError::new(
                        STRAY_TOKEN,
                        token.at,
                        format!(
                            "expected `,` or `]`, found {}",
                            token.describe(self.lx.src())
                        ),
                    ));
                }
            }
        }
    }

    /// `name "=" value` inside block `index`'s list, carrying `annotations`; a list is lowered
    /// to one assignment per element. Returns the index of the (first) assignment.
    pub(super) fn assignment(
        &mut self,
        index: usize,
        annotations: Annotations,
    ) -> Result<usize, ParseError> {
        let (name, name_at) = self.name("a parameter name")?;
        self.punct(b'=', "after the parameter name")?;
        let mut pending = annotations;
        let value = self.value(0, &mut pending)?;
        let first = self.sequence.blocks[index].assignments.len();
        match value {
            Value::Scalar(scalar) => {
                self.push_assignment(index, name, scalar, name_at)?;
            }
            Value::List(open, items) => {
                let class_path = self.sequence.blocks[index].class_path.clone();
                for (indices, scalar) in shape(open, items)? {
                    let template = element_template(&class_path, name.as_str(), indices.len());
                    let element = oce_api::param_element_name(&template, &indices)
                        .unwrap_or_else(|| name.as_str().to_owned());
                    let at = scalar.at;
                    self.push_assignment(index, Identifier::unchecked(element), scalar, at)?;
                }
            }
        }
        Ok(first)
    }

    fn push_assignment(
        &mut self,
        index: usize,
        name: Identifier,
        scalar: Scalar,
        at: Location,
    ) -> Result<(), ParseError> {
        let block = &mut self.sequence.blocks[index];
        room(block.assignments.len(), at, "assignments in one block")?;
        let mut assignment = Assignment::new(name, scalar.value);
        assignment.suffix = scalar.suffix;
        assignment.scaled_from = scalar.scaled_from;
        assignment.annotations = scalar.annotations;
        block.assignments.push(assignment);
        self.spans.assignments[index].push(at);
        Ok(())
    }
}

/// The last scalar of a list read so far (the last cell of its last row).
fn last_scalar(items: &mut [Value]) -> Option<&mut Scalar> {
    match items.last_mut()? {
        Value::Scalar(scalar) => Some(scalar),
        Value::List(_, cells) => last_scalar(cells),
    }
}

/// The row-major elements of a list with their 1-based indices: a non-empty vector of scalars,
/// or a non-empty matrix of equal, non-empty rows of scalars.
fn shape(open: Location, items: Vec<Value>) -> Result<Vec<(Vec<usize>, Scalar)>, ParseError> {
    let malformed = |why: &str| Err(ParseError::new(MALFORMED_LIST, open, why.to_owned()));
    if items.is_empty() {
        return malformed("a list has at least one item");
    }
    let rows = items
        .iter()
        .filter(|v| matches!(v, Value::List(..)))
        .count();
    if rows == 0 {
        return Ok(items
            .into_iter()
            .enumerate()
            .filter_map(|(i, v)| match v {
                Value::Scalar(s) => Some((vec![i + 1], s)),
                Value::List(..) => None,
            })
            .collect());
    }
    if rows != items.len() {
        return malformed("a list holds either values or rows, not both");
    }
    let mut out = Vec::new();
    let mut width = None;
    for (r, row) in items.into_iter().enumerate() {
        let Value::List(_, cells) = row else {
            return malformed("a list holds either values or rows, not both");
        };
        if cells.is_empty() {
            return malformed("a matrix row has at least one value");
        }
        if *width.get_or_insert(cells.len()) != cells.len() {
            return malformed("every row of a matrix has the same length");
        }
        for (c, cell) in cells.into_iter().enumerate() {
            match cell {
                Value::Scalar(s) => out.push((vec![r + 1, c + 1], s)),
                Value::List(..) => return malformed("a matrix row holds values"),
            }
        }
    }
    Ok(out)
}

/// The element-name template for list assignment `name` of rank `rank` on `class_path`: the
/// catalog's array template whose base is `name` (`k_<i>`, `K_<row>_<col>`), or, when the class
/// or the parameter is unknown, the same shape built from `name`, so the validator reports the
/// element names it does not know (`unknown-parameter`).
pub(in crate::text) fn element_template(class_path: &str, name: &str, rank: usize) -> String {
    let catalog = lookup_class(class_path).and_then(|class| {
        class
            .metadata
            .parameters
            .iter()
            .find(|p| {
                is_template(p.name)
                    && template_base(p.name) == name
                    && template_rank(p.name) == rank
            })
            .map(|p| p.name.to_owned())
    });
    catalog.unwrap_or_else(|| {
        if rank == 1 {
            format!("{name}_{PARAM_INDEX_PLACEHOLDER}")
        } else {
            format!("{name}_{PARAM_ROW_PLACEHOLDER}_{PARAM_COL_PLACEHOLDER}")
        }
    })
}

/// The base name of an array template: `k_<i>` is `k`, `K_<row>_<col>` is `K`.
pub(in crate::text) fn template_base(template: &str) -> &str {
    template.split_once("_<").map_or(template, |(base, _)| base)
}

/// The number of index placeholders in a template.
pub(in crate::text) fn template_rank(template: &str) -> usize {
    [
        PARAM_INDEX_PLACEHOLDER,
        PARAM_ROW_PLACEHOLDER,
        PARAM_COL_PLACEHOLDER,
    ]
    .iter()
    .map(|p| template.matches(p).count())
    .sum()
}
