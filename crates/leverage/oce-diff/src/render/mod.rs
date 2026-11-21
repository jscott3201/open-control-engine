//! The two renderers, pure functions of a [`crate::Diff`] (diff note, "Output"; decision 7).
//!
//! [`render_text`] writes one line per entry under `#` section headers, in the authoring text
//! form's own syntax. [`render_markdown`] writes a `Change | Subject | Was | Now` table. Both use
//! fixed templates and nothing else: no prose, no model, no clock. What they share is here: how a
//! value is spelled and how a block's class and assignments are spelled.
//!
//! The data may hold any string a model can hold, a name with a line feed included, because the
//! diff never validates. Every string is written so that it cannot end a line or a table cell:
//! [`one_line`] spells a control character as `\u{…}`.

mod markdown;
mod text;

pub use markdown::render_markdown;
pub use text::render_text;

use crate::data::{Assignment, Literal};

/// `text` with every control character spelled `\u{…}` (a line feed is `\u{a}`), so it stays on
/// one line.
pub(crate) fn one_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() {
            out.push_str(&format!("\\u{{{:x}}}", u32::from(character)));
        } else {
            out.push(character);
        }
    }
    out
}

/// A value as the authoring text form spells it: a Real by its shortest round-trip spelling, an
/// Integer and a Boolean bare, a string quoted with `"`, `\` and line feed escaped, a reference
/// by its name, a member as `Type.Member`. A NaN has no spelling of its own there and two NaNs
/// spell alike, so a NaN is written with its bits: `NaN(0x7ff8000000000000)`.
pub(crate) fn literal_text(value: &Literal) -> String {
    match value {
        Literal::Real { real, bits } if real == "NaN" => format!("NaN({bits})"),
        Literal::Real { real, .. } => real.clone(),
        Literal::Integer { integer } => integer.to_string(),
        Literal::Boolean { boolean } => boolean.to_string(),
        Literal::Text { text } => {
            let mut out = String::with_capacity(text.len() + 2);
            out.push('"');
            for character in text.chars() {
                match character {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    other => out.push(other),
                }
            }
            out.push('"');
            out
        }
        Literal::Ref { name } => name.clone(),
        Literal::Member { member } => member.clone(),
    }
}

/// Whether a value is a number or a Boolean, which the Markdown table writes bare.
pub(crate) fn is_bare(value: &Literal) -> bool {
    matches!(
        value,
        Literal::Real { .. } | Literal::Integer { .. } | Literal::Boolean { .. }
    )
}

/// A block's right-hand side as the text form writes it on one line: `Class(a = 1, b = true)`,
/// or `Class` with no assignments.
pub(crate) fn block_text(class: &str, params: &[Assignment]) -> String {
    if params.is_empty() {
        return class.to_owned();
    }
    let assignments: Vec<String> = params
        .iter()
        .map(|assignment| format!("{} = {}", assignment.name, literal_text(&assignment.value)))
        .collect();
    format!("{class}({})", assignments.join(", "))
}

/// A boundary's declaration after its name: `Real @unit K @min 0`.
pub(crate) fn connector_text(value_type: &str, tags: [(&str, Option<String>); 5]) -> String {
    let mut out = value_type.to_owned();
    for (tag, value) in tags {
        if let Some(value) = value {
            out.push_str(&format!(" @{tag} {value}"));
        }
    }
    out
}
