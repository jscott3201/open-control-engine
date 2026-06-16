//! Array #5 normalization tests (M1-PR-9; doc 04 §3.6.1). Headline: the **preserved** encoding
//! (`isArray`/`sizeOfDimensions`, `k[2]`) and the **flattened** encoding (scalar `k_1`/`k_2`) lower
//! to a **byte-identical** `ModelGraph` (exit #5), for both 1-D and **multi-dimensional** (`B[2,2]`)
//! arrays. Plus: per-rank goldens, determinism, 1-based-at-boundary, the element-value oracle,
//! row-major / last-index-fastest ordering (a column-major flip fails the 2-D golden), param-only
//! connector-neutrality, the `ArrayFlattenCollision` gate, and a panic-free edge corpus (malformed
//! dims, missing/negative dims, length mismatch, broadcast, symbolic dims, array-expr rejection,
//! size-0, size-0-with-value rejection, usize-overflow, and the `MAX_ARRAY_ELEMENTS` DoS ceiling).
//! Floats compared by IEEE-754 bits (`TESTING.md` pillar 2).
//!
//! Regenerate the goldens after an intentional change:
//! ```text
//! OCE_BLESS=1 cargo test -p oce-cxf --test resolve_array golden_array_modelgraph
//! OCE_BLESS=1 cargo test -p oce-cxf --test resolve_array golden_array2d_modelgraph
//! ```

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::DiagCode;
use oce_model::{ModelGraph, Value};
use serde_json::{Value as Json, json};

const PRESERVED: &str = include_str!("fixtures/array_preserved.jsonld");
const FLATTENED: &str = include_str!("fixtures/array_flattened.jsonld");
const PRESERVED_2D: &str = include_str!("fixtures/array2d_preserved.jsonld");
const FLATTENED_2D: &str = include_str!("fixtures/array2d_flattened.jsonld");
const COLLISION: &str = include_str!("fixtures/invalid/array_flatten_collision.jsonld");
const GOLDEN_REL: &str = "tests/fixtures/golden/array.modelgraph.txt";
const GOLDEN_2D_REL: &str = "tests/fixtures/golden/array2d.modelgraph.txt";

// ---- bit-exact deterministic render (mirrors resolve_golden.rs) -----------------------------

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
        .expect("array fixture must resolve without error");
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

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_REL)
}

fn golden_2d_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_2D_REL)
}

/// A typed `#double` literal value node.
fn td(lexical: &str) -> Json {
    json!({ "@value": lexical, "@type": "http://www.w3.org/2001/XMLSchema#double" })
}

/// A minimal valid composite carrying one array parameter `k` on a `Constant` block, with the array
/// node's `sizeOfDimensions`/`numberDimensions`/`value` parameterized (1-D decorated label `k[2]`).
/// Mutated by the edge tests. The expansion base derives from the `@id` (`…A.con.k`), so the
/// decorative `label` does not affect the result — see [`array_doc_nd`] for a label-explicit variant.
fn array_doc(size: &str, n_dims: Json, value: Json) -> Json {
    array_doc_nd("k[2]", size, n_dims, value)
}

/// Like [`array_doc`] but with an explicit decorated `label` (e.g. `"k[2,2]"` for a 2-D array), so a
/// multi-dimensional fixture reads honestly even though the expansion base comes from the `@id`.
fn array_doc_nd(label: &str, size: &str, n_dims: Json, value: Json) -> Json {
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": [
            { "@id": "http://example.org#A", "@type": "S231:Block", "S231:label": "A",
              "S231:containsBlock": [ { "@id": "http://example.org#A.con" } ],
              "S231:hasOutput": { "@id": "http://example.org#A.yOut" } },
            { "@id": "http://example.org#A.con",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:label": "con",
              "S231:hasParameter": { "@id": "http://example.org#A.con.k" },
              "S231:hasOutput": { "@id": "http://example.org#A.con.y" } },
            { "@id": "http://example.org#A.con.k", "S231:label": label, "S231:isArray": true,
              "S231:numberDimensions": n_dims, "S231:sizeOfDimensions": size, "S231:value": value },
            { "@id": "http://example.org#A.con.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" },
              "S231:isConnectedTo": { "@id": "http://example.org#A.yOut" } },
            { "@id": "http://example.org#A.yOut", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    })
}

// ---- headline: exit #5 convergence + golden -------------------------------------------------

#[test]
fn array_preserved_and_flattened_converge_byte_identical() {
    let preserved = render(&import_ok(PRESERVED));
    let flattened = render(&import_ok(FLATTENED));
    assert_eq!(
        preserved, flattened,
        "exit #5: preserved and flattened array encodings must lower to a byte-identical ModelGraph"
    );
}

