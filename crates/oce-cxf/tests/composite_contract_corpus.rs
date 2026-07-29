//! Conformance-corpus drivers for the published composite-subset contract
//! (`docs/cxf-composite-subset.md`). The corpus under
//! `tests/fixtures/composite_contract/` is the SAME set of files an external CXF emitter tests
//! its output against, so every pin here is a published promise: accepted fixtures are held to
//! blessed byte-exact `.modelgraph.txt` goldens (stored in the shared golden tree under a
//! `composite_contract_` prefix), rejected fixtures are held to their full
//! (DiagCode, subject, message) triple per file, and the corpus `README.md` index is held equal
//! to the deterministically sorted `*.jsonld` listing so no fixture ships unindexed.
//!
//! Regenerate the accepted goldens after an intentional lowering change:
//! ```text
//! OCE_BLESS=1 cargo test -p oce-cxf --test composite_contract_corpus
//! ```

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{ModelGraph, Value};

/// Corpus root, resolved from the crate manifest so the tests run from any working directory.
fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/composite_contract")
}

fn read_fixture(rel: &str) -> String {
    let path = corpus_dir().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("corpus fixture {} must be readable: {e}", path.display()))
}

/// Accepted fixtures paired with their golden snapshots in the shared golden tree.
const ACCEPTED: [(&str, &str); 3] = [
    (
        "accepted/minimal_nested.jsonld",
        "tests/fixtures/golden/composite_contract_minimal_nested.modelgraph.txt",
    ),
    (
        "accepted/two_level_nesting.jsonld",
        "tests/fixtures/golden/composite_contract_two_level_nesting.modelgraph.txt",
    ),
    (
        "accepted/registered_leaf_carveout.jsonld",
        "tests/fixtures/golden/composite_contract_registered_leaf_carveout.modelgraph.txt",
    ),
];

// ---- bit-exact deterministic render (mirrors resolve_composite.rs / resolve_golden.rs) -------

fn render(graph: &ModelGraph) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "blocks: {}", graph.blocks.len());
    for block in &graph.blocks {
        let _ = writeln!(
            out,
            "  B{} decl={} class={} instance_iri={:?}",
            block.id.0,
            block.decl_order,
            block.class_iri,
            block.instance_iri.as_deref()
        );
        let _ = writeln!(
            out,
            "    inputs={:?} outputs={:?}",
            block.inputs.iter().map(|id| id.0).collect::<Vec<_>>(),
            block.outputs.iter().map(|id| id.0).collect::<Vec<_>>()
        );
        for (name, value) in &block.params.values {
            let _ = writeln!(out, "    param {name}={}", render_value(value));
        }
    }
    let _ = writeln!(out, "connectors: {}", graph.connectors.len());
    for connector in &graph.connectors {
        let _ = writeln!(
            out,
            "  C{} block=B{} dir={:?} type={:?} decl={} iri={:?}",
            connector.id.0,
            connector.block.0,
            connector.dir,
            connector.value_type,
            connector.decl_order,
            connector.iri.as_deref()
        );
    }
    let _ = writeln!(out, "connections: {}", graph.connections.len());
    for connection in &graph.connections {
        let _ = writeln!(out, "  C{} -> C{}", connection.from.0, connection.to.0);
    }
    let _ = writeln!(
        out,
        "external_inputs: {:?}",
        graph
            .external_inputs
            .iter()
            .map(|id| id.0)
            .collect::<Vec<_>>()
    );
    out
}

fn render_value(value: &Value) -> String {
    match value {
        Value::Real(x) => format!("Real(0x{:016x})", x.to_bits()),
        Value::Integer(x) => format!("Integer({x})"),
        Value::Boolean(x) => format!("Boolean({x})"),
        Value::String(x) => format!("String({x:?})"),
        Value::Enum { class, ordinal } => format!("Enum(class={},ordinal={ordinal})", class.0),
    }
}

