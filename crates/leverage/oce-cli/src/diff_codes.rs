//! The codes `oce diff` passes through from `oce-diff`, each with a one-line meaning, in the order
//! of `oce_diff::DiffError::CODES` (pinned by the unit tests below). Every one is exit `2`: a
//! difference is never a code, it is an entry of the diff (diff note, decision 1).
//!
//! The list is `oce-diff`'s whole list, so that no refusal can carry a code `vocab` does not
//! print: it holds the two codes of the blob comparison, which `diff` does not expose (the verb
//! reads authoring models and CXF only), as the import table holds the codes of a reader `import`
//! does not expose.

/// `oce-diff`'s refusal codes and their meanings, in `oce_diff::DiffError::CODES` order.
pub(crate) const DIFF_MEANINGS: &[(&str, &str)] = &[
    (
        "outside-root",
        "a CXF block or boundary is not named under its document's root (one the engine exported included); at names the side",
    ),
    (
        "unresolved-ports",
        "the ports of a CXF block cannot be named from its class and ground parameters (never expected for a loaded document)",
    ),
    (
        "unreadable-blob",
        "the facade reads no summary from a blob (the blob comparison; diff reads no blob)",
    ),
    (
        "unexpected-refusal",
        "the facade refused a blob with an error that is not a blob refusal (never expected; diff reads no blob)",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_oce_diffs_code_list_in_order() {
        let codes: Vec<&str> = DIFF_MEANINGS.iter().map(|(code, _)| *code).collect();
        assert_eq!(codes, oce_diff::DiffError::CODES);
    }

    #[test]
    fn every_meaning_is_one_nonempty_line() {
        for (code, meaning) in DIFF_MEANINGS {
            assert!(!meaning.is_empty() && !meaning.contains('\n'), "{code}");
        }
    }
}
