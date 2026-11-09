//! Connections: `endpoint "->" endpoint`, an endpoint a boundary name or `block.port`.
//!
//! A line that starts with a word other than a statement keyword is a connection. When that word
//! is followed by another word, the line is a misspelled statement (`blok x = …`) and is refused as
//! [`UNKNOWN_KEYWORD`] rather than as a stray token. An endpoint's parts are kept unchecked, so a
//! malformed one (`a.b.c`) reaches the validator as `malformed-connection`.

use crate::model::{Connection, Endpoint};

use super::super::ParseError;
use super::super::codes::{STRAY_TOKEN, UNKNOWN_KEYWORD};
use super::super::lexer::{Tok, Token};
use super::{Parser, room};

impl Parser<'_> {
    /// One connection, starting at `first` (not yet consumed).
    pub(super) fn connection(&mut self, first: Token) -> Result<(), ParseError> {
        let annotations = self.take_pending();
        let (source, at) = self.dotted("a connection source")?;
        let next = self.lx.peek()?;
        if next.tok == Tok::Word && !source.contains('.') {
            return Err(ParseError::new(
                UNKNOWN_KEYWORD,
                first.at,
                format!(
                    "`{source}` is not a statement keyword; the statements are `param`, `in`, `out`, `block` and connections `a.y -> b.u`"
                ),
            ));
        }
        if next.tok != Tok::Arrow {
            return Err(ParseError::new(
                STRAY_TOKEN,
                next.at,
                format!(
                    "expected `->` after the connection source, found {}",
                    next.describe(self.lx.src())
                ),
            ));
        }
        room(self.sequence.connections.len(), at, "connections")?;
        self.lx.bump(&next);
        let (target, _) = self.dotted("a connection target")?;
        let mut connection = Connection::new(Endpoint::parse(&source), Endpoint::parse(&target));
        connection.annotations = annotations;
        if let Some(comment) = self.end_statement()? {
            connection.annotations.comments.push(comment);
        }
        self.sequence.connections.push(connection);
        self.spans.connections.push(at);
        Ok(())
    }
}
