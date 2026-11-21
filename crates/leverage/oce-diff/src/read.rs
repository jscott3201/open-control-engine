//! Reading a diff back, strictly (diff note, decisions 7 and 13).
//!
//! A [`Diff`] is written by this crate and read by receipts, cards and agents, some of which hand
//! it back. What is read is held to what is written:
//!
//! - **Closed objects, every key required.** A key the format does not define is refused, and so
//!   is a missing one: a value that may be absent is written `null` ([`nullable`]), so "not
//!   there" and "there and empty" cannot be confused.
//! - **The schema.** `schema` must be [`DIFF_SCHEMA`]. The format stays at revision 1 until
//!   1.0.0, so no other tag is a diff this build knows.
//! - **A Real is its bits.** `bits` must be `0x` and 16 lowercase hex digits, and `real` must be
//!   the spelling of exactly those bits, so the two halves cannot disagree.
//! - **The summary follows the sections.** `equal` must be "every section is empty", and each
//!   section flag "no entry other than `reordered`" (`null` only beside an empty section), so a
//!   reader that branches on a flag is never told something the entries contradict.
//! - **A CXF diff's flags are the level's.** It cannot tell its parameters apart, so
//!   `parameters_equal` is `null`; the only metadata it sees is a renamed root, so its metadata
//!   holds at most `sequence-renamed` entries and `metadata_equal` is `false` beside one and
//!   `null` without one (owner ruling of 2026-11-21). An authoring diff is held to the rule
//!   above only.
//!
//! Two things are taken as written: the block counts, and the order of the entries.

use serde::{Deserialize, Deserializer};

use crate::changes::{
    GraphChange, InterfaceChange, MetadataChange, ParameterChange, same_but_for_order,
};
use crate::data::{DIFF_SCHEMA, Diff, Literal, Note, SequenceRef, Source, Summary};

/// Read a value that may be `null` and must be present: a field read through this is required,
/// where serde would read a missing `Option` field as `None`.
pub(crate) fn nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

/// A [`Diff`] as its JSON has it, before the checks.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawDiff {
    schema: String,
    source: Source,
    catalog: String,
    base: SequenceRef,
    head: SequenceRef,
    summary: Summary,
    notes: Vec<Note>,
    parameters: Vec<ParameterChange>,
    interface: Vec<InterfaceChange>,
    graph: Vec<GraphChange>,
    metadata: Vec<MetadataChange>,
}

impl TryFrom<RawDiff> for Diff {
    type Error = String;

    fn try_from(raw: RawDiff) -> Result<Self, String> {
        if raw.schema != DIFF_SCHEMA {
            return Err(format!("schema `{}` is not `{DIFF_SCHEMA}`", raw.schema));
        }
        let summary = raw.summary;
        let empty = raw.parameters.is_empty()
            && raw.interface.is_empty()
            && raw.graph.is_empty()
            && raw.metadata.is_empty();
        let flags = [
            ("equal", Some(summary.equal), empty, empty),
            (
                "graph_equal",
                Some(summary.graph_equal),
                same_but_for_order(&raw.graph),
                raw.graph.is_empty(),
            ),
            (
                "parameters_equal",
                summary.parameters_equal,
                same_but_for_order(&raw.parameters),
                raw.parameters.is_empty(),
            ),
            (
                "interface_equal",
                Some(summary.interface_equal),
                same_but_for_order(&raw.interface),
                raw.interface.is_empty(),
            ),
            (
                "metadata_equal",
                summary.metadata_equal,
                raw.metadata.is_empty(),
                raw.metadata.is_empty(),
            ),
        ];
        for (name, written, expected, section_is_empty) in flags {
            match written {
                Some(flag) if flag != expected => {
                    return Err(format!(
                        "summary.{name} is {flag} and the sections make it {expected}"
                    ));
                }
                None if !section_is_empty => {
                    return Err(format!(
                        "summary.{name} is null beside a section that has entries"
                    ));
                }
                _ => {}
            }
        }
        let known = match raw.source {
            Source::Authoring => true,
            Source::Cxf => {
                summary.parameters_equal.is_none()
                    && summary.metadata_equal == (!raw.metadata.is_empty()).then_some(false)
                    && raw
                        .metadata
                        .iter()
                        .all(|entry| matches!(entry, MetadataChange::SequenceRenamed { .. }))
            }
        };
        if !known {
            return Err(
                "summary.parameters_equal, summary.metadata_equal or the metadata section are not what a cxf diff writes"
                    .to_owned(),
            );
        }
        Ok(Self {
            schema: raw.schema,
            source: raw.source,
            catalog: raw.catalog,
            base: raw.base,
            head: raw.head,
            summary,
            notes: raw.notes,
            parameters: raw.parameters,
            interface: raw.interface,
            graph: raw.graph,
            metadata: raw.metadata,
        })
    }
}

/// A [`Literal`] as its JSON has it, before the check of a Real.
#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(crate) enum RawLiteral {
    Real {
        real: String,
        bits: String,
    },
    Integer {
        integer: i64,
    },
    Boolean {
        boolean: bool,
    },
    Text {
        text: String,
    },
    Ref {
        #[serde(rename = "ref")]
        name: String,
    },
    Member {
        member: String,
    },
}

impl TryFrom<RawLiteral> for Literal {
    type Error = String;

    fn try_from(raw: RawLiteral) -> Result<Self, String> {
        Ok(match raw {
            RawLiteral::Real { real, bits } => {
                let digits = bits
                    .strip_prefix("0x")
                    .filter(|digits| {
                        digits.len() == 16
                            && digits
                                .bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    })
                    .ok_or_else(|| {
                        format!("bits `{bits}` is not `0x` and 16 lowercase hex digits")
                    })?;
                let value = u64::from_str_radix(digits, 16)
                    .map(f64::from_bits)
                    .map_err(|error| format!("bits `{bits}`: {error}"))?;
                // The bits are canonical by now, so the value written back has the same ones.
                if real != format!("{value:?}") {
                    return Err(format!(
                        "real `{real}` is not the spelling of its bits, `{value:?}`"
                    ));
                }
                Self::real(value)
            }
            RawLiteral::Integer { integer } => Self::Integer { integer },
            RawLiteral::Boolean { boolean } => Self::Boolean { boolean },
            RawLiteral::Text { text } => Self::Text { text },
            RawLiteral::Ref { name } => Self::Ref { name },
            RawLiteral::Member { member } => Self::Member { member },
        })
    }
}
