//! What the text form can spell: the lexical check [`crate::print_text`] runs before it writes
//! anything, so it either refuses a model with a typed [`PrintError`] or writes text that parses
//! back to the same model (issue #482 review, ruling A). The printer then refuses, with the same
//! error, a statement whose printed line would pass the line bound and a document that would pass
//! the byte or line bound, so every bound `parse_text` enforces is enforced here first.
//!
//! A model from the wire form or the builder can hold values the lexer cannot read back: a name
//! with a space or a line feed, an atom with a `#`, a class path that is not dotted words, a
//! suffix that is neither a base unit nor a scaled one, a scaled suffix whose written number does
//! not scale to the value bit for bit ([`crate::Assignment::scaled_from`]), a `derived_from` id of
//! `none`, a reference named `true`, an enumeration member with no type, an enumeration type
//! spelled as a scalar type or not as one word, a non-finite Real or bound, a string with a
//! control character or a bidirectional override. Each is refused here, naming the subject by the validator's model
//! path. Comments are the exception: a line feed in a comment splits it into several comment
//! lines and trailing spaces are dropped, the two normalizations the parser applies itself; a
//! comment piece the lexer would refuse is refused here.

use std::fmt;

use crate::model::{
    Annotations, Bound, Boundary, Endpoint, Literal, MemberRef, ParamValue, ParameterType,
    SemanticPoint, Sequence,
};

use super::codes::{MAX_COMMENT_BYTES, MAX_STATEMENTS_PER_KIND, MAX_STRING_BYTES, MAX_TOKEN_BYTES};
use super::is_bidi_control;
use super::units::agreement;

/// Why a model has no text spelling. Never a panic.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrintError {
    /// A name, class path, atom, suffix, string, value, comment or `derived_from` id that the
    /// text form's lexer could not read back as the same value.
    Unspellable {
        /// The model path of what holds it (`sequence`, `derived_from`, `TOut`, `hys.uHigh`,
        /// `connection[2]`), as the validator names subjects.
        subject: String,
        /// What cannot be spelled, and why.
        detail: String,
    },
}

impl fmt::Display for PrintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unspellable { subject, detail } => {
                write!(f, "unspellable at {subject}: {detail}")
            }
        }
    }
}

impl std::error::Error for PrintError {}

/// The words that start a statement: a connection source cannot begin with one.
const STATEMENT_WORDS: &[&str] = &["sequence", "derived_from", "param", "in", "out", "block"];

type Checked = Result<(), String>;

fn word(text: &str) -> Checked {
    let bytes = text.as_bytes();
    let starts = bytes
        .first()
        .is_some_and(|&b| b.is_ascii_alphabetic() || b == b'_');
    if !starts
        || !bytes
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(format!("`{text}` is not a word `[A-Za-z_][A-Za-z0-9_]*`"));
    }
    if bytes.len() > MAX_TOKEN_BYTES {
        return Err(format!("`{text}` is longer than {MAX_TOKEN_BYTES} bytes"));
    }
    Ok(())
}

fn dotted(text: &str) -> Checked {
    text.split('.').try_for_each(word)
}

/// A word on the right of `=` read as a reference: not a Boolean or a non-finite word.
fn reference(text: &str) -> Checked {
    word(text)?;
    if matches!(text, "true" | "false")
        || ["nan", "inf", "infinity"]
            .iter()
            .any(|w| w.eq_ignore_ascii_case(text))
    {
        return Err(format!("a reference named `{text}` reads as a literal"));
    }
    Ok(())
}

fn atom(text: &str) -> Checked {
    if text.is_empty() || text.len() > MAX_TOKEN_BYTES {
        return Err(format!(
            "an atom is 1 to {MAX_TOKEN_BYTES} bytes; this one is {}",
            text.len()
        ));
    }
    match text
        .bytes()
        .position(|b| !(b'!'..=b'~').contains(&b) || b == b'#')
    {
        Some(at) => Err(format!(
            "atom `{text}`: byte {at} is not printable ASCII without space or `#`"
        )),
        None => Ok(()),
    }
}