#[test]
fn golden_array_modelgraph() {
    let actual = render(&import_ok(PRESERVED));
    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(golden_path().parent().unwrap()).unwrap();
        std::fs::write(golden_path(), &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(golden_path())
        .expect("golden snapshot missing — regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "array ModelGraph diverged from the golden"
    );
    // The flattened encoding matches the same golden (redundant cross-check of convergence).
    assert_eq!(render(&import_ok(FLATTENED)), expected);
}

// ---- determinism, 1-based, oracle, neutrality -----------------------------------------------

#[test]
fn array_import_is_deterministic() {
    assert_eq!(render(&import_ok(PRESERVED)), render(&import_ok(PRESERVED)));
    assert_eq!(render(&import_ok(FLATTENED)), render(&import_ok(FLATTENED)));
}

#[test]
fn array_keys_are_1_based_at_boundary() {
    let g = import_ok(PRESERVED);
    let keys: Vec<&str> = g.blocks[0]
        .params
        .values
        .iter()
        .map(|(n, _)| n.as_ref())
        .collect();
    assert_eq!(keys, vec!["k_1", "k_2"], "1-based row-major element keys");
    assert!(
        !keys
            .iter()
            .any(|k| *k == "k_0" || *k == "k" || *k == "k[2]"),
        "no 0-based, undecorated, or bracketed key may leak to the boundary"
    );
}

#[test]
fn array_element_values_oracle() {
    // The flattened fixture's hand-authored literals ARE the oracle for the preserved side.
    let g = import_ok(PRESERVED);
    let vals = &g.blocks[0].params.values;
    assert!(vals[0].1.bit_eq(&Value::Real(2.0)), "k_1 == 2.0 by bits");
    assert!(vals[1].1.bit_eq(&Value::Real(3.0)), "k_2 == 3.0 by bits");
}

#[test]
fn array_param_expansion_is_param_only_no_connector_renumber() {
    // Param-only invariant: array normalization mints NO connector, NO edge, NO external input —
    // so ConnectorIds/external_inputs/iri are untouched (arms the M2 array-connector remap contract).
    let g = import_ok(PRESERVED);
    assert_eq!(
        g.connectors.len(),
        1,
        "only con.y; the array param adds no connector"
    );
    assert!(g.connections.is_empty());
    assert!(g.external_inputs.is_empty());
    // Identical connector/connection/external structure to the flattened encoding.
    let f = import_ok(FLATTENED);
    assert_eq!(g.connectors.len(), f.connectors.len());
    assert_eq!(g.connections.len(), f.connections.len());
    assert_eq!(g.external_inputs.len(), f.external_inputs.len());
}

// ---- ArrayFlattenCollision gate -------------------------------------------------------------

#[test]
fn array_flatten_collision_is_rejected() {
    // Instance has both `k[2]` (mints k_1, k_2) and a scalar sibling literally `k_1` → collision.
    let doc: Json = serde_json::from_str(COLLISION).expect("fixture is valid JSON");
    assert_error_code(&doc, DiagCode::ArrayFlattenCollision);
}

// ---- panic-free edge corpus -----------------------------------------------------------------

#[test]
fn malformed_size_dims_is_malformed_document() {
    for bad in ["(", "(a,)", "(2.5)", "(-1)", "2)", "()"] {
        assert_error_code(
            &array_doc(bad, json!(1), json!([td("2.0"), td("3.0")])),
            DiagCode::MalformedDocument,
        );
    }
}

#[test]
fn number_dimensions_disagreement_is_malformed_document() {
    // sizeOfDimensions "(2)" is 1-D, but numberDimensions says 2.
    assert_error_code(
        &array_doc("(2)", json!(2), json!([td("2.0"), td("3.0")])),
        DiagCode::MalformedDocument,
    );
}

#[test]
fn value_list_length_mismatch_is_grounding_failed() {
    // dims imply N=2, but the value list has 3 elements (and 3 != 1 broadcast).
    assert_error_code(
        &array_doc("(2)", json!(1), json!([td("2.0"), td("3.0"), td("4.0")])),
        DiagCode::GroundingFailed,
    );
}

#[test]
fn single_value_broadcasts_to_all_elements() {
    // A one-element list broadcasts to all N (the structural fill(value, N) equivalent).
    let g = import_json(&array_doc("(2)", json!(1), json!([td("7.0")])))
        .expect("broadcast must resolve");
    let vals = &g.blocks[0].params.values;
    assert_eq!(vals.len(), 2);
    assert!(vals[0].1.bit_eq(&Value::Real(7.0)));
    assert!(vals[1].1.bit_eq(&Value::Real(7.0)));
}

#[test]
fn array_expression_value_is_grounding_failed_m2() {
    // An array EXPRESSION (fill/comprehension) arrives as an Expr string, not a List → M2-deferred.
    assert_error_code(
        &array_doc("(2)", json!(1), json!("fill(1, 2)")),
        DiagCode::GroundingFailed,
    );
}

#[test]
fn symbolic_dimension_resolves_against_earlier_param() {
    // sizeOfDimensions "(nin)" with `nin` a ground EARLIER Integer parameter (declaration order).
    let doc = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": [
            { "@id": "http://example.org#A", "@type": "S231:Block", "S231:label": "A",
              "S231:containsBlock": [ { "@id": "http://example.org#A.con" } ],
              "S231:hasOutput": { "@id": "http://example.org#A.yOut" } },
            { "@id": "http://example.org#A.con",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:label": "con",
              "S231:hasParameter": [
                { "@id": "http://example.org#A.con.nin" },
                { "@id": "http://example.org#A.con.k" }
              ],
              "S231:hasOutput": { "@id": "http://example.org#A.con.y" } },
            { "@id": "http://example.org#A.con.nin", "S231:label": "nin", "S231:value": 2 },
            { "@id": "http://example.org#A.con.k", "S231:label": "k[nin]", "S231:isArray": true,
              "S231:numberDimensions": 1, "S231:sizeOfDimensions": "(nin)",
              "S231:value": [ td("2.0"), td("3.0") ] },
            { "@id": "http://example.org#A.con.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" },
              "S231:isConnectedTo": { "@id": "http://example.org#A.yOut" } },
            { "@id": "http://example.org#A.yOut", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    });
    let g = import_json(&doc).expect("symbolic dimension must resolve");
    let keys: Vec<&str> = g.blocks[0]
        .params
        .values
        .iter()
        .map(|(n, _)| n.as_ref())
        .collect();
    // nin (scalar) then the two expanded elements, in declaration + row-major order.
    assert_eq!(keys, vec!["nin", "k_1", "k_2"]);
}

