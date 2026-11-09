//! The text form's lexer: a bounded pre-scan of the whole document, then a pull lexer the
//! recursive-descent parser drives one token at a time.
//!
//! The pre-scan refuses what no grammar position may hold (oversize input, a byte-order mark,
//! control characters, too many lines, an overlong line) before a token is read. The lexer then
//! yields words, numbers, strings, comments, line feeds and punctuation, and reads the three
//! context-dependent runs (an atom, a unit suffix, a provenance id) on the parser's request with
//! [`Lexer::raw_run`]. Every refusal is a typed [`ParseError`]; nothing here can panic: every
//! slice is cut at an ASCII byte, which is always a character boundary.

use crate::location::Location;

use super::ParseError;
use super::codes::{
    BAD_ESCAPE, BYTE_ORDER_MARK, CONTROL_CHARACTER, INPUT_TOO_LARGE, LINE_TOO_LONG, MAX_LINE_BYTES,
    MAX_STRING_BYTES, MAX_TEXT_BYTES, MAX_TEXT_LINES, MAX_TOKEN_BYTES, STRAY_TOKEN,
    STRING_TOO_LONG, TOKEN_TOO_LONG, TOO_MANY_LINES, UNTERMINATED_STRING,
};

/// Refuse what no grammar position may hold, in document order.
pub(super) fn prescan(text: &str) -> Result<(), ParseError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(ParseError::new(
            INPUT_TOO_LARGE,
            Location::new(1, 1),
            format!(
                "the document is {} bytes; at most {MAX_TEXT_BYTES} are allowed",
                text.len()
            ),
        ));
    }
    if text.starts_with('\u{feff}') {
        return Err(ParseError::new(
            BYTE_ORDER_MARK,
            Location::new(1, 1),
            "the document starts with a byte-order mark",
        ));
    }
    let bytes = text.as_bytes();
    let mut line = 1usize;
    let mut line_start = 0usize;
    for (at, &byte) in bytes.iter().enumerate() {
        if at == line_start && line > MAX_TEXT_LINES {
            return Err(ParseError::new(
                TOO_MANY_LINES,
                Location::new(line, 1),
                format!("the document has more than {MAX_TEXT_LINES} lines"),
            ));
        }
        if byte == b'\n' {
            line += 1;
            line_start = at + 1;
            continue;
        }
        let column = at - line_start + 1;
        if column > MAX_LINE_BYTES {
            return Err(ParseError::new(
                LINE_TOO_LONG,
                Location::new(line, column),
                format!("line {line} is longer than {MAX_LINE_BYTES} bytes"),
            ));
        }
        let c1 = byte == 0xC2 && bytes.get(at + 1).is_some_and(|b| (0x80..=0x9F).contains(b));
        if byte < 0x20 || byte == 0x7F || c1 {
            let code = if c1 {
                u32::from(bytes[at + 1])
            } else {
                u32::from(byte)
            };
            return Err(ParseError::new(
                CONTROL_CHARACTER,
                Location::new(line, column),
                format!(
                    "control character U+{code:04X}; only line feeds and spaces separate tokens"
                ),
            ));
        }
    }
    Ok(())
}

/// What a token is.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Tok {
    /// `[A-Za-z_][A-Za-z0-9_]*`.
    Word,
    /// A number-shaped run, validated by the parser.
    Number,
    /// A string, decoded.
    Str(String),
    /// A comment: its text after `#`, trailing spaces removed; `eol` when a token precedes it
    /// on its line.
    Comment {
        /// The text after `#`.
        text: String,
        /// Whether it ends a line that holds a token.
        eol: bool,
    },
    /// A line feed.
    Newline,
    /// One of `: = ( ) , [ ] . @ -`.
    Punct(u8),
    /// `->`.
    Arrow,
    /// The end of the document.
    Eof,
}

/// A token, its byte range and its location.
#[derive(Clone, Debug)]
pub(super) struct Token {
    pub(super) tok: Tok,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) at: Location,
}

