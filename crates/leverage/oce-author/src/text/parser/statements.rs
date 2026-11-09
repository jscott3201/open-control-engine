//! The declaration statements: `param`, `in` / `out` with their tags, and `block` with its
//! parameter list.
//!
//! A sequence parameter's type is `Real`, `Integer`, `Boolean` or any other word, read as an
//! enumeration type name the validator resolves (the parser stays catalog-free; issue #492). A
//! boundary's type is one of the three scalars. `@min` and `@max` take one number token, with no
//! unit suffix.

use crate::location::Location;
use crate::model::{
    Annotations, Block, Bound, Boundary, Literal, ParameterType, SequenceParameter, SignalType,
};

use super::super::ParseError;
use super::super::codes::{
    COMMENT_BEFORE_CLOSE, DUPLICATE_TAG, MALFORMED_ATOM, MALFORMED_NUMBER, MISPLACED_VALUE,
    NON_FINITE_NUMBER, STRAY_TOKEN, UNCLOSED_BRACKET, UNKNOWN_TAG, UNKNOWN_TYPE,
};
use super::super::lexer::Tok;
use super::values::{Value, non_finite_word, number};
use super::{Parser, room};

/// The boundary tags, in canonical order.
pub(in crate::text) const TAGS: &[&str] = &[
    "unit",
    "quantity",
    "displayUnit",
    "min",
    "max",
    "brick",
    "s223",
    "qudt",
    "haystack",
    "name",
    "label",
    "provisional",
];

