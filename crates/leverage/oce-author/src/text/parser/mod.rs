//! The recursive-descent parser (decision 1): the file production, the header, statement
//! dispatch, and the layout rules — blank lines and comments — every statement shares.
//!
//! Layout. A blank line before a statement (or before its comments) sets its
//! `blank_line_before`; comment lines on their own lines are that statement's `comments`; a comment
//! after a token on the same line (an end-of-line comment) is appended to the comments of the
//! statement on that line, so `fmt` moves it to its own line above. Comments before `sequence`
//! and between `sequence` and `extends` are the header's ([`crate::Sequence::annotations`]); the
//! comments after the last statement are [`crate::Sequence::trailing_comments`].

mod connections;
mod statements;
mod values;

pub(in crate::text) use statements::TAGS;
pub(in crate::text) use values::template_base;

use crate::identifier::{Identifier, ProvenanceId};
use crate::location::Location;
use crate::model::{Annotations, Sequence};

use super::ParseError;
use super::codes::{
    MALFORMED_ATOM, MAX_STATEMENTS_PER_KIND, MISPLACED_STATEMENT, MISSING_EXTENDS,
    MISSING_SEQUENCE, STRAY_TOKEN, TOO_MANY_STATEMENTS,
};
use super::lexer::{Lexer, Tok, Token};
use super::spans::SourceMap;

/// The parser state: the lexer, the model being built, the source map and the annotations
/// waiting for the next statement.
pub(crate) struct Parser<'a> {
    lx: Lexer<'a>,
    sequence: Sequence,
    spans: SourceMap,
    pending: Annotations,
}

impl<'a> Parser<'a> {
    /// A parser over a pre-scanned document.
    pub(crate) fn new(text: &'a str) -> Self {
        Self {
            lx: Lexer::new(text),
            sequence: Sequence::new(Identifier::unchecked("")),
            spans: SourceMap::new(Location::new(1, 1)),
            pending: Annotations::default(),
        }
    }

    /// Parse the whole document.
    pub(crate) fn run(mut self) -> Result<(Sequence, SourceMap), ParseError> {
        self.header()?;
        loop {
            let token = self.lx.peek()?;
            match &token.tok {
                Tok::Eof => break,
                Tok::Newline => {
                    if self.lx.line_is_empty() {
                        self.pending.blank_line_before = true;
                    }
                    self.lx.bump(&token);
                }
                Tok::Comment { text, .. } => {
                    self.pending.comments.push(text.clone());
                    self.lx.bump(&token);
                }
                Tok::Word => self.statement(token)?,
                _ => {
                    return Err(ParseError::new(
                        STRAY_TOKEN,
                        token.at,
                        format!(
                            "expected a statement, found {}",
                            token.describe(self.lx.src())
                        ),
                    ));
                }
            }
        }
        self.sequence.trailing_comments = std::mem::take(&mut self.pending.comments);
        Ok((self.sequence, self.spans))
    }

    /// `sequence name` and `extends ( none | id )`, with the comments around them.
    fn header(&mut self) -> Result<(), ParseError> {
        let keyword = self.skip_header_layout()?;
        if !self.is_word(&keyword, "sequence") {
            return Err(ParseError::new(
                MISSING_SEQUENCE,
                keyword.at,
                format!(
                    "a document starts with `sequence <name>`, found {}",
                    keyword.describe(self.lx.src())
                ),
            ));
        }
        self.lx.bump(&keyword);
        let (name, at) = self.name("the sequence name")?;
        self.sequence.name = name;
        self.spans.sequence = at;
        self.end_header_statement()?;

        let keyword = self.skip_header_layout()?;
        if !self.is_word(&keyword, "extends") {
            return Err(ParseError::new(
                MISSING_EXTENDS,
                keyword.at,
                format!(
                    "`sequence` is followed by `extends none` or `extends <provenance id>` (decision 5), found {}",
                    keyword.describe(self.lx.src())
                ),
            ));
        }
        self.lx.bump(&keyword);
        let Some((id, at)) = self.lx.raw_run(&[])? else {
            let found = self.lx.peek()?;
            return Err(ParseError::new(
                STRAY_TOKEN,
                found.at,
                "expected `none` or a provenance id after `extends`",
            ));
        };
        if let Some(bad) = id.bytes().position(|b| !(b'!'..=b'~').contains(&b)) {
            return Err(ParseError::new(
                MALFORMED_ATOM,
                at,
                format!(
                    "the provenance id `{id}` has a byte other than printable ASCII at byte {bad}"
                ),
            ));
        }
        self.spans.extends = at;
        self.sequence.extends = (id != "none").then(|| ProvenanceId::unchecked(id));
        self.end_header_statement()
    }

