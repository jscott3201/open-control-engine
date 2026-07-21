//! Array-*expression* parameter grounding tests for doc 04 §3.6.1 (issue #142). Headline: the
//! three encodings of the same array parameter — preserved value **list**, **flattened**
//! per-element scalars, and preserved value **expression** (`fill(1, nin)`) — lower to a
//! **byte-identical** `ModelGraph`, pinned by a checked-in golden and a twice-import determinism
//! check. Plus the expression-path contract: `{…}` literal and `a:b` range variants (element
//! types preserved), the declared-dims shape check (expected vs got), the 2-D-expression
//! deferral, the no-broadcast rule for a scalar-evaluating expression, the `NonScalar` rejection
//! of an array-evaluating expression on a scalar parameter, the `ArrayFlattenCollision` gate on
//! expression-minted names, the Step-7 forward-reference limitation, and the hostile
//! `fill(1, 2e9)` count dying as a typed diagnostic (oce-expr `ArrayTooLarge`), never an OOM.
//! Floats compared by IEEE-754 bits (`TESTING.md` pillar 2).
//!
//! Regenerate the golden after an intentional change:
//! ```text
//! OCE_BLESS=1 cargo test -p oce-cxf --test resolve_array_expression golden_array_expression_modelgraph
//! ```

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::DiagCode;
use oce_model::{ModelGraph, Value};
use serde_json::{Value as Json, json};

const EXPRESSION: &str = include_str!("fixtures/array_expression_preserved.jsonld");
const GOLDEN_REL: &str = "tests/fixtures/golden/array_expression.modelgraph.txt";

// ---- bit-exact deterministic render (mirrors resolve_array.rs / resolve_golden.rs) -----------

fn render(g: &ModelGraph) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "blocks: {}", g.blocks.len());
    for b in &g.blocks {
        let _ = writeln!(
            s,
            "  B{} decl={} class={} instance_iri={:?}",
            b.id.0,
            b.decl_order,
            b.class_iri,
            b.instance_iri.as_deref()
        );
        let _ = writeln!(
            s,
            "    inputs={:?} outputs={:?}",
            id_list(&b.inputs),
            id_list(&b.outputs)
        );
        for (name, v) in &b.params.values {
            let _ = writeln!(s, "    param {name}={}", render_value(v));
        }
    }
    let _ = writeln!(s, "connectors: {}", g.connectors.len());
    for c in &g.connectors {
        let _ = writeln!(
            s,
            "  C{} block=B{} dir={:?} type={:?} decl={} iri={:?}",
            c.id.0,
            c.block.0,
            c.dir,
            c.value_type,
            c.decl_order,
            c.iri.as_deref()
        );
    }
    let _ = writeln!(s, "connections: {}", g.connections.len());
    for c in &g.connections {
        let _ = writeln!(s, "  C{} -> C{}", c.from.0, c.to.0);
    }
    let _ = writeln!(
        s,
        "external_inputs: {:?}",
        g.external_inputs.iter().map(|c| c.0).collect::<Vec<_>>()
    );
    s
}

fn id_list<T: std::fmt::Debug>(ids: &[T]) -> Vec<String> {
    ids.iter().map(|i| format!("{i:?}")).collect()
}

fn render_value(v: &Value) -> String {
    match v {
        Value::Real(r) => format!("Real(0x{:016x})", r.to_bits()),
        Value::Integer(i) => format!("Integer({i})"),
        Value::Boolean(b) => format!("Boolean({b})"),
        Value::String(s) => format!("String({s:?})"),
        Value::Enum { class, ordinal } => format!("Enum(class={},ordinal={})", class.0, ordinal),
    }
}

// ---- import helpers -------------------------------------------------------------------------

fn import_ok(src: &str) -> ModelGraph {
    let (g, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("array-expression fixture must resolve without error");
    assert!(
        report.is_empty(),
        "expected zero diagnostics, got: {:?}",
        report.diagnostics
    );
    g
}

fn import_json(doc: &Json) -> Result<ModelGraph, CxfError> {
    let bytes = serde_json::to_vec(doc).expect("serialize doc");
    import_cxf(&bytes, &ResolveOptions::default()).map(|(g, _)| g)
}

/// Assert the import fails with at least one **error** diagnostic of `code`.
#[track_caller]
fn assert_error_code(doc: &Json, code: DiagCode) {
    match import_json(doc) {
        Err(CxfError::Validation(diags)) => assert!(
            diags.iter().any(|d| d.code == code && d.is_error()),
            "expected error diagnostic {code:?}, got {diags:#?}"
        ),
        Ok(_) => panic!("expected Validation({code:?}), but import succeeded"),
        Err(other) => panic!("expected Validation({code:?}), got {other:?}"),
    }
}

/// Assert the import fails with a `GroundingFailed` **error** whose message contains every needle
/// (pins the diagnostic wording the contract promises, e.g. expected-vs-got element counts).
#[track_caller]
fn assert_grounding_failed_with(doc: &Json, needles: &[&str]) {
    match import_json(doc) {
        Err(CxfError::Validation(diags)) => assert!(
            diags.iter().any(|d| d.code == DiagCode::GroundingFailed
                && d.is_error()
                && needles.iter().all(|n| d.message.contains(n))),
            "expected a GroundingFailed error containing {needles:?}, got {diags:#?}"
        ),
        Ok(_) => panic!("expected GroundingFailed containing {needles:?}, but import succeeded"),
        Err(other) => panic!("expected Validation(GroundingFailed), got {other:?}"),
    }
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_REL)
}