#[test]
fn symbolic_dimension_unbound_is_malformed_document() {
    // "(nin)" with no `nin` parameter in scope → MalformedDocument (never a panic).
    assert_error_code(
        &array_doc("(nin)", json!(1), json!([td("2.0"), td("3.0")])),
        DiagCode::MalformedDocument,
    );
}

#[test]
fn size_zero_array_expands_to_no_entries() {
    // A legal empty array (size 0) → zero param entries, no panic.
    let g = import_json(&array_doc("(0)", json!(1), json!([]))).expect("empty array resolves");
    assert!(
        g.blocks[0].params.values.is_empty(),
        "size-0 array contributes no parameter entries"
    );
}

// ---- multi-dimensional (rank >= 2): the row-major / last-index-fastest odometer ------------
// These prove the OQ-1 multi-dim contract (B[2,2] -> B_1_1,B_1_2,B_2_1,B_2_2) that the 1-D suite
// could not reach: a column-major flip or a reversed fastest-index would pass every 1-D test but
// fail here. Implementation under test: resolve.rs array_element_names odometer (carry/reset loop).

#[test]
fn array2d_preserved_and_flattened_converge_byte_identical() {
    let preserved = render(&import_ok(PRESERVED_2D));
    let flattened = render(&import_ok(FLATTENED_2D));
    assert_eq!(
        preserved, flattened,
        "exit #5 (2-D): preserved B[2,2] and its flattened k_1_1.. twin must lower byte-identically"
    );
}

#[test]
fn array2d_keys_are_row_major_last_index_fastest() {
    let g = import_ok(PRESERVED_2D);
    let keys: Vec<&str> = g.blocks[0]
        .params
        .values
        .iter()
        .map(|(n, _)| n.as_ref())
        .collect();
    // Pins last-index-fastest. Column-major would be k_1_1,k_2_1,k_1_2,k_2_2 and fail here.
    assert_eq!(keys, vec!["k_1_1", "k_1_2", "k_2_1", "k_2_2"]);
}

#[test]
fn array2d_element_values_land_row_major() {
    // Distinct per-cell values, so a row/column transpose in the odometer fails this oracle.
    let g = import_ok(PRESERVED_2D);
    let v = &g.blocks[0].params.values;
    assert!(v[0].1.bit_eq(&Value::Real(10.0)), "k_1_1 == 10.0 by bits");
    assert!(v[1].1.bit_eq(&Value::Real(20.0)), "k_1_2 == 20.0 by bits");
    assert!(v[2].1.bit_eq(&Value::Real(30.0)), "k_2_1 == 30.0 by bits");
    assert!(v[3].1.bit_eq(&Value::Real(40.0)), "k_2_2 == 40.0 by bits");
}

