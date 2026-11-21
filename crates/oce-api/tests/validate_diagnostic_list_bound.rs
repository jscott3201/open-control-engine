//! What a host sees when one validator pass's diagnostics pass the budget of text (issue #726),
//! through `Engine::load_cxf`.
//!
//! Each validator pass lists its diagnostics while their messages and subjects fit 8 MiB, a
//! subject counted for every diagnostic that carries it, as the reader does (GitHub #713). The
//! first that does not fit ends the list, and one `resource-limit-exceeded` error closes it after
//! the pass's sorted diagnostics: last in `OcError::diagnostics()`. The reader's own list is
//! measured in `cxf_diagnostic_list_bound.rs`; nothing here reaches it.

use oce_api::{Engine, OcError};
use oce_diag::{DiagCode, Severity};

const ROOT: &str = "http://example.org#M";

/// The bytes of the block's `@id`: 16 findings on it are just over the budget, 15 are not.
const ID_BYTES: usize = 512 * 1024;

/// A root composite holding one `CDL.Reals.Sources.TimeTable` whose `@id` is [`ID_BYTES`] long and
/// whose table has `rows` rows of one Boolean output cell and no time column: one validator
/// finding a cell (not numeric) and one for the absent time cells, all on the block's `@id`. The
/// parameter nodes have short `@id`s, so the document is about two `@id`s long.
fn document(rows: usize) -> (String, Vec<u8>) {
    let stem = format!("{ROOT}.t");
    let id = format!("{stem}{}", "x".repeat(ID_BYTES - stem.len()));
    let cells: Vec<String> = (1..=rows)
        .map(|row| format!("urn:p{row}.table_{row}_2"))
        .collect();
    let references = cells
        .iter()
        .map(|cell| format!(r#"{{"@id":"{cell}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let mut nodes = vec![
        format!(
            r#"{{"@id":"{ROOT}","@type":"S231:Block","S231:containsBlock":[{{"@id":"{id}"}}]}}"#
        ),
        format!(
            r#"{{"@id":"{id}","@type":"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.TimeTable","S231:hasParameter":[{references}],"S231:hasOutput":{{"@id":"urn:y.y"}}}}"#
        ),
        r#"{"@id":"urn:y.y","@type":"S231:RealOutput","S231:isOfDataType":{"@id":"S231:Real"}}"#
            .to_owned(),
    ];
    nodes.extend(cells.iter().map(|cell| {
        format!(
            r#"{{"@id":"{cell}","@type":"S231:Parameter","S231:isOfDataType":{{"@id":"S231:Boolean"}},"S231:value":true}}"#
        )
    }));
    let bytes = format!(
        r#"{{"@context":{{"S231":"http://data.ashrae.org/S231#"}},"@graph":[{}]}}"#,
        nodes.join(",")
    )
    .into_bytes();
    (id, bytes)
}

fn refusal(bytes: &[u8]) -> OcError {
    Engine::new()
        .load_cxf(bytes)
        .expect_err("a table of Boolean cells is refused")
}

#[test]
fn validator_findings_within_the_budget_are_listed_whole_with_no_closing_error() {
    let (id, bytes) = document(14);
    let error = refusal(&bytes);
    assert!(matches!(error, OcError::Validate(_)), "{error:.200}");
    let diagnostics = error.diagnostics();
    assert_eq!(diagnostics.len(), 15, "14 cells and the absent time column");
    assert!(
        diagnostics
            .iter()
            .all(|d| d.subject.as_deref() == Some(id.as_str()))
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == DiagCode::ResourceLimitExceeded)
    );
}

#[test]
fn validator_findings_past_the_budget_close_with_a_count_last_in_the_refusal() {
    let (id, bytes) = document(20);
    // The `@id` twice (the block and the root's reference to it) and a little more: 1 MiB of
    // document for 10.5 MiB of findings.
    assert!(
        bytes.len() < 2 * ID_BYTES + 8 * 1024,
        "{} bytes",
        bytes.len()
    );
    let error = refusal(&bytes);
    assert!(matches!(error, OcError::Validate(_)), "{error:.200}");
    let diagnostics = error.diagnostics();
    // 21 findings of about 512 KiB each: 15 fit in 8 MiB, 6 are counted.
    let (closing, listed) = diagnostics.split_last().expect("diagnostics");
    assert_eq!(listed.len(), 15);
    assert!(listed.iter().all(|d| {
        d.subject.as_deref() == Some(id.as_str()) && d.code == DiagCode::ParameterOutOfRange
    }));
    assert_eq!(closing.code, DiagCode::ResourceLimitExceeded);
    assert_eq!(closing.severity, Severity::Error);
    assert_eq!(closing.subject, None);
    assert_eq!(
        closing.message,
        format!(
            "6 more diagnostic(s) are not listed, the first of them `parameter-out-of-range` on \
             `{}…`: a diagnostic list holds at most 8388608 bytes of message and subject text",
            &id[..64]
        )
    );
    // The listed ones are the first raised: the absent time column, then rows 1 to 14.
    assert!(
        listed
            .iter()
            .any(|d| d.message.contains("missing 20 required cells"))
    );
    for row in 1..=14 {
        let name = format!("`table_{row}_2`");
        assert!(listed.iter().any(|d| d.message.contains(&name)), "{name}");
    }
    // Sorted within the pass, the closing error after them.
    assert!(
        listed
            .windows(2)
            .all(|pair| pair[0].message <= pair[1].message)
    );
}