impl Token {
    /// The token's source text.
    pub(super) fn text<'a>(&self, src: &'a str) -> &'a str {
        src.get(self.start..self.end).unwrap_or("")
    }

    /// A short description for `expected ..., found ...` messages.
    pub(super) fn describe(&self, src: &str) -> String {
        match &self.tok {
            Tok::Newline => "the end of the line".to_owned(),
            Tok::Eof => "the end of the document".to_owned(),
            Tok::Comment { .. } => "a comment".to_owned(),
            Tok::Str(_) => "a string".to_owned(),
            _ => format!("`{}`", self.text(src)),
        }
    }
}

/// The pull lexer.
pub(super) struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: usize,
    line_start: usize,
    line_has_token: bool,
    line_has_comment: bool,
}

fn is_word_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

impl<'a> Lexer<'a> {
    /// A lexer over a pre-scanned document.
    pub(super) fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            line: 1,
            line_start: 0,
            line_has_token: false,
            line_has_comment: false,
        }
    }

    /// Whether the current line has held neither a token nor a comment so far (a line feed now
    /// ends a blank line).
    pub(super) fn line_is_empty(&self) -> bool {
        !self.line_has_token && !self.line_has_comment
    }

    /// The source text.
    pub(super) fn src(&self) -> &'a str {
        self.src
    }

    /// The location of byte `pos` on the current line.
    fn loc(&self, pos: usize) -> Location {
        Location::new(self.line, pos.saturating_sub(self.line_start) + 1)
    }

    fn skip_spaces(&self) -> usize {
        let mut at = self.pos;
        while self.bytes.get(at) == Some(&b' ') {
            at += 1;
        }
        at
    }

    /// The next token, not consumed.
    pub(super) fn peek(&self) -> Result<Token, ParseError> {
        let start = self.skip_spaces();
        let at = self.loc(start);
        let token = |tok, end| Token {
            tok,
            start,
            end,
            at,
        };
        let Some(&first) = self.bytes.get(start) else {
            return Ok(token(Tok::Eof, start));
        };
        let next = self.bytes.get(start + 1).copied();
        match first {
            b'\n' => Ok(token(Tok::Newline, start + 1)),
            b'#' => {
                let end = self.bytes[start..]
                    .iter()
                    .position(|&b| b == b'\n')
                    .map_or(self.bytes.len(), |n| start + n);
                let text = self.src.get(start + 1..end).unwrap_or("");
                Ok(token(
                    Tok::Comment {
                        text: text.trim_end_matches(' ').to_owned(),
                        eol: self.line_has_token,
                    },
                    end,
                ))
            }
            b'"' => self.string(start, at),
            b'-' if next == Some(b'>') => Ok(token(Tok::Arrow, start + 2)),
            b'0'..=b'9' => self.number(start, at),
            b'-' | b'+' | b'.' if next.is_some_and(|b| b.is_ascii_digit()) => {
                self.number(start, at)
            }
            b':' | b'=' | b'(' | b')' | b',' | b'[' | b']' | b'.' | b'@' | b'-' => {
                Ok(token(Tok::Punct(first), start + 1))
            }
            b if is_word_start(b) => {
                let len = self.bytes[start..]
                    .iter()
                    .take_while(|&&b| is_word_byte(b))
                    .count();
                bounded(len, at, "name")?;
                Ok(token(Tok::Word, start + len))
            }
            _ => {
                let ch = self.src.get(start..).and_then(|s| s.chars().next());
                Err(ParseError::new(
                    STRAY_TOKEN,
                    at,
                    format!(
                        "unexpected character `{}`",
                        ch.map_or_else(String::new, |c| c.to_string())
                    ),
                ))
            }
        }
    }

    fn number(&self, start: usize, at: Location) -> Result<Token, ParseError> {
        let mut end = start + 1;
        while let Some(&b) = self.bytes.get(end) {
            let exponent_sign =
                (b == b'+' || b == b'-') && matches!(self.bytes[end - 1], b'e' | b'E');
            if is_word_byte(b) || b == b'.' || exponent_sign {
                end += 1;
            } else {
                break;
            }
        }
        bounded(end - start, at, "number")?;
        Ok(Token {
            tok: Tok::Number,
            start,
            end,
            at,
        })
    }

    fn string(&self, start: usize, at: Location) -> Result<Token, ParseError> {
        let mut value = String::new();
        let mut run = start + 1;
        let mut i = start + 1;
        loop {
            if i - (start + 1) > MAX_STRING_BYTES {
                return Err(ParseError::new(
                    STRING_TOO_LONG,
                    at,
                    format!("the string is longer than {MAX_STRING_BYTES} bytes as written"),
                ));
            }
            match self.bytes.get(i) {
                None | Some(b'\n') => {
                    return Err(ParseError::new(
                        UNTERMINATED_STRING,
                        at,
                        "the string has no closing `\"` on its line",
                    ));
                }
                Some(b'"') => {
                    value.push_str(self.src.get(run..i).unwrap_or(""));
                    return Ok(Token {
                        tok: Tok::Str(value),
                        start,
                        end: i + 1,
                        at,
                    });
                }
                Some(b'\\') => {
                    value.push_str(self.src.get(run..i).unwrap_or(""));
                    let decoded = match self.bytes.get(i + 1) {
                        Some(b'"') => '"',
                        Some(b'\\') => '\\',
                        Some(b'n') => '\n',
                        None | Some(b'\n') => {
                            return Err(ParseError::new(
                                UNTERMINATED_STRING,
                                at,
                                "the string has no closing `\"` on its line",
                            ));
                        }
                        Some(_) => {
                            let ch = self.src.get(i + 1..).and_then(|s| s.chars().next());
                            return Err(ParseError::new(
                                BAD_ESCAPE,
                                self.loc(i),
                                format!(
                                    "`\\{}` is not an escape; only `\\\"`, `\\\\` and `\\n` are",
                                    ch.map_or_else(String::new, |c| c.to_string())
                                ),
                            ));
                        }
                    };
                    value.push(decoded);
                    i += 2;
                    run = i;
                }
                Some(_) => i += 1,
            }
        }
    }

    /// Consume `token` (the last [`Lexer::peek`]).
    pub(super) fn bump(&mut self, token: &Token) {
        self.pos = token.end;
        match token.tok {
            Tok::Newline => {
                self.line += 1;
                self.line_start = token.end;
                self.line_has_token = false;
                self.line_has_comment = false;
            }
            Tok::Comment { .. } => self.line_has_comment = true,
            Tok::Eof => {}
            _ => self.line_has_token = true,
        }
    }

    /// Peek and consume one token.
    pub(super) fn next(&mut self) -> Result<Token, ParseError> {
        let token = self.peek()?;
        self.bump(&token);
        Ok(token)
    }

    /// Whether the next non-space byte ends a value: the end of the document or the line, a
    /// comment, `,`, `)` or `]`.
    pub(super) fn at_value_end(&self) -> bool {
        matches!(
            self.bytes.get(self.skip_spaces()),
            None | Some(b'\n' | b'#' | b',' | b')' | b']')
        )
    }

    /// Read a context-dependent run (an atom, a unit suffix, a provenance id): skip spaces, then
    /// take bytes up to a space, a line feed, the end or one of `stops`. `None` when the run is
    /// empty.
    pub(super) fn raw_run(
        &mut self,
        stops: &[u8],
    ) -> Result<Option<(String, Location)>, ParseError> {
        let start = self.skip_spaces();
        let at = self.loc(start);
        let len = self.bytes[start..]
            .iter()
            .take_while(|&&b| b != b' ' && b != b'\n' && !stops.contains(&b))
            .count();
        if len == 0 {
            return Ok(None);
        }
        bounded(len, at, "token")?;
        self.pos = start + len;
        self.line_has_token = true;
        Ok(Some((
            self.src.get(start..start + len).unwrap_or("").to_owned(),
            at,
        )))
    }
}

fn bounded(len: usize, at: Location, what: &str) -> Result<(), ParseError> {
    if len > MAX_TOKEN_BYTES {
        return Err(ParseError::new(
            TOKEN_TOO_LONG,
            at,
            format!("the {what} is {len} bytes long; at most {MAX_TOKEN_BYTES} are allowed"),
        ));
    }
    Ok(())
}
