//! The budget of text each validator pass holds its diagnostics to (issue #726): what fits is
//! listed whole, the first diagnostic that does not fit ends the list, and one closing
//! `resource-limit-exceeded` error, after the sorted diagnostics, counts the rest and refuses the
//! model, warnings included (the ruling of GitHub #713 for the reader's list, mirrored).
//!
//! The list itself is tested in `oce-diag`; these pin what the validator's entry points return.

use super::common::*;
use crate::validate_block_parameters;

/// The budget of message and subject text (`oce_diag::MAX_DIAGNOSTIC_TEXT_BYTES`), restated so
/// this test fails if it moves without review.
const BUDGET: usize = 8 * 1024 * 1024;

/// The closing error for `unlisted` diagnostics, the first of them described as `first`.
fn closing(unlisted: u64, first: &str) -> oce_diag::Diagnostic {
    oce_diag::Diagnostic::error(
        DiagCode::ResourceLimitExceeded,
        format!(
            "{unlisted} more diagnostic(s) are not listed, the first of them {first}: a \
             diagnostic list holds at most 8388608 bytes of message and subject text"
        ),
    )
}

/// The bytes a diagnostic spends of the budget: its message and its subject.
fn text(diag: &oce_diag::Diagnostic) -> usize {
    diag.message.len() + diag.subject.as_deref().map_or(0, str::len)
}

/// Two Real connectors wired together whose `displayUnit`s differ, `"a"` repeated `pad` times
/// and `"b"`: one `display-unit-divergence` warning, its message `pad` bytes longer than at 0.
fn divergent_pair(first_id: u32, pad: usize) -> [Connector; 2] {
    let display = |unit: String| RealAttrs {
        display_unit: Some(Arc::from(unit)),
        ..RealAttrs::default()
    };
    [
        real_conn(first_id, first_id, Dir::Out, display("a".repeat(pad))),
        real_conn(first_id + 1, first_id + 1, Dir::In, display("b".to_owned())),
    ]
}

/// A model of one divergent pair per entry of `pads`, in order.
fn divergent_model(pads: &[usize]) -> ModelGraph {
    let mut model = ModelGraph::new();
    for (index, &pad) in pads.iter().enumerate() {
        let first = 2 * index as u32;
        model.connectors.extend(divergent_pair(first, pad));
        model.connections.push(conn_edge(first, first + 1));
    }
    model
}

/// The text one divergent pair's warning spends at `pad` 0.
fn divergence_text_at_no_pad() -> usize {
    let warnings = unify_attributes(&mut divergent_model(&[0])).expect("a warning");
    assert_eq!(codes(&warnings), [DiagCode::DisplayUnitDivergence]);
    text(&warnings[0])
}

#[test]
fn warnings_that_fill_the_budget_exactly_pass_and_one_byte_more_refuses_the_model() {
    let pad = BUDGET - divergence_text_at_no_pad();
    let warnings = unify_attributes(&mut divergent_model(&[pad])).expect("exactly the budget");
    assert_eq!(warnings.len(), 1);
    assert_eq!(text(&warnings[0]), BUDGET);

    let err = unify_attributes(&mut divergent_model(&[pad + 1]))
        .expect_err("a warning past the budget refuses the model");
    assert_eq!(
        err.diagnostics,
        [closing(1, "`display-unit-divergence` on `connector#0`")]
    );
}

#[test]
fn the_budget_is_spent_by_every_listed_diagnostic_together() {
    let base = divergence_text_at_no_pad();
    // Two warnings; the second's subject, `connector#2`, is as long as `connector#0`.
    let first_pad = BUDGET - 2 * base;
    let both = unify_attributes(&mut divergent_model(&[first_pad, 0])).expect("together, exactly");
    assert_eq!(both.iter().map(text).sum::<usize>(), BUDGET);

    let err = unify_attributes(&mut divergent_model(&[first_pad + 1, 0]))
        .expect_err("one byte over, together");
    assert_eq!(
        err.diagnostics,
        [closing(1, "`display-unit-divergence` on `connector#2`")],
        "the first warning is listed and filtered out of the refusal; the second is counted"
    );
}

/// A block of an unregistered class, so only the class-independent rules run, carrying the
/// parameter `p` twice: one `duplicate-parameter` error, located on `subject`.
fn duplicate_block(id: u32, subject: &str) -> BlockInstance {
    BlockInstance {
        instance_iri: Some(Arc::from(subject)),
        ..block_with_params(id, "X", &[], &[], vec![rp("p", 1.0), rp("p", 2.0)])
    }
}

#[test]
fn errors_past_the_budget_close_with_a_count_after_the_sorted_ones() {
    let probe = validate(&ModelGraph {
        blocks: vec![duplicate_block(0, "")],
        ..ModelGraph::new()
    })
    .expect_err("a duplicate parameter");
    let message = probe.diagnostics[0].message.len();
    // Raised in block order: `urn:z…` fills half the budget, `urn:a…` the other half, and the
    // three after them do not fit. Sorted, `urn:a…` comes first, and the closing error stays last.
    let subject = |stem: &str| format!("{stem}{}", "x".repeat(BUDGET / 2 - message - stem.len()));
    let model = ModelGraph {
        blocks: vec![
            duplicate_block(0, &subject("urn:z")),
            duplicate_block(1, &subject("urn:a")),
            duplicate_block(2, "urn:c"),
            duplicate_block(3, "urn:b"),
            duplicate_block(4, "urn:d"),
        ],
        ..ModelGraph::new()
    };
    let err = validate(&model).expect_err("five duplicates");
    let subjects: Vec<&str> = err
        .diagnostics
        .iter()
        .map(|d| d.subject.as_deref().unwrap_or("(none)"))
        .collect();
    assert_eq!(
        subjects,
        [
            subject("urn:a").as_str(),
            subject("urn:z").as_str(),
            "(none)"
        ]
    );
    assert_eq!(
        err.diagnostics[2],
        closing(3, "`duplicate-parameter` on `urn:c`"),
        "the first left out in the order raised, not the order sorted"
    );
    // The same model, the same answer: what is listed follows the raised order, which is fixed.
    assert_eq!(
        validate(&model).expect_err("again").diagnostics,
        err.diagnostics
    );
}

#[test]
fn a_finding_larger_than_the_whole_budget_leaves_the_closing_error_alone() {
    let long = format!("urn:{}", "k".repeat(BUDGET));
    let err = validate(&ModelGraph {
        blocks: vec![duplicate_block(0, &long)],
        ..ModelGraph::new()
    })
    .expect_err("one finding, past the budget by itself");
    let quoted = format!("`duplicate-parameter` on `urn:{}…`", "k".repeat(60));
    assert_eq!(err.diagnostics, [closing(1, &quoted)]);
}

#[test]
fn a_block_s_parameters_alone_are_held_to_the_budget() {
    // `validate_block_parameters` names the block `block#0`; the message quotes the name.
    let name = "p".repeat(BUDGET);
    let params = ParamTable {
        values: vec![
            (Arc::from(name.as_str()), Value::Real(1.0)),
            (Arc::from(name.as_str()), Value::Real(2.0)),
        ],
    };
    let err = validate_block_parameters("X", &params).expect_err("past the budget");
    assert_eq!(
        err.diagnostics,
        [closing(1, "`duplicate-parameter` on `block#0`")]
    );
}
