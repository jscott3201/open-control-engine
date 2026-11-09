//! The bounded lexical pass that runs before the typed parse.
//!
//! `serde_json` parses the closed DTOs, but it cannot see the bounds the wire form promises
//! (nesting depth, list lengths, string lengths), duplicate keys in an object it maps onto a
//! struct field it has already filled, control characters spelled as escapes, or the lexical
//! difference between an integer token too large for an `i64` and a real. This pass walks the
//! document once, iteratively bounded by [`MAX_WIRE_DEPTH`], and refuses those with a typed
//! [`WireError`]. It is not a second JSON parser: on anything it does not recognise as JSON it
//! stops and leaves the syntax error to `serde_json`, which reports it with a position.

use std::collections::BTreeSet;

use super::{MAX_WIRE_DEPTH, MAX_WIRE_LIST_LEN, MAX_WIRE_STRING_BYTES, Position, WireError};

/// Why the pass stopped early.
enum Halt {
    /// A typed refusal.
    Refuse(WireError),
    /// Not JSON this pass understands; the typed parse reports the syntax error.
    Defer,
}

type Step<T> = Result<T, Halt>;

/// Scan `text` for the bounds and lexical refusals. `Ok` means "nothing to refuse here", not
/// "valid JSON", and carries the byte offsets of every integer token spelled `-0`: `serde_json`
/// reads that token as the real `-0.0`, but it has no fraction or exponent, so it is the Integer
/// zero, and the caller blanks its sign before the typed parse.
pub(super) fn scan(text: &str) -> Result<Vec<usize>, WireError> {
    let mut scanner = Scanner {
        bytes: text.as_bytes(),
        at: 0,
        line: 1,
        line_start: 0,
        negative_zeros: Vec::new(),
    };
    match scanner.document() {
        Ok(()) | Err(Halt::Defer) => Ok(scanner.negative_zeros),
        Err(Halt::Refuse(error)) => Err(error),
    }
}

struct Scanner<'a> {
    bytes: &'a [u8],
    at: usize,
    line: usize,
    line_start: usize,
    negative_zeros: Vec<usize>,
}

impl<'a> Scanner<'a> {
    fn position(&self) -> Position {
        Position {
            line: self.line,
            column: self.at - self.line_start + 1,
        }
    }

    fn refuse<T>(&self, make: impl FnOnce(Position) -> WireError) -> Step<T> {
        Err(Halt::Refuse(make(self.position())))
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn skip_whitespace(&mut self) {
        while let Some(byte) = self.peek() {
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

    fn expect(&mut self, byte: u8) -> Step<()> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(Halt::Defer)
        }
    }

    fn document(&mut self) -> Step<()> {
        self.skip_whitespace();
        self.value(0)?;
        self.skip_whitespace();
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(Halt::Defer)
        }
    }

