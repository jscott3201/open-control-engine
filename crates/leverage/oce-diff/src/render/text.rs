//! The text rendering: one line per entry, grouped by section under `#` headers, in the authoring
//! text form's own syntax, with `+` added, `-` removed, `>` renamed, `~` changed and a trailing
//! `# was …` for the old value.
//!
//! ```text
//! # oce diff <base sequence> -> <head sequence>
//! # graph differs; parameters equal; interface equal; metadata differs
//! # blocks: 4 by name, 1 renamed, 1 removed, 1 added
//! # graph
//! ~ damOpen.t = 0.85   # was 0.95
//! - block openLong = CDL.Logical.TrueDelay(delayTime = 60, delayOnInit = true)
//! - damOpen.y -> openLong.u
//! ```
//!
//! The header is two lines, then one line for the block counts when any block was renamed,
//! removed or added, and one line per note. A section with no entries is not written. An added or
//! removed block is followed by one line per driven input, the only entries that take more than
//! one line.

use crate::changes::{GraphChange, InterfaceChange, MetadataChange, ParameterChange};
use crate::data::{
    Confidence, Diff, Direction, GraphList, Literal, MAX_RENAME_CANDIDATES, MAX_RENAME_PAIRS, Note,
    Side,
};

use super::{block_text, connector_text, literal_text, one_line};

/// Render `diff` as text. A pure function of the data: the same diff always gives the same
/// bytes, every line ends with one line feed, and no string in the data can add a line. Never
/// panics.
#[must_use]
pub fn render_text(diff: &Diff) -> String {
    let mut out = Lines::default();
    out.push(format!(
        "# oce diff {} -> {}",
        diff.base.sequence, diff.head.sequence
    ));
    let summary = &diff.summary;
    out.push(format!(
        "# graph {}; parameters {}; interface {}; metadata {}",
        if summary.graph_equal {
            "equal"
        } else {
            "differs"
        },
        match summary.parameters_equal {
            Some(true) => "equal",
            Some(false) => "differ",
            None => "not separable",
        },
        if summary.interface_equal {
            "equal"
        } else {
            "differs"
        },
        match summary.metadata_equal {
            Some(true) => "equal",
            Some(false) => "differs",
            None => "not compared",
        },
    ));
    let blocks = summary.blocks;
    let disjoint = diff.notes.contains(&Note::NamesDisjoint);
    if !disjoint && blocks.renamed + blocks.base_only + blocks.head_only > 0 {
        out.push(format!(
            "# blocks: {} by name, {} renamed, {} removed, {} added",
            blocks.by_name, blocks.renamed, blocks.base_only, blocks.head_only
        ));
    }
    for note in &diff.notes {
        out.push(match note {
            Note::NamesDisjoint => format!(
                "# names disjoint: no block kept its name; {} of {} matched by the rename pass, {} not matched",
                blocks.renamed,
                blocks.renamed + blocks.base_only,
                blocks.base_only
            ),
            Note::RenamePassSkipped => format!(
                "# rename pass skipped: more than {MAX_RENAME_CANDIDATES} unmatched blocks on a side or more than {MAX_RENAME_PAIRS} candidate pairs"
            ),
            Note::RenamePassOff => "# rename pass off".to_owned(),
        });
    }
    if !diff.parameters.is_empty() {
        out.push("# parameters".to_owned());
        diff.parameters
            .iter()
            .for_each(|entry| parameter(&mut out, entry));
    }
    if !diff.interface.is_empty() {
        out.push("# interface".to_owned());
        diff.interface
            .iter()
            .for_each(|entry| interface(&mut out, entry));
    }
    if !diff.graph.is_empty() {
        out.push("# graph".to_owned());
        diff.graph.iter().for_each(|entry| graph(&mut out, entry));
    }
    if !diff.metadata.is_empty() {
        out.push("# metadata".to_owned());
        diff.metadata
            .iter()
            .for_each(|entry| metadata(&mut out, entry));
    }
    out.text
}

/// The output, written one whole line at a time.
#[derive(Default)]
struct Lines {
    text: String,
}

impl Lines {
    /// One line: `line` with every control character spelled out, then a line feed.
    fn push(&mut self, line: String) {
        self.text.push_str(&one_line(&line));
        self.text.push('\n');
    }

