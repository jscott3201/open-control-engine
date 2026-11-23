//! The label budget unification checks (issue #815): what counts, that a propagated label counts
//! as the same label written on each connector, that order does not matter, and that a refusal
//! rolls the propagation back.

use super::common::*;

use crate::rules::{MAX_DOCUMENT_LABEL_BYTES, document_label_bytes};

/// One `Constant` output (connector 0) driving `targets` `Abs` inputs (connectors 1, 3, 5, …,
/// outputs 2, 4, 6, …). The output carries `unit`, `quantity` and `display_unit`; the first
/// `literal` inputs carry the same unit and quantity themselves. With `reversed`, the
/// connections are listed last first.
fn star(
    unit: &str,
    quantity: &str,
    display_unit: &str,
    targets: u32,
    literal: u32,
    reversed: bool,
) -> ModelGraph {
    let labels = |unit: &str, quantity: &str, display_unit: Option<&str>| RealAttrs {
        unit: Some(Arc::from(unit)),
        quantity: Some(Arc::from(quantity)),
        display_unit: display_unit.map(Arc::from),
        ..RealAttrs::default()
    };
    let mut model = ModelGraph {
        blocks: vec![constant_block(0, &[0])],
        connectors: vec![real_conn(
            0,
            0,
            Dir::Out,
            labels(unit, quantity, Some(display_unit)),
        )],
        ..ModelGraph::new()
    };
    for t in 0..targets {
        let (input, output) = (1 + 2 * t, 2 + 2 * t);
        model
            .blocks
            .push(block(1 + t, "CDL.Reals.Abs", &[input], &[output]));
        let attrs = if t < literal {
            labels(unit, quantity, None)
        } else {
            RealAttrs::default()
        };
        model
            .connectors
            .push(real_conn(input, 1 + t, Dir::In, attrs));
        model
            .connectors
            .push(real_conn(output, 1 + t, Dir::Out, RealAttrs::default()));
        model.connections.push(conn_edge(0, input));
    }
    if reversed {
        model.connections.reverse();
    }
    model
}

#[test]
fn a_propagated_label_counts_once_for_every_connector_that_carries_it() {
    // 5 + 3 bytes on the output and on each of 4 inputs after propagation, and the output's
    // 2-byte display unit, which is never propagated: (4 + 1) × 8 + 2.
    for literal in [0, 2, 4] {
        for reversed in [false, true] {
            let mut model = star("kg/s2", "Qty", "dU", 4, literal, reversed);
            unify_attributes(&mut model).expect("within the budget");
            assert_eq!(
                document_label_bytes(&model),
                42,
                "{literal} literal, reversed {reversed}"
            );
        }
    }
}

#[test]
fn declared_outputs_count_and_attribute_sets_without_text_do_not() {
    let mut model = star("K", "T", "C", 1, 0, false);
    model.boundary_outputs.push(boundary_output(
        "urn:top:y",
        0,
        Attrs::Real(RealAttrs::default()),
    ));
    let mut integer = conn(9, 0, Dir::Out, ValueType::Integer);
    integer.attrs = Attrs::Integer(IntAttrs {
        min: Some(0),
        max: Some(3),
    });
    model.connectors.push(integer);
    unify_attributes(&mut model).expect("within the budget");
    // Output 3 (K, T, C), input 2 (K, T), declared output 2 (K, T, propagated), integer 0.
    assert_eq!(document_label_bytes(&model), 7);
}

#[test]
fn labels_past_the_budget_are_refused_whatever_their_spelling_or_order_and_roll_back() {
    // 8 inputs reached by a unit and a quantity of `half` bytes each: 9 connectors carrying
    // `2 × half` bytes, plus the 1-byte display unit. At `half` = (budget − 1) / 18 rounded
    // down the total fits; one byte longer it does not.
    let half = (MAX_DOCUMENT_LABEL_BYTES - 1) / 18;
    let fits = 9 * 2 * half + 1;
    assert!(fits <= MAX_DOCUMENT_LABEL_BYTES);
    assert!(9 * 2 * (half + 1) + 1 > MAX_DOCUMENT_LABEL_BYTES);
    for literal in [0, 3, 8] {
        for reversed in [false, true] {
            let what = format!("{literal} literal, reversed {reversed}");
            let unit = "u".repeat(half);
            let quantity = "q".repeat(half);
            let mut model = star(&unit, &quantity, "d", 8, literal, reversed);
            unify_attributes(&mut model).unwrap_or_else(|e| panic!("{what}: {e:?}"));
            assert_eq!(document_label_bytes(&model), fits, "{what}");

            let unit = "u".repeat(half + 1);
            let quantity = "q".repeat(half + 1);
            let mut model = star(&unit, &quantity, "d", 8, literal, reversed);
            let before = document_label_bytes(&model);
            let error = unify_attributes(&mut model).expect_err("past the budget");
            assert_eq!(
                codes(&error.diagnostics),
                [DiagCode::ResourceLimitExceeded],
                "{what}"
            );
            let total = 9 * 2 * (half + 1) + 1;
            assert!(
                error.diagnostics[0]
                    .message
                    .contains(&format!("take {total} bytes")),
                "{what}: {}",
                error.diagnostics[0].message
            );
            assert_eq!(error.diagnostics[0].subject, None, "{what}");
            // Nothing stays propagated.
            assert_eq!(document_label_bytes(&model), before, "{what}");
        }
    }
}

#[test]
fn a_bound_conflict_is_reported_alone_and_the_budget_is_not_counted() {
    // Labels past the budget once propagated, and a bound conflict in the same cluster: the
    // conflict is the refusal, and the labels are not counted beside it.
    let half = MAX_DOCUMENT_LABEL_BYTES / 18 + 1;
    let unit = "u".repeat(half);
    let quantity = "q".repeat(half);
    let mut model = star(&unit, &quantity, "d", 8, 0, false);
    // The output's and the first input's lower bounds differ; the labels still propagate to
    // every input, past the budget.
    if let Attrs::Real(output) = &mut model.connectors[0].attrs {
        output.min = Some(1.0);
    }
    model.connectors[1].attrs = Attrs::Real(RealAttrs {
        min: Some(0.0),
        ..RealAttrs::default()
    });
    let error = unify_attributes(&mut model).expect_err("a conflict");
    assert_eq!(codes(&error.diagnostics), [DiagCode::BoundMismatch]);
}
