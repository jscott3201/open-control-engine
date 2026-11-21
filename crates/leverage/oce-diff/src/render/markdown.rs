//! The Markdown rendering: one `Change | Subject | Was | Now` table for a bundle card.
//!
//! ```markdown
//! | Change | Subject | Was | Now |
//! | --- | --- | --- | --- |
//! | reparameterized | `damOpen.t` | 0.95 | 0.85 |
//! | removed | `openLong` | `CDL.Logical.TrueDelay(delayTime = 60, delayOnInit = true)` | |
//! | renamed (exact) | `setFrac` | `seventySet` | `setFrac` |
//! ```
//!
//! One row per entry, the sections in the data's order (parameters, interface, graph, metadata).
//! A graph or metadata entry's change is its kind; a parameter entry's is `param` and its kind,
//! an interface entry's its direction and its kind. A number or a Boolean is written bare; a
//! name, a class, a statement and any other value is a code span; comment text is escaped prose.
//! A cell for a side that has nothing is empty.
//!
//! No string in the data can break the table. A value that could not sit in a code span (it
//! holds a backtick, a pipe or a control character) is written as escaped prose instead:
//! Markdown's own characters behind a backslash, `&`, `<` and `>` as entities, a line feed as
//! `<br>` and any other control character as `\u{…}`.

use crate::changes::{GraphChange, InterfaceChange, MetadataChange, ParameterChange};
use crate::data::{Confidence, Diff, Direction, GraphList, Literal, Side};

use super::{block_text, connector_text, is_bare, literal_text};

/// Render `diff` as a Markdown table. A pure function of the data: the same diff always gives
/// the same bytes, every row is one line of four cells, and an empty diff is the two header
/// lines. Never panics.
#[must_use]
pub fn render_markdown(diff: &Diff) -> String {
    let mut out = String::from("| Change | Subject | Was | Now |\n| --- | --- | --- | --- |\n");
    for entry in &diff.parameters {
        row(&mut out, parameter(entry));
    }
    for entry in &diff.interface {
        row(&mut out, interface(entry));
    }
    for entry in &diff.graph {
        row(&mut out, graph(entry));
    }
    for entry in &diff.metadata {
        row(&mut out, metadata(entry));
    }
    out
}

/// One row: its change, its subject (always a code span), and the was and now cells as written.
struct Row {
    change: String,
    subject: String,
    was: String,
    now: String,
}

fn row(out: &mut String, row: Row) {
    for cell in [row.change, code(&row.subject), row.was, row.now] {
        out.push_str("| ");
        if !cell.is_empty() {
            out.push_str(&cell);
            out.push(' ');
        }
    }
    out.push_str("|\n");
}

/// `text` as escaped prose that cannot end a cell or start markup.
fn prose(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\n' => out.push_str("<br>"),
            '\\' | '|' | '`' | '*' | '_' | '[' | ']' | '#' | '~' => {
                out.push('\\');
                out.push(character);
            }
            other if other.is_control() => {
                out.push_str(&format!("\\\\u{{{:x}}}", u32::from(other)));
            }
            other => out.push(other),
        }
    }
    out
}

/// `text` as a code span, or as escaped prose when a code span could not hold it. Empty text is
/// an empty cell.
fn code(text: &str) -> String {
    let plain = !text.starts_with(' ')
        && !text.ends_with(' ')
        && !text
            .chars()
            .any(|character| character == '`' || character == '|' || character.is_control());
    if text.is_empty() {
        String::new()
    } else if plain {
        format!("`{text}`")
    } else {
        prose(text)
    }
}

/// A value: a number or a Boolean bare, anything else as a code span.
fn value(literal: &Literal) -> String {
    let text = literal_text(literal);
    if is_bare(literal) { text } else { code(&text) }
}

fn optional(literal: Option<&Literal>) -> String {
    literal.map(value).unwrap_or_default()
}

fn optional_code(text: Option<&String>) -> String {
    text.map(|text| code(text)).unwrap_or_default()
}

fn parameter(entry: &ParameterChange) -> Row {
    let typed = |value_type: &str, literal: &Literal| {
        code(&format!("{value_type} = {}", literal_text(literal)))
    };
    let (kind, was, now) = match entry {
        ParameterChange::Added {
            value_type, value, ..
        } => ("added", String::new(), typed(value_type, value)),
        ParameterChange::Removed {
            value_type, value, ..
        } => ("removed", typed(value_type, value), String::new()),
        ParameterChange::Changed { old, new, .. } => ("changed", value(old), value(new)),
        ParameterChange::TypeChanged {
            old_type,
            new_type,
            old,
            new,
            ..
        } => ("type-changed", typed(old_type, old), typed(new_type, new)),
        ParameterChange::Reordered { .. } => ("reordered", String::new(), String::new()),
    };
    Row {
        change: format!("param {kind}"),
        subject: entry.subject().to_owned(),
        was,
        now,
    }
}