    /// One line with a trailing comment: `body   # note; note`, or `body` alone with no notes.
    fn noted(&mut self, body: String, notes: &[String]) {
        if notes.is_empty() {
            self.push(body);
        } else {
            self.push(format!("{body}   # {}", notes.join("; ")));
        }
    }
}

fn affects(list: &[String]) -> Option<String> {
    (!list.is_empty()).then(|| format!("affects {}", list.join(", ")))
}

fn parameter(out: &mut Lines, entry: &ParameterChange) {
    match entry {
        ParameterChange::Added {
            subject,
            value_type,
            value,
            affects: list,
        } => out.noted(
            format!("+ param {subject} : {value_type} = {}", literal_text(value)),
            &affects(list).into_iter().collect::<Vec<_>>(),
        ),
        ParameterChange::Removed {
            subject,
            value_type,
            value,
            affects: list,
        } => out.noted(
            format!("- param {subject} : {value_type} = {}", literal_text(value)),
            &affects(list).into_iter().collect::<Vec<_>>(),
        ),
        ParameterChange::Changed {
            subject,
            value_type,
            old,
            new,
            affects: list,
        } => {
            let mut notes = vec![format!("was {}", literal_text(old))];
            notes.extend(affects(list));
            out.noted(
                format!("~ param {subject} : {value_type} = {}", literal_text(new)),
                &notes,
            );
        }
        ParameterChange::TypeChanged {
            subject,
            old_type,
            new_type,
            old,
            new,
            affects: list,
        } => {
            let mut notes = vec![format!("was {old_type} = {}", literal_text(old))];
            notes.extend(affects(list));
            out.noted(
                format!("~ param {subject} : {new_type} = {}", literal_text(new)),
                &notes,
            );
        }
        ParameterChange::Reordered { subject } => out.push(format!("~ # {subject} reordered")),
    }
}

fn or_none(value: Option<&String>) -> &str {
    value.map_or("none", String::as_str)
}

fn bound_or_none(value: Option<&Literal>) -> String {
    value.map_or_else(|| "none".to_owned(), literal_text)
}

/// One changed tag of a boundary: `~ in TOut @unit degC   # was K`.
fn tag(out: &mut Lines, direction: Direction, subject: &str, tag: &str, old: &str, new: &str) {
    out.noted(
        format!("~ {} {subject} @{tag} {new}", direction.keyword()),
        &[format!("was {old}")],
    );
}

fn interface(out: &mut Lines, entry: &InterfaceChange) {
    match entry {
        InterfaceChange::Added {
            subject,
            direction,
            value_type,
            unit,
            quantity,
            display_unit,
            min,
            max,
        }
        | InterfaceChange::Removed {
            subject,
            direction,
            value_type,
            unit,
            quantity,
            display_unit,
            min,
            max,
        } => {
            let sign = if matches!(entry, InterfaceChange::Added { .. }) {
                '+'
            } else {
                '-'
            };
            let declaration = connector_text(
                value_type,
                [
                    ("unit", unit.clone()),
                    ("quantity", quantity.clone()),
                    ("displayUnit", display_unit.clone()),
                    ("min", min.as_ref().map(literal_text)),
                    ("max", max.as_ref().map(literal_text)),
                ],
            );
            out.push(format!(
                "{sign} {} {subject} : {declaration}",
                direction.keyword()
            ));
        }
        InterfaceChange::TypeChanged {
            subject,
            direction,
            old,
            new,
        } => out.noted(
            format!("~ {} {subject} : {new}", direction.keyword()),
            &[format!("was {old}")],
        ),
        InterfaceChange::UnitChanged {
            subject,
            direction,
            old,
            new,
        } => tag(
            out,
            *direction,
            subject,
            "unit",
            or_none(old.as_ref()),
            or_none(new.as_ref()),
        ),
        InterfaceChange::QuantityChanged {
            subject,
            direction,
            old,
            new,
        } => tag(
            out,
            *direction,
            subject,
            "quantity",
            or_none(old.as_ref()),
            or_none(new.as_ref()),
        ),
        InterfaceChange::DisplayUnitChanged {
            subject,
            direction,
            old,
            new,
        } => tag(
            out,
            *direction,
            subject,
            "displayUnit",
            or_none(old.as_ref()),
            or_none(new.as_ref()),
        ),
        InterfaceChange::MinChanged {
            subject,
            direction,
            old,
            new,
        } => tag(
            out,
            *direction,
            subject,
            "min",
            &bound_or_none(old.as_ref()),
            &bound_or_none(new.as_ref()),
        ),
        InterfaceChange::MaxChanged {
            subject,
            direction,
            old,
            new,
        } => tag(
            out,
            *direction,
            subject,
            "max",
            &bound_or_none(old.as_ref()),
            &bound_or_none(new.as_ref()),
        ),
        InterfaceChange::Reordered { subject } => out.push(format!("~ # {subject} reordered")),
    }
}