#[test]
fn golden_array2d_modelgraph() {
    let actual = render(&import_ok(PRESERVED_2D));
    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(golden_2d_path().parent().unwrap()).unwrap();
        std::fs::write(golden_2d_path(), &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(golden_2d_path())
        .expect("2-D golden snapshot missing — regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "2-D array ModelGraph diverged from the golden"
    );
    // The flattened encoding matches the same golden (redundant cross-check of convergence).
    assert_eq!(render(&import_ok(FLATTENED_2D)), expected);
}

#[test]
fn array2d_single_value_broadcasts_to_all_elements() {
    // One value against a 2x2 array broadcasts to all 4 cells, keyed row-major.
    let doc = array_doc_nd("k[2,2]", "(2, 2)", json!(2), json!([td("9.0")]));
    let g = import_json(&doc).expect("2-D broadcast must resolve");
    let v = &g.blocks[0].params.values;
    let keys: Vec<&str> = v.iter().map(|(n, _)| n.as_ref()).collect();
    assert_eq!(keys, vec!["k_1_1", "k_1_2", "k_2_1", "k_2_2"]);
    assert!(
        v.iter().all(|(_, val)| val.bit_eq(&Value::Real(9.0))),
        "every broadcast cell == 9.0 by bits"
    );
}

// ---- previously-uncovered error arms (each must map to a typed diagnostic, never a panic) ----

#[test]
fn array_missing_size_of_dimensions_is_malformed_document() {
    // isArray=true but NO sizeOfDimensions → structural fail-fast (cannot determine element count).
    let doc = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": [
            { "@id": "http://example.org#A", "@type": "S231:Block", "S231:label": "A",
              "S231:containsBlock": [ { "@id": "http://example.org#A.con" } ],
              "S231:hasOutput": { "@id": "http://example.org#A.yOut" } },
            { "@id": "http://example.org#A.con",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:label": "con",
              "S231:hasParameter": { "@id": "http://example.org#A.con.k" },
              "S231:hasOutput": { "@id": "http://example.org#A.con.y" } },
            { "@id": "http://example.org#A.con.k", "S231:label": "k[2]", "S231:isArray": true,
              "S231:numberDimensions": 1, "S231:value": [ td("2.0"), td("3.0") ] },
            { "@id": "http://example.org#A.con.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" },
              "S231:isConnectedTo": { "@id": "http://example.org#A.yOut" } },
            { "@id": "http://example.org#A.yOut", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    });
    assert_error_code(&doc, DiagCode::MalformedDocument);
}

#[test]
fn negative_number_dimensions_is_malformed_document() {
    // numberDimensions < 0 — distinct from the arity-disagreement sub-arm (which uses 2 vs 1-D).
    assert_error_code(
        &array_doc("(2)", json!(-1), json!([td("2.0"), td("3.0")])),
        DiagCode::MalformedDocument,
    );
}

#[test]
fn element_count_overflow_is_malformed_document() {
    // Two near-i64::MAX dimensions whose product overflows usize → MalformedDocument, never a panic
    // (checked_mul guard). Fails in array_element_names before the value list is ever consulted.
    assert_error_code(
        &array_doc_nd(
            "k[*,*]",
            "(9000000000000000000, 9000000000000000000)",
            json!(2),
            json!([td("1.0")]),
        ),
        DiagCode::MalformedDocument,
    );
}

#[test]
fn huge_array_dimension_is_rejected_not_allocated() {
    // A large-but-non-overflowing dimension (2e9) exceeds MAX_ARRAY_ELEMENTS → MalformedDocument.
    // The ceiling is checked BEFORE Vec::with_capacity, so this resolves quickly instead of an
    // uncatchable multi-GB OOM abort (the DoS guard; M1 exit #6 panic-free posture).
    assert_error_code(
        &array_doc("(2000000000)", json!(1), json!([td("1.0")])),
        DiagCode::MalformedDocument,
    );
}

#[test]
fn size_zero_array_with_value_is_grounding_failed() {
    // A value supplied for a declared empty (size-0) array must NOT be silently dropped — broadcast
    // (m==1) does not apply when N==0. Both a well-formed and a malformed value are rejected, so a
    // malformed literal can never be silently accepted as valid.
    assert_error_code(
        &array_doc("(0)", json!(1), json!([td("7.0")])),
        DiagCode::GroundingFailed,
    );
    assert_error_code(
        &array_doc("(0)", json!(1), json!([td("not-a-number")])),
        DiagCode::GroundingFailed,
    );
}
