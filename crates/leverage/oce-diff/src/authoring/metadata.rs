//! The document-level metadata: the sequence name, what it derives from, and the comments above
//! the `sequence` line and after the last statement. (Comments on statements, semantic fields and
//! spellings are added by the module that compares the statement.)

use oce_author::Sequence;

use crate::changes::MetadataChange;

use super::blocks::commented;

/// Add the document-level metadata entries.
pub(super) fn document(base: &Sequence, head: &Sequence, metadata: &mut Vec<MetadataChange>) {
    if base.name != head.name {
        metadata.push(MetadataChange::SequenceRenamed {
            subject: "sequence".to_owned(),
            old: Some(base.name.as_str().to_owned()),
            new: Some(head.name.as_str().to_owned()),
        });
    }
    if base.derived_from != head.derived_from {
        let id = |sequence: &Sequence| {
            sequence
                .derived_from
                .as_ref()
                .map(|id| id.as_str().to_owned())
        };
        metadata.push(MetadataChange::DerivedFromChanged {
            subject: "derived_from".to_owned(),
            old: id(base),
            new: id(head),
        });
    }
    commented(
        "sequence",
        &base.annotations.comments,
        &head.annotations.comments,
        metadata,
    );
    commented(
        "sequence.trailing",
        &base.trailing_comments,
        &head.trailing_comments,
        metadata,
    );
}