// ---- document builders ----------------------------------------------------------------------

/// An `{"@id": "…A.con.<name>"}` parameter reference.
fn pref(name: &str) -> Json {
    json!({ "@id": format!("http://example.org#A.con.{name}") })
}

/// A minimal valid composite whose one `Constant` block carries the given parameter-reference
/// array and parameter nodes — the shared skeleton of every document in this suite (mirrors
/// `array_expression_preserved.jsonld`).
fn constant_doc(param_refs: Json, param_nodes: &[Json]) -> Json {
    let mut graph = vec![
        json!({ "@id": "http://example.org#A", "@type": "S231:Block", "S231:label": "A",
                "S231:containsBlock": [ { "@id": "http://example.org#A.con" } ],
                "S231:hasOutput": { "@id": "http://example.org#A.yOut" } }),
        json!({ "@id": "http://example.org#A.con",
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                "S231:label": "con",
                "S231:hasParameter": param_refs,
                "S231:hasOutput": { "@id": "http://example.org#A.con.y" } }),
    ];
    graph.extend_from_slice(param_nodes);
    graph.push(
        json!({ "@id": "http://example.org#A.con.y", "@type": "S231:RealOutput",
                       "S231:isOfDataType": { "@id": "S231:Real" },
                       "S231:isConnectedTo": { "@id": "http://example.org#A.yOut" } }),
    );
    graph.push(
        json!({ "@id": "http://example.org#A.yOut", "@type": "S231:RealOutput",
                       "S231:isOfDataType": { "@id": "S231:Real" } }),
    );
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": graph
    })
}

/// One array parameter `k` with the given decorated label, dims, and value (the expression-value
/// analogue of resolve_array.rs's `array_doc`).
fn expr_array_doc(label: &str, size: &str, n_dims: Json, value: Json) -> Json {
    constant_doc(
        json!([pref("k")]),
        &[
            json!({ "@id": "http://example.org#A.con.k", "S231:label": label, "S231:isArray": true,
                    "S231:numberDimensions": n_dims, "S231:sizeOfDimensions": size,
                    "S231:value": value }),
        ],
    )
}

/// The preserved-**list** twin of the expression fixture: `nin = 2`, `k[nin] = [1, 1]`.
fn list_twin() -> Json {
    constant_doc(
        json!([pref("nin"), pref("k")]),
        &[
            json!({ "@id": "http://example.org#A.con.nin", "S231:label": "nin", "S231:value": 2 }),
            json!({ "@id": "http://example.org#A.con.k", "S231:label": "k[nin]",
                    "S231:isArray": true, "S231:numberDimensions": 1,
                    "S231:sizeOfDimensions": "(nin)", "S231:value": [1, 1] }),
        ],
    )
}

/// The **flattened** twin of the expression fixture: `nin = 2` plus scalar `k_1 = 1`, `k_2 = 1`.
fn flattened_twin() -> Json {
    constant_doc(
        json!([pref("nin"), pref("k_1"), pref("k_2")]),
        &[
            json!({ "@id": "http://example.org#A.con.nin", "S231:label": "nin", "S231:value": 2 }),
            json!({ "@id": "http://example.org#A.con.k_1", "S231:label": "k_1", "S231:value": 1 }),
            json!({ "@id": "http://example.org#A.con.k_2", "S231:label": "k_2", "S231:value": 1 }),
        ],
    )
}

// ---- headline: three-encoding convergence + golden + determinism ----------------------------