// ---- import helpers ---------------------------------------------------------------------------

/// Import an accepted-corpus document and enforce accept-precondition (iv): warning-free.
fn import_ok(src: &str) -> ModelGraph {
    let (graph, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("accepted corpus fixture must import");
    assert!(
        report.is_empty(),
        "accepted corpus fixtures must import warning-free: {:?}",
        report.diagnostics
    );
    graph
}

/// Import a rejected-corpus document and return the full, finalize-sorted rejection vector.
fn reject(src: &str) -> Vec<Diagnostic> {
    match import_cxf(src.as_bytes(), &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => diags,
        other => panic!("expected a validation rejection, got {other:?}"),
    }
}

fn error_with_subject(code: DiagCode, subject: &str, message: &str) -> Diagnostic {
    Diagnostic::error(code, message).with_subject(subject.to_owned())
}

/// The published pin per rejected fixture file: the exact, complete diagnostic vector its
/// import returns. Subjects are present only where the contract defines one — the pure-cycle
/// zero-root rejection deliberately carries none.
fn expected_rejections() -> Vec<(&'static str, Vec<Diagnostic>)> {
    vec![
        (
            "rejected/multi_root.jsonld",
            vec![error_with_subject(
                DiagCode::MalformedDocument,
                "http://example.org#M",
                "composite/root-count: expected exactly one top composite root after nested \
                 classification, found 2 candidate roots: http://example.org#M, \
                 http://example.org#M2",
            )],
        ),
        (
            "rejected/pure_cycle.jsonld",
            vec![Diagnostic::error(
                DiagCode::MalformedDocument,
                "composite/root-count: expected exactly one top composite root after nested \
                 classification, found zero candidate roots",
            )],
        ),
        (
            "rejected/reachable_cycle.jsonld",
            vec![error_with_subject(
                DiagCode::MalformedDocument,
                "http://example.org#A",
                "composite/contains-cycle: cycle in nested composite containsBlock graph: \
                 http://example.org#A -> http://example.org#B -> http://example.org#C -> \
                 http://example.org#A",
            )],
        ),
        (
            "rejected/self_loop.jsonld",
            vec![error_with_subject(
                DiagCode::MalformedDocument,
                "http://example.org#A",
                "composite/contains-cycle: cycle in nested composite containsBlock graph: \
                 http://example.org#A -> http://example.org#A",
            )],
        ),
        (
            // ONE structural cycle {A, C} reachable via TWO paths (root→A and root→B→C):
            // the k-re-entry contract — one truthful path-ordered diagnostic per re-entry,
            // post-finalize_diags subject order.
            "rejected/diamond_cycle.jsonld",
            vec![
                error_with_subject(
                    DiagCode::MalformedDocument,
                    "http://example.org#A",
                    "composite/contains-cycle: cycle in nested composite containsBlock graph: \
                     http://example.org#A -> http://example.org#C -> http://example.org#A",
                ),
                error_with_subject(
                    DiagCode::MalformedDocument,
                    "http://example.org#C",
                    "composite/contains-cycle: cycle in nested composite containsBlock graph: \
                     http://example.org#C -> http://example.org#A -> http://example.org#C",
                ),
            ],
        ),
        (
            "rejected/banned_key_bare.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.c2",
                "composite/banned-modelica-key: unsupported Modelica construct `redeclare` \
                 survived CXF lowering",
            )],
        ),
        (
            "rejected/banned_key_prefixed.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.c2",
                "composite/banned-modelica-key: unsupported Modelica construct \
                 `S231:extendsFrom` survived CXF lowering",
            )],
        ),
        (
            "rejected/banned_key_absolute_iri.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.c2",
                "composite/banned-modelica-key: unsupported Modelica construct \
                 `http://data.ashrae.org/S231P#moSource` survived CXF lowering",
            )],
        ),
        (
            "rejected/array_parameter.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.p",
                "composite/array-parameter: array-valued composite parameters are not supported \
                 by this CXF lowering subset",
            )],
        ),
        (
            "rejected/array_connector.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.c2.u",
                "composite/array-connector: array-valued connector nodes are not supported; \
                 flatten the array to one connector per element",
            )],
        ),
        (
            "rejected/array_instance.jsonld",
            vec![error_with_subject(
                DiagCode::NonSubsetConstruct,
                "http://example.org#M.c2",
                "composite/array-instance: array-valued block-instance nodes are not supported; \
                 flatten the array to one instance per element",
            )],
        ),
        (
            "rejected/replaceable.jsonld",
            vec![error_with_subject(
                DiagCode::UnresolvedPolymorphism,
                "http://example.org#M.c2",
                "composite/replaceable: replaceable CXF components must be resolved before \
                 import",
            )],
        ),
    ]
}