    /// One value at nesting `depth` (the number of enclosing objects and arrays).
    fn value(&mut self, depth: usize) -> Step<()> {
        let rest = &self.bytes[self.at..];
        if rest.starts_with(b"NaN")
            || rest.starts_with(b"Infinity")
            || rest.starts_with(b"-Infinity")
        {
            return self.refuse(|at| WireError::NonFiniteNumber { at });
        }
        match self.peek() {
            Some(b'{') => self.object(depth + 1),
            Some(b'[') => self.array(depth + 1),
            Some(b'"') => self.string().map(|_| ()),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b't') => self.word(b"true"),
            Some(b'f') => self.word(b"false"),
            Some(b'n') => self.word(b"null"),
            _ => Err(Halt::Defer),
        }
    }

    fn word(&mut self, word: &[u8]) -> Step<()> {
        if self.bytes[self.at..].starts_with(word) {
            self.at += word.len();
            Ok(())
        } else {
            Err(Halt::Defer)
        }
    }

    fn enter(&self, depth: usize) -> Step<()> {
        if depth > MAX_WIRE_DEPTH {
            return self.refuse(|at| WireError::TooDeep {
                at,
                limit: MAX_WIRE_DEPTH,
            });
        }
        Ok(())
    }

    fn object(&mut self, depth: usize) -> Step<()> {
        self.enter(depth)?;
        self.expect(b'{')?;
        let mut keys = BTreeSet::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(());
        }
        loop {
            self.skip_whitespace();
            let key_at = self.position();
            let (start, end) = self.string()?;
            let bytes: &'a [u8] = self.bytes;
            let key = &bytes[start..end];
            if !keys.insert(key) {
                let key = String::from_utf8_lossy(key).into_owned();
                return Err(Halt::Refuse(WireError::DuplicateKey { key, at: key_at }));
            }
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();
            self.value(depth)?;
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(());
                }
                _ => return Err(Halt::Defer),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Step<()> {
        self.enter(depth)?;
        let opened = self.position();
        self.expect(b'[')?;
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(());
        }
        let mut len = 0usize;
        loop {
            len += 1;
            if len > MAX_WIRE_LIST_LEN {
                return Err(Halt::Refuse(WireError::ListTooLong {
                    at: opened,
                    limit: MAX_WIRE_LIST_LEN,
                }));
            }
            self.skip_whitespace();
            self.value(depth)?;
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(());
                }
                _ => return Err(Halt::Defer),
            }
        }
    }

    /// A string token; returns the byte range of its raw (still escaped) content.
    fn string(&mut self) -> Step<(usize, usize)> {
        let opened = self.position();
        self.expect(b'"')?;
        let start = self.at;
        loop {
            let Some(byte) = self.peek() else {
                return Err(Halt::Defer);
            };
            match byte {
                b'"' => break,
                b'\\' => {
                    self.at += 1;
                    match self.peek() {
                        Some(b'"' | b'\\' | b'/' | b'n') => self.at += 1,
                        Some(b'b' | b'f' | b'r' | b't') => {
                            return self.refuse(|at| WireError::ControlCharacter { at });
                        }
                        Some(b'u') => {
                            let hex = self
                                .bytes
                                .get(self.at + 1..self.at + 5)
                                .ok_or(Halt::Defer)?;
                            let hex = std::str::from_utf8(hex).map_err(|_| Halt::Defer)?;
                            let code = u32::from_str_radix(hex, 16).map_err(|_| Halt::Defer)?;
                            if is_refused_control(code) {
                                return self.refuse(|at| WireError::ControlCharacter { at });
                            }
                            self.at += 5;
                        }
                        _ => return Err(Halt::Defer),
                    }
                }
                0x00..=0x1F | 0x7F => {
                    return self.refuse(|at| WireError::ControlCharacter { at });
                }
                // U+0080..U+009F (C1 controls) are 0xC2 0x80..=0x9F in UTF-8.
                0xC2 if self
                    .bytes
                    .get(self.at + 1)
                    .is_some_and(|next| (0x80..=0x9F).contains(next)) =>
                {
                    return self.refuse(|at| WireError::ControlCharacter { at });
                }
                _ => self.at += 1,
            }
        }
        let end = self.at;
        if end - start > MAX_WIRE_STRING_BYTES {
            return Err(Halt::Refuse(WireError::StringTooLong {
                at: opened,
                limit: MAX_WIRE_STRING_BYTES,
            }));
        }
        self.at += 1;
        Ok((start, end))
    }

    fn number(&mut self) -> Step<()> {
        let at = self.position();
        let start = self.at;
        while self
            .peek()
            .is_some_and(|b| matches!(b, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
        {
            self.at += 1;
        }
        let token = std::str::from_utf8(&self.bytes[start..self.at]).map_err(|_| Halt::Defer)?;
        if token.contains(['.', 'e', 'E']) {
            let value: f64 = token.parse().map_err(|_| Halt::Defer)?;
            if !value.is_finite() {
                return Err(Halt::Refuse(WireError::NonFiniteNumber { at }));
            }
        } else if token == "-0" {
            self.negative_zeros.push(start);
        } else if token.parse::<i64>().is_err() {
            let digits = token.strip_prefix('-').unwrap_or(token);
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(Halt::Refuse(WireError::IntegerOutOfRange { at }));
            }
            return Err(Halt::Defer);
        }
        Ok(())
    }
}

/// Control characters a wire string may not carry: C0 except line feed (a text value may span
/// lines, as the text form's `\n` escape allows), DEL and C1.
fn is_refused_control(code: u32) -> bool {
    (code < 0x20 && code != 0x0A) || (0x7F..=0x9F).contains(&code)
}