fn string(text: &str) -> Checked {
    if let Some(c) = text
        .chars()
        .find(|&c| (c.is_control() && c != '\n') || is_bidi_control(c))
    {
        return Err(format!(
            "a string carries U+{:04X}, which the text form refuses",
            u32::from(c)
        ));
    }
    let written: usize = text
        .chars()
        .map(|c| match c {
            '"' | '\\' | '\n' => 2,
            c => c.len_utf8(),
        })
        .sum();
    if written > MAX_STRING_BYTES {
        return Err(format!(
            "a string is {written} bytes as written; at most {MAX_STRING_BYTES}"
        ));
    }
    Ok(())
}

fn member(m: &MemberRef) -> Checked {
    if m.type_name.is_empty() {
        return Err(format!(
            "the member `{}` has no `Type.` prefix and would read as a reference",
            m.member
        ));
    }
    // The parser splits a member at its first dot, so the type is one word.
    reference(&m.type_name)?;
    dotted(&m.member)
}

fn literal(value: &Literal) -> Checked {
    match value {
        Literal::Real(v) if !v.is_finite() => Err(format!("the Real {v} is not finite")),
        Literal::Text(text) => string(text),
        Literal::Member(m) => member(m),
        _ => Ok(()),
    }
}

/// A parameter type: an enumeration type name is one word, and not a scalar type's spelling
/// (which would read back as the scalar type).
fn parameter_type(value_type: &ParameterType) -> Checked {
    let ParameterType::Enum(type_name) = value_type else {
        return Ok(());
    };
    word(type_name)?;
    if ParameterType::parse(type_name) != *value_type {
        return Err(format!(
            "the enumeration type `{type_name}` reads as the scalar type `{type_name}`"
        ));
    }
    Ok(())
}

/// A boundary's bounds: each finite.
fn bounds(b: &Boundary) -> Checked {
    for (side, bound) in [("min", b.min), ("max", b.max)] {
        if let Some(Bound::Real(v)) = bound
            && !v.is_finite()
        {
            return Err(format!("the `{side}` bound {v} is not finite"));
        }
    }
    Ok(())
}

/// A suffix's agreement with the value and the written number a scaled suffix keeps, its token
/// bound included ([`super::units::agreement`], which stage 1 runs too, so no suffix stops a
/// model stage 1 accepts from printing).
fn suffix(value: &ParamValue, suffix: Option<&String>, scaled_from: Option<&Literal>) -> Checked {
    agreement(value, suffix.map(String::as_str), scaled_from)
}

fn comment_lines(lines: &[String]) -> Checked {
    for line in lines.iter().flat_map(|l| l.split('\n')) {
        let line = line.trim_end_matches(' ');
        if let Some(c) = line.chars().find(|&c| c.is_control() || is_bidi_control(c)) {
            return Err(format!("a comment carries U+{:04X}", u32::from(c)));
        }
        if line.len() > MAX_COMMENT_BYTES {
            return Err(format!(
                "a comment line is {} bytes; at most {MAX_COMMENT_BYTES}",
                line.len()
            ));
        }
    }
    Ok(())
}

fn annotations(a: &Annotations) -> Checked {
    comment_lines(&a.comments)
}

fn semantic(b: &SemanticPoint, quantity: Option<&String>, display: Option<&String>) -> Checked {
    let atoms = [&b.unit, &b.brick, &b.s223, &b.qudt];
    for value in atoms.into_iter().flatten().chain(quantity).chain(display) {
        atom(value)?;
    }
    b.haystack.iter().try_for_each(|t| atom(t))?;
    b.name.iter().chain(&b.label).try_for_each(|t| string(t))
}

