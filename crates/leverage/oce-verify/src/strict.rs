//! The strict, bounded JSON cursor every reader in this crate is written against (vectors note
//! decision 2).
//!
//! It is the authoring wire form's profile, with the same bounds and the same code spellings:
//! one UTF-8 JSON document, nesting at most [`MAX_DEPTH`], lists at most [`MAX_LIST_LEN`], string
//! tokens at most [`MAX_STRING_BYTES`], no duplicate key (compared after unescaping, so `"a"` and
//! `"a"` are one key), no control character in a string (anything in C0 but line feed, DEL
//! or C1, raw or escaped), no `NaN` or `Infinity`, no real that overflows binary64 and no integer
//! token outside `i64`. A number keeps the kind it was written as ([`Number`]): an integer token
//! has no fraction and no exponent, and `-0` is the integer zero.
//!
//! A [`Cursor`] reads one value at a time and never looks ahead further than one byte, so a typed
//! reader built on it stops at the first thing it does not expect and nothing is materialised for
//! a value nobody asked for. [`scan`] walks a whole document through the same cursor: it is how a
//! load refuses every lexical defect before it judges the shape. Recursion is bounded by
//! [`MAX_DEPTH`], and every refusal is a typed [`Refusal`] with a line and column, never a panic.

use std::collections::BTreeSet;

use crate::error::{Location, VectorError, VectorErrorKind};
use crate::limits::{MAX_DEPTH, MAX_LIST_LEN, MAX_STRING_BYTES};
use crate::number::Number;

/// A refusal at a place in the text.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Refusal {
    /// The closed kind.
    pub(crate) kind: VectorErrorKind,
    /// Where the reader stopped.
    pub(crate) at: Location,
    /// The sentence for people.
    pub(crate) detail: String,
}

impl Refusal {
    /// A refusal of `kind` at `at`.
    pub(crate) fn new(kind: VectorErrorKind, at: Location, detail: impl Into<String>) -> Self {
        Self {
            kind,
            at,
            detail: detail.into(),
        }
    }
}

impl From<Refusal> for VectorError {
    fn from(refusal: Refusal) -> Self {
        Self::in_text(refusal.kind, refusal.at, refusal.detail)
    }
}

/// The result of one read.
pub(crate) type Read<T> = Result<T, Refusal>;

/// The JSON type of the value a cursor stands before.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `{`.
    Object,
    /// `[`.
    Array,
    /// `"`.
    String,
    /// `-` or a digit.
    Number,
    /// `true` or `false`.
    Boolean,
    /// `null`.
    Null,
}

impl Kind {
    /// The kind with its article, for a `wrong-type` sentence.
    pub(crate) fn described(self) -> &'static str {
        match self {
            Self::Object => "an object",
            Self::Array => "a list",
            Self::String => "a string",
            Self::Number => "a number",
            Self::Boolean => "a boolean",
            Self::Null => "null",
        }
    }
}

/// The progress of one open list.
pub(crate) struct ListProgress {
    opened: Location,
    len: usize,
}

/// The progress of one open object.
pub(crate) struct ObjectProgress {
    opened: Location,
    any: bool,
}

impl ObjectProgress {
    /// Where the object's `{` is.
    pub(crate) fn opened(&self) -> Location {
        self.opened
    }
}

/// A position in one JSON text.
pub(crate) struct Cursor<'a> {
    text: &'a str,
    at: usize,
    line: usize,
    line_start: usize,
    depth: usize,
}

impl<'a> Cursor<'a> {
    /// A cursor at the start of `text`.
    pub(crate) fn new(text: &'a str) -> Self {
        Self {
            text,
            at: 0,
            line: 1,
            line_start: 0,
            depth: 0,
        }
    }

    /// The line and column of the next unread byte.
    pub(crate) fn location(&self) -> Location {
        Location::new(self.line, self.at - self.line_start + 1)
    }