/// Deterministically sorted `<subdir>/<name>.jsonld` listing of one corpus subdirectory.
fn sorted_fixture_listing(subdir: &str) -> Vec<String> {
    let dir = corpus_dir().join(subdir);
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("corpus subdir {} must exist: {e}", dir.display()))
        .map(|entry| entry.expect("readable directory entry").file_name())
        .filter_map(|name| {
            let name = name.to_str().expect("UTF-8 fixture name").to_owned();
            name.ends_with(".jsonld")
                .then(|| format!("{subdir}/{name}"))
        })
        .collect();
    names.sort();
    names
}

// ---- accepted corpus --------------------------------------------------------------------------

#[test]
fn accepted_fixtures_match_their_blessed_modelgraph_goldens_byte_exactly() {
    for (fixture, golden_rel) in ACCEPTED {
        let actual = render(&import_ok(&read_fixture(fixture)));
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(golden_rel);
        if std::env::var_os("OCE_BLESS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, &actual).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&path)
            .expect("golden snapshot missing; regenerate with OCE_BLESS=1");
        assert_eq!(
            actual, expected,
            "{fixture} diverged from its blessed golden {golden_rel}"
        );
    }
}

#[test]
fn accepted_fixture_imports_are_byte_identical_across_repeated_imports() {
    for (fixture, _) in ACCEPTED {
        let src = read_fixture(fixture);
        assert_eq!(
            render(&import_ok(&src)),
            render(&import_ok(&src)),
            "{fixture} must lower deterministically"
        );
    }
}

#[test]
fn parameter_scope_chain_grounds_sibling_then_parent_references_into_leaves() {
    // minimal_nested: kBase=0.25, sibling kTop = kBase + 0.25, child constant kInner = kTop,
    // leaf gain.k = kInner ⇒ 0.5 exactly. two_level_nesting adds one more hop with an
    // arithmetic step: kRoot=3.0 → kOuter=kRoot → kInner=kOuter+1.0 → gain.k = 4.0.
    let cases = [
        ("accepted/minimal_nested.jsonld", ".sub.gain", 0.5f64),
        (
            "accepted/two_level_nesting.jsonld",
            ".outer.inner.gain",
            4.0f64,
        ),
    ];
    for (fixture, gain_suffix, expected_k) in cases {
        let graph = import_ok(&read_fixture(fixture));
        let gain = graph
            .blocks
            .iter()
            .find(|block| {
                block
                    .instance_iri
                    .as_deref()
                    .is_some_and(|iri| iri.ends_with(gain_suffix))
            })
            .unwrap_or_else(|| panic!("{fixture}: missing gain leaf {gain_suffix}"));
        let k = gain
            .params
            .values
            .iter()
            .find_map(|(name, value)| (name.as_ref() == "k").then_some(value))
            .unwrap_or_else(|| panic!("{fixture}: gain leaf must carry param k"));
        assert!(
            k.bit_eq(&Value::Real(expected_k)),
            "{fixture}: inherited gain.k must ground to {expected_k}, got {k:?}"
        );
    }
}

#[test]
fn registered_leaf_with_contains_block_stays_a_leaf_and_its_protected_child_is_elided() {
    // Rule 1 carve-out through the corpus file: `con` is a registered Constant that carries
    // containsBlock, so it imports as a normal block; the protected child never becomes a
    // block, a connector, or an external input.
    let graph = import_ok(&read_fixture("accepted/registered_leaf_carveout.jsonld"));
    let instances: Vec<&str> = graph
        .blocks
        .iter()
        .map(|block| block.instance_iri.as_deref().expect("source path"))
        .collect();
    assert_eq!(
        instances,
        vec![
            "http://example.org#cc.registered_leaf_carveout.con",
            "http://example.org#cc.registered_leaf_carveout.post",
        ],
        "the carve-out leaf and its sibling are the only runtime blocks"
    );
    assert!(
        graph.blocks[0].class_iri.ends_with("Sources.Constant"),
        "the carve-out leaf keeps its registered class"
    );
    assert!(
        graph.external_inputs.is_empty(),
        "a source-driven model exposes no external inputs"
    );
}

// ---- rejected corpus --------------------------------------------------------------------------

#[test]
fn rejected_fixtures_pin_their_exact_diagnostic_triples_through_the_files() {
    for (fixture, expected) in expected_rejections() {
        assert_eq!(
            reject(&read_fixture(fixture)),
            expected,
            "{fixture} must reject with exactly its published (code, subject, message) pins"
        );
    }
}

#[test]
fn rejected_fixture_rejections_are_byte_identical_across_repeated_imports() {
    for (fixture, _) in expected_rejections() {
        let src = read_fixture(fixture);
        assert_eq!(
            reject(&src),
            reject(&src),
            "{fixture} must reject deterministically"
        );
    }
}

#[test]
fn every_corpus_fixture_file_is_driven_by_a_pin_and_every_pin_has_a_file() {
    let mut pinned: Vec<String> = expected_rejections()
        .iter()
        .map(|(fixture, _)| (*fixture).to_owned())
        .collect();
    pinned.sort();
    assert_eq!(
        sorted_fixture_listing("rejected"),
        pinned,
        "the rejected corpus on disk and the pinned expectation table must stay one-to-one"
    );

    let mut accepted: Vec<String> = ACCEPTED
        .iter()
        .map(|(fixture, _)| (*fixture).to_owned())
        .collect();
    accepted.sort();
    assert_eq!(
        sorted_fixture_listing("accepted"),
        accepted,
        "the accepted corpus on disk and the golden table must stay one-to-one"
    );
}

// ---- README index -----------------------------------------------------------------------------

#[test]
fn readme_index_lists_exactly_the_sorted_jsonld_corpus() {
    // Index scope contract (documented in the corpus README): the index covers `*.jsonld`
    // fixture files only — goldens live in the shared golden tree, outside the corpus dirs.
    // Every backtick-quoted token of the shape `accepted/<name>.jsonld` or
    // `rejected/<name>.jsonld` is an index row; the sorted token set must equal the sorted
    // on-disk listing, with no duplicate rows.
    let readme = read_fixture("README.md");
    let mut indexed: Vec<String> = readme
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|token| {
            token.ends_with(".jsonld")
                && (token.starts_with("accepted/") || token.starts_with("rejected/"))
        })
        .map(str::to_owned)
        .collect();
    indexed.sort();
    let duplicates: Vec<&String> = indexed
        .windows(2)
        .filter_map(|w| (w[0] == w[1]).then_some(&w[0]))
        .collect();
    assert!(
        duplicates.is_empty(),
        "README must index each fixture exactly once, duplicated: {duplicates:?}"
    );
    let mut on_disk = sorted_fixture_listing("accepted");
    on_disk.extend(sorted_fixture_listing("rejected"));
    on_disk.sort();
    assert_eq!(
        indexed, on_disk,
        "the README index and the on-disk *.jsonld corpus must stay one-to-one"
    );
}