fn endpoint(e: &Endpoint, source: bool) -> Checked {
    let first = match e {
        Endpoint::Boundary(name) => {
            word(name.as_str())?;
            name.as_str()
        }
        Endpoint::Port { block, port } => {
            word(block.as_str())?;
            dotted(port.as_str())?;
            block.as_str()
        }
    };
    if source && STATEMENT_WORDS.contains(&first) {
        return Err(format!(
            "a connection source `{first}` reads as a statement"
        ));
    }
    Ok(())
}

/// Refuse a model with no text spelling; see the module documentation.
pub(super) fn check(s: &Sequence) -> Result<(), PrintError> {
    let at = |subject: &str| {
        let subject = subject.to_owned();
        move |detail: String| PrintError::Unspellable {
            subject: subject.clone(),
            detail,
        }
    };
    let kinds = [
        ("parameters", s.params.len()),
        ("inputs", s.inputs.len()),
        ("outputs", s.outputs.len()),
        ("blocks", s.blocks.len()),
        ("connections", s.connections.len()),
    ];
    if let Some((kind, len)) = kinds.iter().find(|(_, len)| *len > MAX_STATEMENTS_PER_KIND) {
        return Err(at("sequence")(format!(
            "{len} {kind}; the text form reads at most {MAX_STATEMENTS_PER_KIND} of one kind"
        )));
    }
    if let Some(b) = s
        .blocks
        .iter()
        .find(|b| b.assignments.len() > MAX_STATEMENTS_PER_KIND)
    {
        return Err(at(b.name.as_str())(format!(
            "{} assignments; the text form reads at most {MAX_STATEMENTS_PER_KIND} in one block",
            b.assignments.len()
        )));
    }
    word(s.name.as_str())
        .and_then(|()| comment_lines(&s.annotations.comments))
        .and_then(|()| comment_lines(&s.trailing_comments))
        .map_err(at("sequence"))?;
    if let Some(id) = &s.derived_from {
        let id = id.as_str();
        let ok = !id.is_empty()
            && id.len() <= MAX_TOKEN_BYTES
            && id != "none"
            && id.bytes().all(|b| (b'!'..=b'~').contains(&b));
        if !ok {
            return Err(at("derived_from")(format!(
                "`{id}` is not 1 to {MAX_TOKEN_BYTES} bytes of printable ASCII other than `none`"
            )));
        }
    }
    for p in &s.params {
        let name = p.name.as_str();
        word(name)
            .and_then(|()| parameter_type(&p.value_type))
            .and_then(|()| literal(&p.value))
            .and_then(|()| {
                suffix(
                    &ParamValue::Literal(p.value.clone()),
                    p.suffix.as_ref(),
                    p.scaled_from.as_ref(),
                )
            })
            .and_then(|()| annotations(&p.annotations))
            .map_err(at(name))?;
    }
    for b in s.inputs.iter().chain(&s.outputs) {
        let name = b.name.as_str();
        word(name)
            .and_then(|()| semantic(&b.semantic, b.quantity.as_ref(), b.display_unit.as_ref()))
            .and_then(|()| bounds(b))
            .and_then(|()| annotations(&b.annotations))
            .map_err(at(name))?;
    }
    for b in &s.blocks {
        let name = b.name.as_str();
        word(name)
            .and_then(|()| dotted(&b.class_path))
            .and_then(|()| annotations(&b.annotations))
            .map_err(at(name))?;
        for a in &b.assignments {
            let path = format!("{name}.{}", a.name);
            word(a.name.as_str())
                .and_then(|()| match &a.value {
                    ParamValue::Literal(l) => literal(l),
                    ParamValue::Ref(r) => reference(r.as_str()),
                })
                .and_then(|()| suffix(&a.value, a.suffix.as_ref(), a.scaled_from.as_ref()))
                .and_then(|()| annotations(&a.annotations))
                .map_err(at(&path))?;
        }
    }
    for (i, c) in s.connections.iter().enumerate() {
        endpoint(&c.source, true)
            .and_then(|()| endpoint(&c.target, false))
            .and_then(|()| annotations(&c.annotations))
            .map_err(at(&format!("connection[{i}]")))?;
    }
    Ok(())
}