    fn byte(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn rest_starts_with(&self, word: &str) -> bool {
        self.text.as_bytes()[self.at..].starts_with(word.as_bytes())
    }

    fn refuse<T>(&self, kind: VectorErrorKind, detail: &str) -> Read<T> {
        Err(Refusal::new(kind, self.location(), detail))
    }

    fn syntax<T>(&self, detail: &str) -> Read<T> {
        self.refuse(VectorErrorKind::JsonSyntax, detail)
    }

    fn skip_whitespace(&mut self) {
        while let Some(byte) = self.byte() {
            match byte {
                b' ' | b'\t' | b'\r' => self.at += 1,
                b'\n' => {
                    self.at += 1;
                    self.line += 1;
                    self.line_start = self.at;
                }
                _ => break,
            }
        }
    }

    /// The kind of the next value and where it starts, without consuming it.
    ///
    /// # Errors
    /// `non-finite-number` before `NaN`, `Infinity` or `-Infinity`; `json-syntax` when no value
    /// starts here.
    pub(crate) fn peek(&mut self) -> Read<(Kind, Location)> {
        self.skip_whitespace();
        let at = self.location();
        let kind = match self.byte() {
            Some(b'{') => Kind::Object,
            Some(b'[') => Kind::Array,
            Some(b'"') => Kind::String,
            Some(b'-') if self.rest_starts_with("-Infinity") => {
                return self.refuse(VectorErrorKind::NonFiniteNumber, "`-Infinity` is no number");
            }
            Some(b'-' | b'0'..=b'9') => Kind::Number,
            Some(b't' | b'f') => Kind::Boolean,
            Some(b'n') => Kind::Null,
            Some(b'N') if self.rest_starts_with("NaN") => {
                return self.refuse(VectorErrorKind::NonFiniteNumber, "`NaN` is no number");
            }
            Some(b'I') if self.rest_starts_with("Infinity") => {
                return self.refuse(VectorErrorKind::NonFiniteNumber, "`Infinity` is no number");
            }
            Some(_) => return self.syntax("expected a value"),
            None => return self.syntax("the document ends where a value is expected"),
        };
        Ok((kind, at))
    }

    /// Require the next value to be of `kind`; `what` names it in the refusal.
    ///
    /// # Errors
    /// `wrong-type` naming what was found, or any refusal of [`Self::peek`].
    pub(crate) fn expect(&mut self, kind: Kind, what: &str) -> Read<Location> {
        let (found, at) = self.peek()?;
        if found == kind {
            Ok(at)
        } else {
            Err(Refusal::new(
                VectorErrorKind::WrongType,
                at,
                format!("expected {what}, found {}", found.described()),
            ))
        }
    }

    fn enter(&mut self, at: Location) -> Read<()> {
        if self.depth >= MAX_DEPTH {
            return Err(Refusal::new(
                VectorErrorKind::NestingTooDeep,
                at,
                format!("nesting deeper than {MAX_DEPTH} levels"),
            ));
        }
        self.depth += 1;
        self.at += 1;
        Ok(())
    }

    /// Consume the `{` the cursor stands before ([`Self::peek`] said [`Kind::Object`]).
    ///
    /// # Errors
    /// `nesting-too-deep` past [`MAX_DEPTH`]; `json-syntax` when no `{` is here.
    pub(crate) fn open_object(&mut self) -> Read<ObjectProgress> {
        self.skip_whitespace();
        let opened = self.location();
        if self.byte() != Some(b'{') {
            return self.syntax("expected `{`");
        }
        self.enter(opened)?;
        Ok(ObjectProgress { opened, any: false })
    }

    /// The next key of an open object, unescaped, with its location; `None` once the object
    /// closes. The cursor is left before the key's value.
    ///
    /// # Errors
    /// `json-syntax` for a missing `,`, `:` or key; any refusal of a string token.
    pub(crate) fn next_key(
        &mut self,
        object: &mut ObjectProgress,
    ) -> Read<Option<(String, Location)>> {
        self.skip_whitespace();
        match (self.byte(), object.any) {
            (Some(b'}'), _) => {
                self.at += 1;
                self.depth -= 1;
                return Ok(None);
            }
            (Some(b','), true) => {
                self.at += 1;
                self.skip_whitespace();
            }
            (_, true) => return self.syntax("expected `,` or `}`"),
            (_, false) => {}
        }
        object.any = true;
        let at = self.location();
        if self.byte() != Some(b'"') {
            return self.syntax("expected a key");
        }
        let key = self.string()?;
        self.skip_whitespace();
        if self.byte() != Some(b':') {
            return self.syntax("expected `:` after a key");
        }
        self.at += 1;
        Ok(Some((key, at)))
    }

    /// Consume the `[` the cursor stands before ([`Self::peek`] said [`Kind::Array`]).
    ///
    /// # Errors
    /// `nesting-too-deep` past [`MAX_DEPTH`]; `json-syntax` when no `[` is here.
    pub(crate) fn open_list(&mut self) -> Read<ListProgress> {
        self.skip_whitespace();
        let opened = self.location();
        if self.byte() != Some(b'[') {
            return self.syntax("expected `[`");
        }
        self.enter(opened)?;
        Ok(ListProgress { opened, len: 0 })
    }

    /// Whether an open list has another item; `false` once it closes. On `true` the cursor is
    /// left before the item.
    ///
    /// # Errors
    /// `list-too-long` on the item past [`MAX_LIST_LEN`]; `json-syntax` for a missing `,`.
    pub(crate) fn next_item(&mut self, list: &mut ListProgress) -> Read<bool> {
        self.skip_whitespace();
        match (self.byte(), list.len) {
            (Some(b']'), _) => {
                self.at += 1;
                self.depth -= 1;
                return Ok(false);
            }
            // The first item follows the `[` directly; every later one follows a `,`.
            (_, 0) => {}
            (Some(b','), _) => self.at += 1,
            _ => return self.syntax("expected `,` or `]`"),
        }
        list.len += 1;
        if list.len > MAX_LIST_LEN {
            return Err(Refusal::new(
                VectorErrorKind::ListTooLong,
                list.opened,
                format!("the list has more than {MAX_LIST_LEN} entries"),
            ));
        }
        Ok(true)
    }

    /// Read a string token and unescape it.
    ///
    /// # Errors
    /// `string-too-long` once the token passes [`MAX_STRING_BYTES`]; `control-character` for a
    /// raw or escaped control character; `json-syntax` for an unterminated string, an unknown
    /// escape or a lone surrogate.
    pub(crate) fn string(&mut self) -> Read<String> {
        self.skip_whitespace();
        let opened = self.location();
        if self.byte() != Some(b'"') {
            return self.syntax("expected a string");
        }
        self.at += 1;
        let start = self.at;
        let mut out = String::new();
        let mut run = start;
        loop {
            if self.at - start > MAX_STRING_BYTES {
                return Err(Refusal::new(
                    VectorErrorKind::StringTooLong,
                    opened,
                    format!("the string is longer than {MAX_STRING_BYTES} bytes"),
                ));
            }
            let Some(byte) = self.byte() else {
                return self.syntax("the document ends inside a string");
            };
            match byte {
                b'"' => break,
                b'\\' => {
                    out.push_str(&self.text[run..self.at]);
                    out.push(self.escape()?);
                    run = self.at;
                }
                0x00..=0x1F | 0x7F => return self.control_character(),
                // U+0080..U+009F (C1 controls) are 0xC2 0x80..=0x9F in UTF-8.
                0xC2 if self
                    .text
                    .as_bytes()
                    .get(self.at + 1)
                    .is_some_and(|next| (0x80..=0x9F).contains(next)) =>
                {
                    return self.control_character();
                }
                _ => self.at += 1,
            }
        }
        out.push_str(&self.text[run..self.at]);
        self.at += 1;
        Ok(out)
    }

    fn control_character<T>(&self) -> Read<T> {
        self.refuse(
            VectorErrorKind::ControlCharacter,
            "a control character in a string",
        )
    }

    /// Read one escape; the cursor stands on its backslash and is left after it.
    fn escape(&mut self) -> Read<char> {
        let simple = match self.text.as_bytes().get(self.at + 1) {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'/') => '/',
            Some(b'n') => '\n',
            Some(b'b' | b'f' | b'r' | b't') => return self.control_character(),
            Some(b'u') => return self.unicode_escape(),
            _ => return self.syntax("an unknown escape in a string"),
        };
        self.at += 2;
        Ok(simple)
    }

