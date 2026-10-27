//! CDL-legal feedback through a multi-output matrix reduction is refused at load with the
//! `block-algebraic-loop` code (GitHub issue #347).
//!
//! `CDL.Reals.MatrixMax` and `CDL.Reals.MatrixMin` with two or more outputs give each output only
//! its own row (`rowMax`/`rowMin = true`) or column. Feeding one output back, through a gain, into
//! an element of another row or column leaves the connector graph acyclic, but atomic block firing
//! cannot place the block. These tests drive that refusal end to end from CXF, mirroring the
//! `RealExtractSignal` refusal in `algebraic_loop_diagnostics`, on both reduction axes.

use serde_json::{Value, json};

use super::algebraic_loop_diagnostics::{build_message, loop_order, refuse, render};
use crate::{DiagnosticSeverity, DiagnosticStage, DiagnosticSubject, Engine, OcError};

/// `MatrixMin` row reduction of a 2x1 matrix (`y1 = u[1,1]`, `y2 = u[2,1]`) whose `y1` returns
/// through a gain to `u[2,1]`.
const MATRIX_MIN_BLOCK_LOOP_GOLDEN: &str =
    include_str!("fixtures/golden/matrix_min_block_loop.receipt.txt");
/// `MatrixMax` column reduction of a 1x2 matrix (`y1 = u[1,1]`, `y2 = u[1,2]`) whose `y1` returns
/// through a gain to `u[1,2]`.
const MATRIX_MAX_BLOCK_LOOP_GOLDEN: &str =
    include_str!("fixtures/golden/matrix_max_block_loop.receipt.txt");

const PREFIX: &str = "http://example.org#mat.";

fn iri(local: &str) -> String {
    format!("{PREFIX}{local}")
}

/// A reduction block with local name `red` (each class uses its own, so the two receipts differ),
/// two inputs `u1`, `u2` and two outputs `y1`, `y2`, plus a gain `gai` fed by `red.y<from>`. With
/// `to = Some(n)`, `gai → red.u<n>` closes the feedback and the boundary input drives the other
/// reduction input; with `None` the gain drives nothing and the boundary input drives both. The
/// reduction's other output drives the boundary output.
fn reduction_feedback(
    class: &str,
    red: &str,
    params: &[(&str, Value)],
    from: u8,
    to: Option<u8>,
) -> Vec<u8> {
    let real = || json!({ "@id": "S231:Real" });
    let boundary_targets: Vec<Value> = [1, 2]
        .into_iter()
        .filter(|port| Some(*port) != to)
        .map(|port| json!({ "@id": iri(&format!("{red}.u{port}")) }))
        .collect();
    let mut gain_output = json!({
        "@id": iri("gai.y"), "@type": "S231:RealOutput", "S231:isOfDataType": real(),
    });
    if let Some(to) = to {
        gain_output["S231:isConnectedTo"] = json!({ "@id": iri(&format!("{red}.u{to}")) });
    }
    let mut graph = vec![
        json!({
            "@id": "http://example.org#mat",
            "@type": "S231:Block",
            "S231:containsBlock": [{ "@id": iri(red) }, { "@id": iri("gai") }],
            "S231:hasInput": { "@id": iri("u") },
            "S231:hasOutput": { "@id": iri("y") },
        }),
        json!({
            "@id": iri("u"), "@type": "S231:RealInput", "S231:isOfDataType": real(),
            "S231:isConnectedTo": boundary_targets,
        }),
        json!({ "@id": iri("y"), "@type": "S231:RealOutput", "S231:isOfDataType": real() }),
        json!({
            "@id": iri(red),
            "@type": format!("http://example.org#Buildings.Controls.OBC.CDL.Reals.{class}"),
            "S231:hasParameter": params
                .iter()
                .map(|(name, _)| json!({ "@id": iri(&format!("{red}.{name}")) }))
                .collect::<Vec<_>>(),
            "S231:hasInput": [
                { "@id": iri(&format!("{red}.u1")) },
                { "@id": iri(&format!("{red}.u2")) },
            ],
            "S231:hasOutput": [
                { "@id": iri(&format!("{red}.y1")) },
                { "@id": iri(&format!("{red}.y2")) },
            ],
        }),
        json!({
            "@id": iri("gai"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
            "S231:hasParameter": { "@id": iri("gai.k") },
            "S231:hasInput": { "@id": iri("gai.u") },
            "S231:hasOutput": { "@id": iri("gai.y") },
        }),
        json!({
            "@id": iri("gai.k"), "@type": "S231:Parameter", "S231:isOfDataType": real(),
            "S231:value": { "@value": "0.5", "@type": "http://www.w3.org/2001/XMLSchema#double" },
        }),
        json!({ "@id": iri("gai.u"), "@type": "S231:RealInput", "S231:isOfDataType": real() }),
        gain_output,
    ];
    for (name, value) in params {
        let data_type = if value.is_boolean() {
            "S231:Boolean"
        } else {
            "S231:Integer"
        };
        graph.push(json!({
            "@id": iri(&format!("{red}.{name}")), "@type": "S231:Parameter",
            "S231:isOfDataType": { "@id": data_type }, "S231:value": value,
        }));
    }
    for port in ["u1", "u2"] {
        graph.push(json!({
            "@id": iri(&format!("{red}.{port}")), "@type": "S231:RealInput",
            "S231:isOfDataType": real(),
        }));
    }
    for port in [1, 2] {
        let target = if port == from { iri("gai.u") } else { iri("y") };
        graph.push(json!({
            "@id": iri(&format!("{red}.y{port}")), "@type": "S231:RealOutput",
            "S231:isOfDataType": real(), "S231:isConnectedTo": { "@id": target },
        }));
    }
    serde_json::to_vec(&json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph,
    }))
    .expect("serialize document")
}

