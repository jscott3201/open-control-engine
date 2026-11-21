//! Comment removal for a commissioning spec and a point mapping (commissioning note decision 3:
//! "the loader strips `//` and `/* */` comments outside strings, then parses strictly").
//!
//! Two comment forms, and nothing else, are tolerated beyond strict JSON:
//!
//! - `//` to the end of its line: up to, and not including, the next line feed or carriage
//!   return, or to the end of the text;
//! - `/*` to the next `*/`. Block comments do not nest, the two characters of `/*` are never
//!   read as the start of `*/` (`/*/` is an open comment), and one that is never closed is
//!   refused.
//!
//! A comment starts only outside a string. Inside a string a `"` ends it and a `\` takes the
//! byte after it with it, so `"http://x"`, `"/* kept */"` and `"\"//"` are untouched, and a
//! string that never ends swallows whatever follows: the strict reader then refuses the
//! unterminated string. This is the one place that decides what a string is before the strict
//! cursor does, and it agrees with the cursor on every text the cursor accepts.
//!
//! Every byte of a comment becomes a space, except a line feed, which is kept. The result has
//! the length of the input, with every other byte where it was, so a line and a byte column in
//! the stripped text are the same line and column in the text as written.
//!
//! The pass reads each byte once and looks at most one byte ahead: its work is the length of the
//! text, whatever the text holds. A text with no comment is returned as it is, with no
//! allocation; otherwise the one allocation is a buffer of the text's length.

use std::borrow::Cow;

use crate::error::Location;

/// Where the pass is in the text.
#[derive(Clone, Copy)]
enum State {
    /// Outside a string and a comment.
    Code,
    /// Inside a string.
    Text,
    /// After a backslash inside a string: the next byte is taken whatever it is.
    Escape,
    /// Inside a `//` comment.
    Line,
    /// Inside a `/* */` comment whose `/` is at this byte offset.
    Block { opened: usize },
}

/// `text` with every comment blanked (see the module documentation).
///
/// # Errors
/// The line and column of the `/*` of a block comment that is never closed.
pub(crate) fn strip(text: &str) -> Result<Cow<'_, str>, Location> {
    let bytes = text.as_bytes();
    let mut out: Option<String> = None;
    // `text[..copied]` is already in `out`, comments blanked.
    let mut copied = 0;
    let mut state = State::Code;
    for (at, byte) in bytes.iter().copied().enumerate() {
        match state {
            State::Code => match byte {
                b'"' => state = State::Text,
                b'/' => {
                    let next = match bytes.get(at + 1) {
                        Some(b'/') => State::Line,
                        Some(b'*') => State::Block { opened: at },
                        _ => continue,
                    };
                    // Both markers are ASCII, so `at` is a character boundary.
                    let blanked = out.get_or_insert_with(|| String::with_capacity(text.len()));
                    blanked.push_str(&text[copied..at]);
                    blanked.push(' ');
                    copied = at + 1;
                    state = next;
                }
                _ => {}
            },
            State::Text => match byte {
                b'\\' => state = State::Escape,
                b'"' => state = State::Code,
                _ => {}
            },
            State::Escape => state = State::Text,
            State::Line => {
                if byte == b'\n' || byte == b'\r' {
                    // The line end is not part of the comment: it is copied with what follows.
                    state = State::Code;
                } else {
                    blank(&mut out, byte);
                    copied = at + 1;
                }
            }
            State::Block { opened } => {
                blank(&mut out, byte);
                copied = at + 1;
                // The `*` of the closing `*/` comes after the two bytes of the opening `/*`.
                if byte == b'/' && at >= opened + 3 && bytes[at - 1] == b'*' {
                    state = State::Code;
                }
            }
        }
    }
    if let State::Block { opened } = state {
        return Err(location_of(bytes, opened));
    }
    Ok(match out {
        Some(mut blanked) => {
            blanked.push_str(&text[copied..]);
            Cow::Owned(blanked)
        }
        None => Cow::Borrowed(text),
    })
}

/// Write one byte of a comment: a line feed as itself, anything else as a space.
fn blank(out: &mut Option<String>, byte: u8) {
    if let Some(blanked) = out {
        blanked.push(if byte == b'\n' { '\n' } else { ' ' });
    }
}

/// The 1-based line and byte column of the byte at `offset`.
fn location_of(bytes: &[u8], offset: usize) -> Location {
    let before = &bytes[..offset];
    let line = 1 + before.iter().filter(|byte| **byte == b'\n').count();
    let line_start = before
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |feed| feed + 1);
    Location::new(line, offset - line_start + 1)
}

#[cfg(test)]
#[path = "comments_tests.rs"]
mod tests;