    /// The four hex digits at byte offset `from`, as a UTF-16 code unit.
    fn code_unit(&self, from: usize) -> Option<u32> {
        let digits = self.text.as_bytes().get(from..from + 4)?;
        digits.iter().try_fold(0u32, |unit, digit| {
            let value = char::from(*digit).to_digit(16)?;
            Some(unit * 16 + value)
        })
    }

    /// Read a `\uXXXX` escape, or the surrogate pair it starts.
    fn unicode_escape(&mut self) -> Read<char> {
        let Some(unit) = self.code_unit(self.at + 2) else {
            return self.syntax("a `\\u` escape needs four hex digits");
        };
        let (scalar, len) = match unit {
            0xD800..=0xDBFF => {
                let bytes = self.text.as_bytes();
                let low = (bytes.get(self.at + 6) == Some(&b'\\')
                    && bytes.get(self.at + 7) == Some(&b'u'))
                .then(|| self.code_unit(self.at + 8))
                .flatten()
                .filter(|low| (0xDC00..=0xDFFF).contains(low));
                let Some(low) = low else {
                    return self.syntax("a lone surrogate in a `\\u` escape");
                };
                (0x1_0000 + ((unit - 0xD800) << 10) + (low - 0xDC00), 12)
            }
            0xDC00..=0xDFFF => return self.syntax("a lone surrogate in a `\\u` escape"),
            _ => (unit, 6),
        };
        if is_refused_control(scalar) {
            return self.control_character();
        }
        let Some(character) = char::from_u32(scalar) else {
            return self.syntax("a `\\u` escape that is no character");
        };
        self.at += len;
        Ok(character)
    }