#[test]
fn expression_list_and_flattened_encodings_converge_byte_identical() {
    // doc-04 exit #5: preserved-list, flattened per-element scalars, and preserved-expression
    // (fill(1, nin)) declaring the same array parameter lower to ONE ModelGraph, byte-identical.
    let expression = render(&import_ok(EXPRESSION));
    let list = render(&import_json(&list_twin()).expect("list twin must resolve"));
    let flattened = render(&import_json(&flattened_twin()).expect("flattened twin must resolve"));
    assert_eq!(
        expression, list,
        "expression and list encodings must lower to a byte-identical ModelGraph"
    );
    assert_eq!(
        expression, flattened,
        "expression and flattened encodings must lower to a byte-identical ModelGraph"
    );
}

#[test]
fn golden_array_expression_modelgraph() {
    let actual = render(&import_ok(EXPRESSION));
    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(golden_path().parent().unwrap()).unwrap();
        std::fs::write(golden_path(), &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(golden_path())
        .expect("golden snapshot missing — regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "array-expression ModelGraph diverged from the golden"
    );
    // Both twins match the same golden (redundant cross-check of convergence).
    assert_eq!(
        render(&import_json(&list_twin()).expect("list twin must resolve")),
        expected
    );
    assert_eq!(
        render(&import_json(&flattened_twin()).expect("flattened twin must resolve")),
        expected
    );
}

#[test]
fn expression_import_is_byte_identical_across_two_imports() {
    assert_eq!(
        render(&import_ok(EXPRESSION)),
        render(&import_ok(EXPRESSION))
    );
}

// ---- expression variants: element values and types preserved --------------------------------

#[test]
fn fill_expression_grounds_elements_with_integer_type() {
    let g = import_ok(EXPRESSION);
    let vals = &g.blocks[0].params.values;
    let keys: Vec<&str> = vals.iter().map(|(n, _)| n.as_ref()).collect();
    // nin (scalar) then the two minted elements, in declaration + row-major order.
    assert_eq!(keys, vec!["nin", "k_1", "k_2"]);
    assert!(vals[0].1.bit_eq(&Value::Integer(2)), "nin == 2");
    assert!(vals[1].1.bit_eq(&Value::Integer(1)), "k_1 == fill value 1");
    assert!(vals[2].1.bit_eq(&Value::Integer(1)), "k_2 == fill value 1");
}

#[test]
fn brace_literal_expression_grounds_real_elements_by_bits() {
    let g = import_json(&expr_array_doc(
        "k[2]",
        "(2)",
        json!(1),
        json!("{1.0, 2.5}"),
    ))
    .expect("brace-literal expression must resolve");
    let vals = &g.blocks[0].params.values;
    let keys: Vec<&str> = vals.iter().map(|(n, _)| n.as_ref()).collect();
    assert_eq!(keys, vec!["k_1", "k_2"]);
    assert!(vals[0].1.bit_eq(&Value::Real(1.0)), "k_1 == 1.0 by bits");
    assert!(vals[1].1.bit_eq(&Value::Real(2.5)), "k_2 == 2.5 by bits");
}

#[test]
fn range_expression_grounds_successive_integers() {
    let g = import_json(&expr_array_doc("k[3]", "(3)", json!(1), json!("1:3")))
        .expect("range expression must resolve");
    let vals = &g.blocks[0].params.values;
    let keys: Vec<&str> = vals.iter().map(|(n, _)| n.as_ref()).collect();
    assert_eq!(keys, vec!["k_1", "k_2", "k_3"]);
    assert!(vals[0].1.bit_eq(&Value::Integer(1)));
    assert!(vals[1].1.bit_eq(&Value::Integer(2)));
    assert!(vals[2].1.bit_eq(&Value::Integer(3)));
}

#[test]
fn size_zero_declared_dims_accept_empty_range_expression() {
    // An empty range against a declared size-0 array is the legal 0 == 0 case: zero entries.
    let g = import_json(&expr_array_doc("k[0]", "(0)", json!(1), json!("1:0")))
        .expect("empty-range expression against a size-0 array must resolve");
    assert!(
        g.blocks[0].params.values.is_empty(),
        "size-0 array contributes no parameter entries"
    );
}

// ---- shape check: expected vs got, 2-D deferral, size-0 -------------------------------------

#[test]
fn shape_mismatch_reports_expected_and_actual_counts() {
    // Declared k[3] but the expression evaluates to 2 elements → both numbers in the diagnostic.
    assert_grounding_failed_with(
        &expr_array_doc("k[3]", "(3)", json!(1), json!("fill(1, 2)")),
        &["2 element(s)", "imply 3"],
    );
}

#[test]
fn size_zero_declared_dims_reject_nonempty_expression() {
    // No broadcast into a declared empty array (mirrors the list-path size-0 rule).
    assert_grounding_failed_with(
        &expr_array_doc("k[0]", "(0)", json!(1), json!("fill(1, 1)")),
        &["1 element(s)", "imply 0"],
    );
}

#[test]
fn two_dimensional_declared_dims_reject_any_expression_value() {
    // oce-expr cannot construct 2-D arrays and a flat 1-D result is never reshaped into a matrix,
    // so ANY expression value on a rank-2 declaration is the named deferral — even one whose
    // element count would match (fill(1, 4)), and even a brace literal.
    for expr in ["fill(1, 4)", "{1, 2, 3, 4}"] {
        assert_grounding_failed_with(
            &expr_array_doc("k[2,2]", "(2, 2)", json!(2), json!(expr)),
            &["2-dimensional", "deferred"],
        );
    }
}

// ---- wrong-shape values: scalar expr on array param, array expr on scalar param -------------

#[test]
fn scalar_evaluating_expression_is_rejected_not_broadcast() {
    // NO broadcast semantics on the expression path: a scalar result is an error — the author
    // writes fill(x, n) explicitly.
    for expr in ["7.0", "1 + 1"] {
        assert_grounding_failed_with(
            &expr_array_doc("k[2]", "(2)", json!(1), json!(expr)),
            &["must evaluate to an array"],
        );
    }
}

#[test]
fn array_evaluating_expression_on_scalar_param_is_grounding_failed() {
    // Regression guard for the ground.rs `Ok(_) => NonScalar` wildcard: an array-evaluating
    // expression on a SCALAR (non-isArray) parameter still fails typed, exactly as before
    // EvalResult::Array became a live variant.
    let doc = constant_doc(
        json!([pref("k")]),
        &[
            json!({ "@id": "http://example.org#A.con.k", "S231:label": "k",
                  "S231:value": "fill(1, 2)" }),
        ],
    );
    assert_grounding_failed_with(&doc, &["did not ground to a scalar"]);
}

#[test]
fn bare_scalar_literal_on_array_param_is_grounding_failed() {
    // A bare (non-list, non-string) literal on an isArray parameter is malformed.
    assert_grounding_failed_with(
        &expr_array_doc("k[2]", "(2)", json!(1), json!(3.0)),
        &["JSON list of element literals or an array expression"],
    );
}

// ---- collision, forward reference, hostile count --------------------------------------------

#[test]
fn expression_minted_element_collision_is_rejected() {
    // The expression path runs the same sibling collision gate as the list path: minted k_1
    // collides with a scalar sibling literally named k_1.
    let doc = constant_doc(
        json!([pref("k"), pref("k_1")]),
        &[
            json!({ "@id": "http://example.org#A.con.k", "S231:label": "k[2]",
                    "S231:isArray": true, "S231:numberDimensions": 1,
                    "S231:sizeOfDimensions": "(2)", "S231:value": "fill(1, 2)" }),
            json!({ "@id": "http://example.org#A.con.k_1", "S231:label": "k_1", "S231:value": 5 }),
        ],
    );
    assert_error_code(&doc, DiagCode::ArrayFlattenCollision);
}

#[test]
fn forward_reference_to_later_param_is_grounding_failed() {
    // The incremental scope only holds EARLIER-declared parameters (the same Step-7 limitation
    // scalar Expr bindings have): `fill(1, nlater)` with `nlater` declared after `k` fails typed.
    let doc = constant_doc(
        json!([pref("k"), pref("nlater")]),
        &[
            json!({ "@id": "http://example.org#A.con.k", "S231:label": "k[2]",
                    "S231:isArray": true, "S231:numberDimensions": 1,
                    "S231:sizeOfDimensions": "(2)", "S231:value": "fill(1, nlater)" }),
            json!({ "@id": "http://example.org#A.con.nlater", "S231:label": "nlater",
                    "S231:value": 2 }),
        ],
    );
    assert_grounding_failed_with(&doc, &["unknown identifier", "nlater"]);
}

#[test]
fn hostile_fill_count_dies_as_typed_diagnostic_not_oom() {
    // The two element-count caps compose: the declared dims here are small (so the resolver-side
    // MAX_ARRAY_ELEMENTS gate passes), and the expression-side 2^20 construction cap in oce-expr
    // rejects fill(1, 2e9) as ArrayTooLarge BEFORE any allocation — surfaced as GroundingFailed,
    // never a multi-GB OOM abort. (The declared-dims cap is pinned separately in resolve_array.rs
    // huge_array_dimension_is_rejected_not_allocated.)
    assert_grounding_failed_with(
        &expr_array_doc("k[2]", "(2)", json!(1), json!("fill(1, 2000000000)")),
        &["array too large", "2000000000"],
    );
}