    /// Skip line feeds and collect comment lines into the header; return the next token.
    fn skip_header_layout(&mut self) -> Result<Token, ParseError> {
        loop {
            let token = self.lx.peek()?;
            match &token.tok {
                Tok::Newline => self.lx.bump(&token),
                Tok::Comment { text, .. } => {
                    self.sequence.annotations.comments.push(text.clone());
                    self.lx.bump(&token);
                }
                _ => return Ok(token),
            }
        }
    }

    fn end_header_statement(&mut self) -> Result<(), ParseError> {
        if let Some(comment) = self.end_statement()? {
            self.sequence.annotations.comments.push(comment);
        }
        Ok(())
    }

    /// Dispatch one statement starting with a word.
    fn statement(&mut self, keyword: Token) -> Result<(), ParseError> {
        let word = keyword.text(self.lx.src());
        match word {
            "sequence" | "extends" => Err(ParseError::new(
                MISPLACED_STATEMENT,
                keyword.at,
                format!("`{word}` is written once, at the top of the document"),
            )),
            "param" => {
                self.lx.bump(&keyword);
                self.param(keyword.at)
            }
            "in" | "out" => {
                self.lx.bump(&keyword);
                self.boundary(keyword.at, word == "in")
            }
            "block" => {
                self.lx.bump(&keyword);
                self.block(keyword.at)
            }
            _ => self.connection(keyword),
        }
    }

    /// The annotations collected for the statement that starts now.
    fn take_pending(&mut self) -> Annotations {
        std::mem::take(&mut self.pending)
    }

    /// After a statement: an optional end-of-line comment (returned), then a line feed or the
    /// end of the document.
    fn end_statement(&mut self) -> Result<Option<String>, ParseError> {
        let token = self.lx.peek()?;
        match &token.tok {
            Tok::Comment { text, .. } => {
                let text = text.clone();
                self.lx.bump(&token);
                // A comment runs to the end of its line, so this is the line feed or the end.
                self.lx.next()?;
                Ok(Some(text))
            }
            Tok::Newline => {
                self.lx.bump(&token);
                Ok(None)
            }
            Tok::Eof => Ok(None),
            _ => Err(ParseError::new(
                STRAY_TOKEN,
                token.at,
                format!(
                    "expected the end of the statement, found {}",
                    token.describe(self.lx.src())
                ),
            )),
        }
    }

    fn is_word(&self, token: &Token, word: &str) -> bool {
        token.tok == Tok::Word && token.text(self.lx.src()) == word
    }

    /// A name: any word, kept unchecked so the validator judges it (`invalid-name`).
    fn name(&mut self, what: &str) -> Result<(Identifier, Location), ParseError> {
        let token = self.lx.peek()?;
        if token.tok != Tok::Word {
            return Err(ParseError::new(
                STRAY_TOKEN,
                token.at,
                format!("expected {what}, found {}", token.describe(self.lx.src())),
            ));
        }
        self.lx.bump(&token);
        Ok((Identifier::unchecked(token.text(self.lx.src())), token.at))
    }

    /// Consume the punctuation `want` or refuse with `expected ...`.
    fn punct(&mut self, want: u8, what: &str) -> Result<Token, ParseError> {
        let token = self.lx.peek()?;
        if token.tok != Tok::Punct(want) {
            return Err(ParseError::new(
                STRAY_TOKEN,
                token.at,
                format!(
                    "expected `{}` {what}, found {}",
                    char::from(want),
                    token.describe(self.lx.src())
                ),
            ));
        }
        self.lx.bump(&token);
        Ok(token)
    }
}

/// Refuse one more statement of a kind already at [`MAX_STATEMENTS_PER_KIND`].
fn room(len: usize, at: Location, what: &str) -> Result<(), ParseError> {
    if len >= MAX_STATEMENTS_PER_KIND {
        return Err(ParseError::new(
            TOO_MANY_STATEMENTS,
            at,
            format!("more than {MAX_STATEMENTS_PER_KIND} {what}"),
        ));
    }
    Ok(())
}