fn graph(out: &mut Lines, entry: &GraphChange) {
    match entry {
        GraphChange::Added {
            subject,
            class,
            params,
            inputs,
        } => {
            out.push(format!("+ block {subject} = {}", block_text(class, params)));
            for input in inputs {
                out.push(format!("+ {} -> {subject}.{}", input.source, input.port));
            }
        }
        GraphChange::Removed {
            subject,
            class,
            params,
            inputs,
        } => {
            out.push(format!("- block {subject} = {}", block_text(class, params)));
            for input in inputs {
                out.push(format!("- {} -> {subject}.{}", input.source, input.port));
            }
        }
        GraphChange::Renamed {
            subject,
            from,
            class,
            params,
            confidence,
        } => out.noted(
            format!("> block {subject} = {}", block_text(class, params)),
            &[
                format!("was {from}"),
                format!(
                    "renamed, {}",
                    match confidence {
                        Confidence::Exact => "exact",
                        Confidence::High => "high",
                    }
                ),
            ],
        ),
        GraphChange::Reparameterized { subject, old, new } => {
            let was = old
                .as_ref()
                .map_or_else(|| "not assigned".to_owned(), literal_text);
            let body = match new {
                Some(new) => format!("~ {subject} = {}", literal_text(new)),
                None => format!("~ {subject} not assigned"),
            };
            out.noted(body, &[format!("was {was}")]);
        }
        GraphChange::Rewired { subject, old, new } => {
            let was = old.as_deref().unwrap_or("not driven");
            let body = match new {
                Some(new) => format!("~ {new} -> {subject}"),
                None => format!("~ {subject} not driven"),
            };
            out.noted(body, &[format!("was {was}")]);
        }
        GraphChange::Reordered { subject, list } => out.push(match list {
            GraphList::Blocks | GraphList::Connections => format!("~ # {subject} reordered"),
            GraphList::Assignments => format!("~ # assignments of {subject} reordered"),
        }),
        GraphChange::ParameterDefaulted {
            subject,
            value,
            side,
        } => {
            let value = literal_text(value);
            match side {
                Side::Base => out.noted(
                    format!("~ {subject} not assigned"),
                    &[format!("was {value}"), "the catalog default".to_owned()],
                ),
                Side::Head => out.noted(
                    format!("~ {subject} = {value}"),
                    &[
                        "was not assigned".to_owned(),
                        "the catalog default".to_owned(),
                    ],
                ),
            }
        }
    }
}

fn metadata(out: &mut Lines, entry: &MetadataChange) {
    match entry {
        MetadataChange::SequenceRenamed { old, new, .. } => out.noted(
            format!("~ sequence {}", or_none(new.as_ref())),
            &[format!("was {}", or_none(old.as_ref()))],
        ),
        MetadataChange::DerivedFromChanged { old, new, .. } => out.noted(
            format!("~ derived_from {}", or_none(new.as_ref())),
            &[format!("was {}", or_none(old.as_ref()))],
        ),
        MetadataChange::SemanticChanged { subject, .. } => {
            out.push(format!("~ # semantic {subject} changed"));
        }
        MetadataChange::CommentChanged { subject, .. } => {
            out.push(format!("~ # comment on {subject} changed"));
        }
        MetadataChange::LiteralRespelled { subject, old, new } => out.push(format!(
            "~ # literal {subject} respelled {}; was {}",
            or_none(new.as_ref()),
            or_none(old.as_ref())
        )),
    }
}