    /// Read a number token, keeping its kind.
    ///
    /// # Errors
    /// `integer-out-of-range` for an integer token outside `i64`; `non-finite-number` for a real
    /// that overflows binary64; `json-syntax` for a token outside the JSON number grammar.
    pub(crate) fn number(&mut self) -> Read<Number> {
        self.skip_whitespace();
        let at = self.location();
        let start = self.at;
        if self.byte() == Some(b'-') {
            self.at += 1;
        }
        match self.byte() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return self.syntax("expected a digit in a number"),
        }
        let mut integer = true;
        if self.byte() == Some(b'.') {
            integer = false;
            self.at += 1;
            if !self.byte().is_some_and(|b| b.is_ascii_digit()) {
                return self.syntax("expected a digit after the decimal point");
            }
            self.digits();
        }
        if matches!(self.byte(), Some(b'e' | b'E')) {
            integer = false;
            self.at += 1;
            if matches!(self.byte(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if !self.byte().is_some_and(|b| b.is_ascii_digit()) {
                return self.syntax("expected a digit in the exponent");
            }
            self.digits();
        }
        let token = &self.text[start..self.at];
        if integer {
            return token.parse::<i64>().map(Number::Integer).map_err(|_| {
                Refusal::new(
                    VectorErrorKind::IntegerOutOfRange,
                    at,
                    "an integer outside the 64-bit signed range",
                )
            });
        }
        match token.parse::<f64>() {
            Ok(value) if value.is_finite() => Ok(Number::Real(value)),
            Ok(_) => Err(Refusal::new(
                VectorErrorKind::NonFiniteNumber,
                at,
                "a real that overflows binary64",
            )),
            Err(_) => Err(Refusal::new(
                VectorErrorKind::JsonSyntax,
                at,
                "a malformed number",
            )),
        }
    }

    fn digits(&mut self) {
        while self.byte().is_some_and(|b| b.is_ascii_digit()) {
            self.at += 1;
        }
    }

    fn word(&mut self, word: &str) -> Read<()> {
        self.skip_whitespace();
        if self.rest_starts_with(word) {
            self.at += word.len();
            Ok(())
        } else {
            self.syntax("expected a value")
        }
    }

    /// Read `true` or `false`.
    ///
    /// # Errors
    /// `json-syntax` when neither word is here.
    pub(crate) fn boolean(&mut self) -> Read<bool> {
        self.skip_whitespace();
        if self.byte() == Some(b't') {
            self.word("true").map(|()| true)
        } else {
            self.word("false").map(|()| false)
        }
    }

    /// Read one whole value of any type and discard it, refusing every lexical defect in it and
    /// every repeated key.
    ///
    /// # Errors
    /// Any strict-JSON refusal.
    pub(crate) fn skip_value(&mut self) -> Read<()> {
        match self.peek()?.0 {
            Kind::Object => {
                let mut object = self.open_object()?;
                let mut keys = BTreeSet::new();
                while let Some((key, at)) = self.next_key(&mut object)? {
                    unique(&mut keys, key, at)?;
                    self.skip_value()?;
                }
                Ok(())
            }
            Kind::Array => {
                let mut list = self.open_list()?;
                while self.next_item(&mut list)? {
                    self.skip_value()?;
                }
                Ok(())
            }
            Kind::String => self.string().map(|_| ()),
            Kind::Number => self.number().map(|_| ()),
            Kind::Boolean => self.boolean().map(|_| ()),
            Kind::Null => self.word("null"),
        }
    }

    /// Require that nothing but whitespace is left.
    ///
    /// # Errors
    /// `json-syntax` when anything follows the document.
    pub(crate) fn finish(&mut self) -> Read<()> {
        self.skip_whitespace();
        if self.at == self.text.len() {
            Ok(())
        } else {
            self.syntax("text after the end of the document")
        }
    }
}

/// Record `key` as written in its object, refusing a repeat.
fn unique(keys: &mut BTreeSet<String>, key: String, at: Location) -> Read<()> {
    if keys.contains(&key) {
        return Err(duplicate_key(&key, at));
    }
    keys.insert(key);
    Ok(())
}

/// The refusal of a repeated `key`.
pub(crate) fn duplicate_key(key: &str, at: Location) -> Refusal {
    Refusal::new(
        VectorErrorKind::DuplicateKey,
        at,
        format!("duplicate key `{key}`"),
    )
}

/// Control characters a string may not carry: C0 except line feed, DEL and C1.
fn is_refused_control(scalar: u32) -> bool {
    (scalar < 0x20 && scalar != 0x0A) || (0x7F..=0x9F).contains(&scalar)
}

/// Walk the whole of `text` as one strict JSON document and refuse the first lexical defect,
/// bound or repeated key in document order. `Ok` carries the root object's `schema` value when
/// the root is an object whose `schema` is a string, so a caller can refuse a foreign document by
/// its tag before judging its shape.
///
/// # Errors
/// Any strict-JSON refusal.
pub(crate) fn scan(text: &str) -> Read<Option<String>> {
    let mut cursor = Cursor::new(text);
    let mut tag = None;
    if cursor.peek()?.0 == Kind::Object {
        let mut object = cursor.open_object()?;
        let mut keys = BTreeSet::new();
        while let Some((key, at)) = cursor.next_key(&mut object)? {
            let is_tag = key == "schema";
            unique(&mut keys, key, at)?;
            if is_tag && cursor.peek()?.0 == Kind::String {
                tag = Some(cursor.string()?);
            } else {
                cursor.skip_value()?;
            }
        }
    } else {
        cursor.skip_value()?;
    }
    cursor.finish()?;
    Ok(tag)
}

#[cfg(test)]
#[path = "strict_tests.rs"]
mod tests;