impl Parser<'_> {
    /// `":" type`.
    fn signal_type(&mut self) -> Result<SignalType, ParseError> {
        self.punct(b':', "before the type")?;
        let token = self.lx.peek()?;
        let found = token.text(self.lx.src());
        let value_type = match (&token.tok, found) {
            (Tok::Word, "Real") => SignalType::Real,
            (Tok::Word, "Integer") => SignalType::Integer,
            (Tok::Word, "Boolean") => SignalType::Boolean,
            (Tok::Word, _) => {
                return Err(ParseError::new(
                    UNKNOWN_TYPE,
                    token.at,
                    format!("`{found}` is not a type; write `Real`, `Integer` or `Boolean`"),
                ));
            }
            _ => {
                return Err(ParseError::new(
                    STRAY_TOKEN,
                    token.at,
                    format!("expected a type, found {}", token.describe(self.lx.src())),
                ));
            }
        };
        self.lx.bump(&token);
        Ok(value_type)
    }

    /// `":" type` of a sequence parameter: a scalar type or an enumeration type name.
    fn parameter_type(&mut self) -> Result<ParameterType, ParseError> {
        self.punct(b':', "before the type")?;
        let token = self.lx.peek()?;
        if token.tok != Tok::Word {
            return Err(ParseError::new(
                STRAY_TOKEN,
                token.at,
                format!("expected a type, found {}", token.describe(self.lx.src())),
            ));
        }
        let value_type = ParameterType::parse(token.text(self.lx.src()));
        self.lx.bump(&token);
        Ok(value_type)
    }

    /// `param name ":" type "=" value`.
    pub(super) fn param(&mut self, keyword: Location) -> Result<(), ParseError> {
        room(self.sequence.params.len(), keyword, "sequence parameters")?;
        let annotations = self.take_pending();
        let (name, at) = self.name("a parameter name")?;
        let value_type = self.parameter_type()?;
        self.punct(b'=', "before the value")?;
        let next = self.lx.peek()?;
        if next.tok == Tok::Punct(b'[') {
            return Err(ParseError::new(
                MISPLACED_VALUE,
                next.at,
                "a sequence parameter takes one literal, not a list",
            ));
        }
        let Value::Scalar(value) = self.value(0, &mut Annotations::default())? else {
            return Err(ParseError::new(
                MISPLACED_VALUE,
                next.at,
                "a sequence parameter takes one literal, not a list",
            ));
        };
        let crate::model::ParamValue::Literal(literal) = value.value else {
            return Err(ParseError::new(
                MISPLACED_VALUE,
                value.at,
                "a sequence parameter takes a literal, not a reference to another parameter",
            ));
        };
        let mut param = SequenceParameter::new(name, value_type, literal);
        param.suffix = value.suffix;
        param.annotations = annotations;
        if let Some(comment) = self.end_statement()? {
            param.annotations.comments.push(comment);
        }
        self.sequence.params.push(param);
        self.spans.params.push(at);
        Ok(())
    }

    /// `("in" | "out") name ":" type { tag }`.
    pub(super) fn boundary(&mut self, keyword: Location, input: bool) -> Result<(), ParseError> {
        let len = if input {
            self.sequence.inputs.len()
        } else {
            self.sequence.outputs.len()
        };
        room(len, keyword, if input { "inputs" } else { "outputs" })?;
        let annotations = self.take_pending();
        let (name, at) = self.name("a connector name")?;
        let mut boundary = Boundary::new(name, self.signal_type()?);
        boundary.annotations = annotations;
        let mut seen: Vec<&str> = Vec::new();
        while self.lx.peek()?.tok == Tok::Punct(b'@') {
            let sigil = self.lx.next()?;
            let token = self.lx.peek()?;
            let word = token.text(self.lx.src());
            let Some(tag) = TAGS
                .iter()
                .copied()
                .find(|t| token.tok == Tok::Word && *t == word)
            else {
                return Err(ParseError::new(
                    UNKNOWN_TAG,
                    sigil.at,
                    format!("`@{word}` is not a tag; the tags are @{}", TAGS.join(", @")),
                ));
            };
            self.lx.bump(&token);
            if tag != "haystack" && seen.contains(&tag) {
                return Err(ParseError::new(
                    DUPLICATE_TAG,
                    sigil.at,
                    format!("`@{tag}` is written twice; only `@haystack` repeats"),
                ));
            }
            seen.push(tag);
            self.tag(tag, &mut boundary)?;
        }
        if let Some(comment) = self.end_statement()? {
            boundary.annotations.comments.push(comment);
        }
        if input {
            self.sequence.inputs.push(boundary);
            self.spans.inputs.push(at);
        } else {
            self.sequence.outputs.push(boundary);
            self.spans.outputs.push(at);
        }
        Ok(())
    }

    /// The number operand of `@min` / `@max`: one number token, with no unit suffix.
    fn bound(&mut self, tag: &str) -> Result<Bound, ParseError> {
        let token = self.lx.peek()?;
        let word = token.text(self.lx.src());
        if token.tok == Tok::Word && non_finite_word(word) || token.tok == Tok::Punct(b'-') {
            return Err(ParseError::new(
                if token.tok == Tok::Word {
                    NON_FINITE_NUMBER
                } else {
                    STRAY_TOKEN
                },
                token.at,
                format!("`@{tag}` takes a finite number with no space after its sign"),
            ));
        }
        if token.tok != Tok::Number {
            return Err(ParseError::new(
                STRAY_TOKEN,
                token.at,
                format!(
                    "expected a number after `@{tag}`, found {}",
                    token.describe(self.lx.src())
                ),
            ));
        }
        let bound = match number(token.text(self.lx.src()), token.at)? {
            Literal::Integer(v) => Bound::Integer(v),
            Literal::Real(v) => Bound::Real(v),
            other => {
                return Err(ParseError::new(
                    MALFORMED_NUMBER,
                    token.at,
                    format!("`@{tag}` takes a number, not {}", other.kind_name()),
                ));
            }
        };
        self.lx.bump(&token);
        let next = self.lx.peek()?;
        if !matches!(
            next.tok,
            Tok::Punct(b'@') | Tok::Newline | Tok::Comment { .. } | Tok::Eof
        ) {
            return Err(ParseError::new(
                STRAY_TOKEN,
                next.at,
                format!(
                    "a bound takes no unit suffix; expected a tag or the end of the line after `@{tag}`, found {}",
                    next.describe(self.lx.src())
                ),
            ));
        }
        Ok(bound)
    }

    /// One tag's operand: an atom, a string (`@name`, `@label`), a number (`@min`, `@max`) or
    /// nothing (`@provisional`).
    fn tag(&mut self, tag: &str, boundary: &mut Boundary) -> Result<(), ParseError> {
        match tag {
            "min" => {
                boundary.min = Some(self.bound(tag)?);
                return Ok(());
            }
            "max" => {
                boundary.max = Some(self.bound(tag)?);
                return Ok(());
            }
            _ => {}
        }
        let semantic = &mut boundary.semantic;
        match tag {
            "provisional" => semantic.provisional = true,
            "name" | "label" => {
                let token = self.lx.peek()?;
                let Tok::Str(text) = token.tok.clone() else {
                    return Err(ParseError::new(
                        STRAY_TOKEN,
                        token.at,
                        format!(
                            "expected a string after `@{tag}`, found {}",
                            token.describe(self.lx.src())
                        ),
                    ));
                };
                self.lx.bump(&token);
                if tag == "name" {
                    semantic.name = Some(text);
                } else {
                    semantic.label = Some(text);
                }
            }
            _ => {
                let Some((atom, at)) = self.lx.raw_run(b"#")? else {
                    let token = self.lx.peek()?;
                    return Err(ParseError::new(
                        STRAY_TOKEN,
                        token.at,
                        format!(
                            "expected an atom after `@{tag}`, found {}",
                            token.describe(self.lx.src())
                        ),
                    ));
                };
                if let Some(bad) = atom.bytes().position(|b| !(b'!'..=b'~').contains(&b)) {
                    return Err(ParseError::new(
                        MALFORMED_ATOM,
                        at,
                        format!(
                            "the `@{tag}` atom `{atom}` has a byte other than printable ASCII at byte {bad}"
                        ),
                    ));
                }
                match tag {
                    "unit" => semantic.unit = Some(atom),
                    "quantity" => boundary.quantity = Some(atom),
                    "displayUnit" => boundary.display_unit = Some(atom),
                    "brick" => semantic.brick = Some(atom),
                    "s223" => semantic.s223 = Some(atom),
                    "qudt" => semantic.qudt = Some(atom),
                    _ => semantic.haystack.push(atom),
                }
            }
        }
        Ok(())
    }

    /// `block name "=" class-path [ "(" [ assign { "," assign } ] ")" ]`.
    pub(super) fn block(&mut self, keyword: Location) -> Result<(), ParseError> {
        room(self.sequence.blocks.len(), keyword, "blocks")?;
        let annotations = self.take_pending();
        let (name, at) = self.name("a block name")?;
        self.punct(b'=', "before the class path")?;
        let (class_path, _) = self.dotted("a class path")?;
        let mut block = Block::new(name, class_path);
        block.annotations = annotations;
        self.sequence.blocks.push(block);
        self.spans.blocks.push(at);
        self.spans.assignments.push(Vec::new());
        let index = self.sequence.blocks.len() - 1;
        if self.lx.peek()?.tok == Tok::Punct(b'(') {
            self.assignments(index)?;
        }
        if let Some(comment) = self.end_statement()? {
            self.sequence.blocks[index]
                .annotations
                .comments
                .push(comment);
        }
        Ok(())
    }

    /// The parenthesized assignment list of block `index`, which may span lines.
    fn assignments(&mut self, index: usize) -> Result<(), ParseError> {
        let open = self.lx.next()?;
        let mut pending = Annotations::default();
        let mut pending_at: Option<Location> = None;
        let mut owner: Option<usize> = None;
        let mut expect_item = true;
        let mut items = 0usize;
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
                    let block = &mut self.sequence.blocks[index];
                    match (eol, owner) {
                        (true, Some(j)) => {
                            block.assignments[j].annotations.comments.push(text.clone())
                        }
                        (true, None) => block.annotations.comments.push(text.clone()),
                        (false, _) => {
                            pending_at.get_or_insert(token.at);
                            pending.comments.push(text.clone());
                        }
                    }
                    self.lx.bump(&token);
                }
                Tok::Punct(b')') => {
                    if expect_item && items > 0 {
                        return Err(ParseError::new(
                            STRAY_TOKEN,
                            token.at,
                            "expected an assignment after `,`, found `)`",
                        ));
                    }
                    if let Some(at) = pending_at.filter(|_| !pending.comments.is_empty()) {
                        return Err(ParseError::new(
                            COMMENT_BEFORE_CLOSE,
                            at,
                            "a comment after the last assignment precedes nothing; move it above the block or an assignment",
                        ));
                    }
                    self.lx.bump(&token);
                    return Ok(());
                }
                Tok::Eof => {
                    return Err(ParseError::new(
                        UNCLOSED_BRACKET,
                        open.at,
                        "the block's `(` is never closed",
                    ));
                }
                Tok::Punct(b',') if !expect_item => {
                    self.lx.bump(&token);
                    expect_item = true;
                }
                Tok::Word if expect_item => {
                    let annotations = std::mem::take(&mut pending);
                    pending_at = None;
                    owner = Some(self.assignment(index, annotations)?);
                    expect_item = false;
                    items += 1;
                }
                _ => {
                    let want = if expect_item {
                        "an assignment or `)`"
                    } else {
                        "`,` or `)`"
                    };
                    return Err(ParseError::new(
                        STRAY_TOKEN,
                        token.at,
                        format!("expected {want}, found {}", token.describe(self.lx.src())),
                    ));
                }
            }
        }
    }
}
