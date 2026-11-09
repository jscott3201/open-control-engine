//! Comment lines from v0 free text (importer note, "Comments from v0 text").
//!
//! A v0 `description` may be up to 4,096 bytes and contain line feeds, and `unit` is free text, so
//! neither is pasted into a comment as is. The text is split at line feeds; every other control
//! character the text form refuses (C0 including CR and tab, DEL, C1, and the bidirectional
//! overrides U+202A–U+202E and U+2066–U+2069) becomes one space; each line is truncated at a
//! character boundary so that `# ` plus the line stays within [`MAX_V0_COMMENT_LINE_BYTES`] (each
//! truncation is reported); trailing spaces are removed, as the text form's parser removes them.
//! No escaping is needed beyond that: a comment runs to the end of its line and the parser accepts
//! any other UTF-8 there.

use super::report::{DroppedProperty, Truncated};

/// The longest comment line the v0 importer writes, in bytes, counting the `# ` it is printed
/// with: the importer note's 4,096, which keeps every imported line within the text form's line
/// bound ([`crate::MAX_LINE_BYTES`]) at any indent the printer gives a comment.
pub const MAX_V0_COMMENT_LINE_BYTES: usize = 4_096;

/// Whether the text form refuses `ch` on a line (other than the line feed the text is split at).
fn refused(ch: char) -> bool {
    ch.is_control() || matches!(ch, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// The comment lines for `text`, appended to `comments` (each stored as the text form's parser
/// stores it: the text after `#`, a leading space included, trailing spaces removed). A truncated
/// line is reported against `subject` and `property`.
pub(super) fn append(
    comments: &mut Vec<String>,
    text: &str,
    subject: &str,
    property: DroppedProperty,
    truncated: &mut Vec<Truncated>,
) {
    // The line is printed as `# ` + line.
    let room = MAX_V0_COMMENT_LINE_BYTES - 2;
    for raw in text.split('\n') {
        let mut line: String = raw
            .chars()
            .map(|ch| if refused(ch) { ' ' } else { ch })
            .collect();
        let bytes = line.len();
        if bytes > room {
            let mut end = room;
            while !line.is_char_boundary(end) {
                end -= 1;
            }
            line.truncate(end);
            truncated.push(Truncated {
                subject: subject.to_owned(),
                property,
                line: comments.len(),
                bytes,
                kept: end,
            });
        }
        let stored = format!(" {line}");
        comments.push(stored.trim_end_matches(' ').to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> (Vec<String>, Vec<Truncated>) {
        let mut comments = Vec::new();
        let mut truncated = Vec::new();
        append(
            &mut comments,
            text,
            "x",
            DroppedProperty::Description,
            &mut truncated,
        );
        (comments, truncated)
    }

    #[test]
    fn line_feeds_split_and_every_other_control_becomes_a_space() {
        let (comments, truncated) = lines("a\tb\rc\0d\u{7f}e\u{85}f\u{202e}g\u{2066}h\nsecond  \n");
        assert_eq!(comments, [" a b c d e f g h", " second", ""]);
        assert!(truncated.is_empty());
    }

    #[test]
    fn a_line_is_truncated_at_a_character_boundary_and_reported() {
        let room = MAX_V0_COMMENT_LINE_BYTES - 2;
        // Exactly at the limit: kept whole.
        let (comments, truncated) = lines(&"a".repeat(room));
        assert_eq!(comments[0].len(), room + 1);
        assert!(truncated.is_empty());
        // One byte over, with a two-byte character straddling the limit.
        let text = format!("{}\u{e9}", "a".repeat(room - 1));
        let (comments, truncated) = lines(&text);
        assert_eq!(comments[0], format!(" {}", "a".repeat(room - 1)));
        assert_eq!(
            truncated,
            [Truncated {
                subject: "x".to_owned(),
                property: DroppedProperty::Description,
                line: 0,
                bytes: room + 1,
                kept: room - 1,
            }]
        );
    }
}