fn interface(entry: &InterfaceChange) -> Row {
    let (direction, kind, was, now): (Option<Direction>, &str, String, String) = match entry {
        InterfaceChange::Added {
            direction,
            value_type,
            unit,
            quantity,
            display_unit,
            min,
            max,
            ..
        }
        | InterfaceChange::Removed {
            direction,
            value_type,
            unit,
            quantity,
            display_unit,
            min,
            max,
            ..
        } => {
            let declaration = code(&connector_text(
                value_type,
                [
                    ("unit", unit.clone()),
                    ("quantity", quantity.clone()),
                    ("displayUnit", display_unit.clone()),
                    ("min", min.as_ref().map(literal_text)),
                    ("max", max.as_ref().map(literal_text)),
                ],
            ));
            if matches!(entry, InterfaceChange::Added { .. }) {
                (Some(*direction), "added", String::new(), declaration)
            } else {
                (Some(*direction), "removed", declaration, String::new())
            }
        }
        InterfaceChange::TypeChanged {
            direction,
            old,
            new,
            ..
        } => (Some(*direction), "type-changed", code(old), code(new)),
        InterfaceChange::UnitChanged {
            direction,
            old,
            new,
            ..
        } => (
            Some(*direction),
            "unit-changed",
            optional_code(old.as_ref()),
            optional_code(new.as_ref()),
        ),
        InterfaceChange::QuantityChanged {
            direction,
            old,
            new,
            ..
        } => (
            Some(*direction),
            "quantity-changed",
            optional_code(old.as_ref()),
            optional_code(new.as_ref()),
        ),
        InterfaceChange::DisplayUnitChanged {
            direction,
            old,
            new,
            ..
        } => (
            Some(*direction),
            "display-unit-changed",
            optional_code(old.as_ref()),
            optional_code(new.as_ref()),
        ),
        InterfaceChange::MinChanged {
            direction,
            old,
            new,
            ..
        } => (
            Some(*direction),
            "min-changed",
            optional(old.as_ref()),
            optional(new.as_ref()),
        ),
        InterfaceChange::MaxChanged {
            direction,
            old,
            new,
            ..
        } => (
            Some(*direction),
            "max-changed",
            optional(old.as_ref()),
            optional(new.as_ref()),
        ),
        InterfaceChange::Reordered { .. } => (None, "reordered", String::new(), String::new()),
    };
    Row {
        change: match direction {
            Some(direction) => format!("{} {kind}", direction.keyword()),
            None => kind.to_owned(),
        },
        subject: entry.subject().to_owned(),
        was,
        now,
    }
}

fn graph(entry: &GraphChange) -> Row {
    let (change, was, now) = match entry {
        GraphChange::Added { class, params, .. } => (
            "added".to_owned(),
            String::new(),
            code(&block_text(class, params)),
        ),
        GraphChange::Removed { class, params, .. } => (
            "removed".to_owned(),
            code(&block_text(class, params)),
            String::new(),
        ),
        GraphChange::Renamed {
            subject,
            from,
            confidence,
            ..
        } => (
            format!(
                "renamed ({})",
                match confidence {
                    Confidence::Exact => "exact",
                    Confidence::High => "high",
                }
            ),
            code(from),
            code(subject),
        ),
        GraphChange::Reparameterized { old, new, .. } => (
            "reparameterized".to_owned(),
            optional(old.as_ref()),
            optional(new.as_ref()),
        ),
        GraphChange::Rewired { old, new, .. } => (
            "rewired".to_owned(),
            optional_code(old.as_ref()),
            optional_code(new.as_ref()),
        ),
        GraphChange::Reordered { list, .. } => (
            format!(
                "reordered ({})",
                match list {
                    GraphList::Blocks => "blocks",
                    GraphList::Connections => "connections",
                    GraphList::Assignments => "assignments",
                }
            ),
            String::new(),
            String::new(),
        ),
        GraphChange::ParameterDefaulted {
            value: literal,
            side,
            ..
        } => match side {
            Side::Base => (
                "parameter-defaulted".to_owned(),
                value(literal),
                String::new(),
            ),
            Side::Head => (
                "parameter-defaulted".to_owned(),
                String::new(),
                value(literal),
            ),
        },
    };
    Row {
        change,
        subject: entry.subject().to_owned(),
        was,
        now,
    }
}

fn metadata(entry: &MetadataChange) -> Row {
    let (change, was, now) = match entry {
        MetadataChange::SequenceRenamed { old, new, .. } => (
            "sequence-renamed",
            optional_code(old.as_ref()),
            optional_code(new.as_ref()),
        ),
        MetadataChange::DerivedFromChanged { old, new, .. } => (
            "derived-from-changed",
            optional_code(old.as_ref()),
            optional_code(new.as_ref()),
        ),
        MetadataChange::SemanticChanged { old, new, .. } => (
            "semantic-changed",
            optional_code(old.as_ref()),
            optional_code(new.as_ref()),
        ),
        MetadataChange::CommentChanged { old, new, .. } => (
            "comment-changed",
            old.as_deref().map(prose).unwrap_or_default(),
            new.as_deref().map(prose).unwrap_or_default(),
        ),
        MetadataChange::LiteralRespelled { old, new, .. } => (
            "literal-respelled",
            optional_code(old.as_ref()),
            optional_code(new.as_ref()),
        ),
    };
    Row {
        change: change.to_owned(),
        subject: entry.subject().to_owned(),
        was,
        now,
    }
}