fn matrix_min_rows(from: u8, to: Option<u8>) -> Vec<u8> {
    reduction_feedback(
        "MatrixMin",
        "min",
        &[
            ("nRow", json!(2)),
            ("nCol", json!(1)),
            ("rowMin", json!(true)),
        ],
        from,
        to,
    )
}

fn matrix_max_columns(from: u8, to: Option<u8>) -> Vec<u8> {
    reduction_feedback(
        "MatrixMax",
        "max",
        &[
            ("nRow", json!(1)),
            ("nCol", json!(2)),
            ("rowMax", json!(false)),
        ],
        from,
        to,
    )
}

fn assert_block_loop_refusal(bytes: &[u8], red: &str, golden: &str) {
    let failure = refuse(bytes);
    assert_eq!(failure.stage(), DiagnosticStage::Schedule);
    assert_eq!(render(failure.diagnostics()), golden);
    let observed: Vec<_> = failure
        .diagnostics()
        .records()
        .iter()
        .map(|record| {
            (
                record.key().stage(),
                record.key().severity(),
                record.key().code(),
                record.key().subject().clone(),
            )
        })
        .collect();
    assert_eq!(
        observed,
        [iri("gai"), iri(red)].map(|subject| (
            DiagnosticStage::Schedule,
            DiagnosticSeverity::Error,
            "block-algebraic-loop",
            DiagnosticSubject::Opaque(subject),
        ))
    );
    assert_eq!(
        loop_order(failure.diagnostics(), "block-algebraic-loop"),
        [iri(red), iri("gai")]
    );
    assert!(matches!(failure.error(), OcError::Build(_)), "{failure:?}");
    let message = build_message(&failure);
    for needle in [
        "block-granularity algebraic loop: 2 block(s)",
        "no connector-level feedthrough cycle exists",
        "The model may be CDL-legal",
    ] {
        assert!(message.contains(needle), "missing {needle:?}: {message}");
    }
    assert!(!message.contains("§7.16 forbids"), "{message}");
}

#[test]
fn matrix_min_row_feedback_is_refused_as_a_block_loop() {
    assert_block_loop_refusal(
        &matrix_min_rows(1, Some(2)),
        "min",
        MATRIX_MIN_BLOCK_LOOP_GOLDEN,
    );
}

#[test]
fn matrix_max_column_feedback_is_refused_as_a_block_loop() {
    assert_block_loop_refusal(
        &matrix_max_columns(1, Some(2)),
        "max",
        MATRIX_MAX_BLOCK_LOOP_GOLDEN,
    );
}

#[test]
fn repeated_matrix_reduction_refusals_produce_identical_receipts() {
    for bytes in [matrix_min_rows(1, Some(2)), matrix_max_columns(1, Some(2))] {
        let first = render(refuse(&bytes).diagnostics());
        for _ in 0..3 {
            assert_eq!(render(refuse(&bytes).diagnostics()), first);
        }
    }
}

#[test]
fn matrix_reduction_feedback_into_its_own_row_is_a_connector_level_loop() {
    // `y1` depends on `u1`, so `y1 → gain → u1` is a true algebraic loop, refused under the
    // connector-level code rather than the block-granularity one.
    for bytes in [matrix_min_rows(1, Some(1)), matrix_max_columns(1, Some(1))] {
        let failure = refuse(&bytes);
        let codes: Vec<&str> = failure
            .diagnostics()
            .records()
            .iter()
            .map(|record| record.key().code())
            .collect();
        assert!(!codes.is_empty());
        assert!(
            codes.iter().all(|code| *code == "algebraic-loop"),
            "{codes:?}"
        );
    }
}

#[test]
fn matrix_reduction_without_feedback_loads() {
    // The same blocks and shapes the refusal tests use (MatrixMin over rows, MatrixMax over
    // columns), with the gain fed from `y1` but driving nothing back into the reduction: the
    // refusal is caused by the feedback, not by the class or its parameters.
    for (class, bytes) in [
        ("MatrixMin", matrix_min_rows(1, None)),
        ("MatrixMax", matrix_max_columns(1, None)),
    ] {
        Engine::in_memory()
            .load_cxf(&bytes)
            .unwrap_or_else(|err| panic!("{class} without feedback must load: {err:?}"));
    }
}
