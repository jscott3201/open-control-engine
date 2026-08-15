//! Conformance-corpus drivers for the published composite-subset contract
//! (`docs/cxf-composite-subset.md`). The corpus under
//! `tests/fixtures/composite_contract/` is the SAME set of files an external CXF emitter tests
//! its output against, so every pin here is a published promise: accepted fixtures are held to
//! blessed byte-exact `.modelgraph.txt` goldens (stored in the shared golden tree under a
//! `composite_contract_` prefix), rejected fixtures are held to their full
//! (DiagCode, subject, message) triple per file, warned fixtures load with exactly their pinned
//! warning vector, and the corpus `README.md` index is held equal to the deterministically
//! sorted `*.jsonld` listing so no fixture ships unindexed.
//!
//! Regenerate the accepted goldens after an intentional lowering change:
//! ```text
//! OCE_BLESS=1 cargo test -p oce-cxf --test composite_contract_corpus
//! ```

mod bless;
mod composite_contract_member_pins;

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

/// Base accepted fixtures paired with their golden snapshots in the shared golden tree; the
/// drivers consume [`accepted_fixtures`], the one assembled table (base rows plus the
/// `hasInstance` member-interface slice rows).
const ACCEPTED: [(&str, &str); 7] = [
    (
        "accepted/forward_sibling_reference.jsonld",
        "tests/fixtures/golden/composite_contract_forward_sibling_reference.modelgraph.txt",
    ),
    (
        "accepted/string_literal_sibling_name.jsonld",
        "tests/fixtures/golden/composite_contract_string_literal_sibling_name.modelgraph.txt",
    ),
    (
        "accepted/leaf_array_parameter_conditional_member.jsonld",
        "tests/fixtures/golden/composite_contract_leaf_array_parameter_conditional_member.modelgraph.txt",
    ),
    (
        "accepted/leaf_identity_parameter_modification.jsonld",
        "tests/fixtures/golden/composite_contract_leaf_identity_parameter_modification.modelgraph.txt",
    ),
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

/// Every accepted fixture with its golden — the single assembled table every accepted-corpus
/// guard consumes.
fn accepted_fixtures() -> Vec<(String, String)> {
    ACCEPTED
        .iter()
        .map(|(fixture, golden)| ((*fixture).to_owned(), (*golden).to_owned()))
        .chain(composite_contract_member_pins::accepted())
        .collect()
}

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
    let _ = writeln!(out, "boundary_outputs: {}", graph.boundary_outputs.len());
    for output in &graph.boundary_outputs {
        let _ = writeln!(
            out,
            "  {} <- C{} attrs={}",
            output.iri,
            output.source.0,
            render_attrs(&output.attrs)
        );
    }
    out
}

/// Bit-exact rendering of a declared boundary output's §7.4.1 attrs — printed unconditionally,
/// so these goldens pin that no attribute leaks into the corpus fixtures (none authors any);
/// Real bounds by `to_bits`, never `==`/epsilon (TESTING.md pillar 2).
fn render_attrs(attrs: &oce_model::Attrs) -> String {
    use oce_model::Attrs;
    let bits = |bound: Option<f64>| {
        bound.map_or_else(|| "-".to_owned(), |x| format!("0x{:016x}", x.to_bits()))
    };
    match attrs {
        Attrs::Real(a) => format!(
            "Real(unit={:?} quantity={:?} display_unit={:?} min={} max={})",
            a.unit.as_deref(),
            a.quantity.as_deref(),
            a.display_unit.as_deref(),
            bits(a.min),
            bits(a.max),
        ),
        Attrs::Integer(a) => format!("Integer(min={:?} max={:?})", a.min, a.max),
        Attrs::Boolean(_) => "Boolean".to_owned(),
        Attrs::String(_) => "String".to_owned(),
        Attrs::Enum(_) => "Enum".to_owned(),
    }
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
    let mut rows = vec![
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
            // One reference cycle among the root's own declarations: one diagnostic per
            // distinct cycle, subject = the participant earliest in chained declaration order,
            // message naming every participant in chained order and closing on the first.
            "rejected/declaration_cycle.jsonld",
            vec![error_with_subject(
                DiagCode::MalformedDocument,
                "http://example.org#M.a",
                "composite/declaration-cycle: cycle in the block's own declaration references: \
                 http://example.org#M.a -> http://example.org#M.b -> http://example.org#M.a",
            )],
        ),
        (
            // Self-reference is a length-1 declaration cycle, never a read of the same-named
            // enclosing binding — the root's own x=1.0 grounds and the inner chain refuses.
            "rejected/self_reference.jsonld",
            vec![error_with_subject(
                DiagCode::MalformedDocument,
                "http://example.org#M.sub.x",
                "composite/declaration-cycle: cycle in the block's own declaration references: \
                 http://example.org#M.sub.x -> http://example.org#M.sub.x",
            )],
        ),
        (
            // One local name bound twice in one chain (parameter then constant): the occurrence
            // beyond the first refuses and names the first; the first stays a normal binding.
            "rejected/duplicate_declaration.jsonld",
            vec![error_with_subject(
                DiagCode::MalformedDocument,
                "http://example.org#M.settings.k",
                "composite/duplicate-declaration: own declaration \
                 http://example.org#M.settings.k re-binds local name `k` first declared at \
                 http://example.org#M.k",
            )],
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
        // Boundary-interface machinery (untagged — not composite-shape rules; see the doc's
        // "shared import machinery" note): a declared boundary output must not shadow an
        // existing connector identity, and must not be multiply driven.
        (
            "rejected/shadowed_output_child_connector.jsonld",
            vec![error_with_subject(
                DiagCode::BoundaryOutputShadowsConnector,
                "http://example.org#M.c1.y",
                "boundary output shadows an instance port connector",
            )],
        ),
        (
            "rejected/shadowed_output_input_output.jsonld",
            vec![error_with_subject(
                DiagCode::BoundaryOutputShadowsConnector,
                "http://example.org#M.io",
                "boundary output IRI is also a boundary input",
            )],
        ),
        (
            "rejected/multi_driven_boundary_output.jsonld",
            vec![error_with_subject(
                DiagCode::SingleAssignment,
                "http://example.org#M.y",
                "boundary output is multiply driven (distinct drivers 2)",
            )],
        ),
    ];
    rows.extend(composite_contract_member_pins::rejections());
    rows
}

/// The published pin per warned fixture file: the exact, complete warning vector its import
/// returns alongside a successful load.
fn expected_warnings() -> Vec<(&'static str, Vec<Diagnostic>)> {
    let mut rows = vec![(
        "warned/undriven_boundary_output.jsonld",
        vec![
            Diagnostic::warning(
                DiagCode::UndrivenBoundaryOutput,
                "declared boundary output has no internal driver",
            )
            .with_subject("http://example.org#M.y".to_owned()),
        ],
    )];
    rows.extend(composite_contract_member_pins::warnings());
    rows
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
    for (fixture, golden_rel) in accepted_fixtures() {
        let actual = render(&import_ok(&read_fixture(&fixture)));
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(&golden_rel);
        if bless::enabled() {
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
    for (fixture, _) in accepted_fixtures() {
        let src = read_fixture(&fixture);
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

// ---- warned corpus ----------------------------------------------------------------------------

#[test]
fn warned_fixtures_load_with_exactly_their_pinned_warning_vectors() {
    for (fixture, expected) in expected_warnings() {
        let (_, report) = import_cxf(read_fixture(fixture).as_bytes(), &ResolveOptions::default())
            .unwrap_or_else(|e| panic!("{fixture} must import despite its advisory: {e:?}"));
        assert_eq!(
            report.diagnostics, expected,
            "{fixture} must load with exactly its published warning vector"
        );
    }
}

#[test]
fn warned_fixture_reports_are_byte_identical_across_repeated_imports() {
    for (fixture, _) in expected_warnings() {
        let src = read_fixture(fixture);
        let report = |src: &str| {
            import_cxf(src.as_bytes(), &ResolveOptions::default())
                .expect("warned fixture imports")
                .1
                .diagnostics
        };
        assert_eq!(
            report(&src),
            report(&src),
            "{fixture} must warn deterministically"
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

    let mut accepted: Vec<String> = accepted_fixtures()
        .into_iter()
        .map(|(fixture, _)| fixture)
        .collect();
    accepted.sort();
    assert_eq!(
        sorted_fixture_listing("accepted"),
        accepted,
        "the accepted corpus on disk and the golden table must stay one-to-one"
    );

    let mut warned: Vec<String> = expected_warnings()
        .iter()
        .map(|(fixture, _)| (*fixture).to_owned())
        .collect();
    warned.sort();
    assert_eq!(
        sorted_fixture_listing("warned"),
        warned,
        "the warned corpus on disk and the pinned warning table must stay one-to-one"
    );
}

// ---- hasInstance member-interface scenario pins ------------------------------------------------

#[test]
fn permuting_member_arrays_leaves_the_modelgraph_byte_identical() {
    // R19-13: `hasInstance` array order is load-bearing for nothing — the permutation control
    // shares every `@id` with the mixed fixture and reorders every member array, and the two
    // renders (connectors AND param rows) must be byte-equal.
    assert_eq!(
        render(&import_ok(&read_fixture(
            "accepted/mixed_member_interface.jsonld"
        ))),
        render(&import_ok(&read_fixture(
            "accepted/member_array_permutation.jsonld"
        ))),
        "permuting hasInstance arrays must not move a ConnectorId, a decl_order, or a param row"
    );
}

#[test]
fn carveout_array_pair_rejects_identically_across_dialects() {
    // R19-11's reachability agreement: on a registered leaf's carve-out child — an active
    // containsBlock referent lowering never reaches — the array marker refuses under BOTH
    // spellings with byte-equal vectors. The unreferenced control loads clean, which is what
    // proves the scan is reference-based rather than position-based.
    assert_eq!(
        reject(&read_fixture(
            "rejected/carveout_member_array_hasinstance.jsonld"
        )),
        reject(&read_fixture(
            "rejected/carveout_member_array_hasinput.jsonld"
        )),
        "the carve-out pair's complete vectors must agree across dialects"
    );
}

#[test]
fn inactive_member_pair_rejects_identically_across_dialects() {
    // R19-3's inactive row: a listed member marked inactive by a pruned sibling's authored
    // reference derives no connector and enters neither ConnectorId block, so the hasInstance
    // spelling takes exactly the arity refusal the authored spelling takes — deriving the
    // member, or dropping it without withdrawing it from block 1, breaks this equality with a
    // second diagnostic ("connector owned by no instance").
    assert_eq!(
        reject(&read_fixture("rejected/inactive_member_hasinstance.jsonld")),
        reject(&read_fixture("rejected/inactive_member_hasinput.jsonld")),
        "the inactive-member pair's complete vectors must agree across dialects"
    );
}

#[test]
fn inactive_leaf_parameter_routes_ground_byte_identically() {
    // R19-3 mirrors the existing leaf-parameter path: activity filters neither an ordinary
    // `hasParameter` declaration nor a classified `hasInstance` parameter member. The fixtures
    // share every identity and differ only in the route that supplies `k`.
    assert_eq!(
        render(&import_ok(&read_fixture(
            "accepted/inactive_parameter_declaration_grounding.jsonld"
        ))),
        render(&import_ok(&read_fixture(
            "accepted/inactive_parameter_member_grounding.jsonld"
        ))),
        "ordinary and classified inactive leaf parameters must ground identically"
    );
}

#[test]
fn unclassifiable_member_control_changes_only_one_nodeless_member_name() {
    const ACCEPTED_MEMBER: &str = "http://example.org#M.sin.y";
    const REJECTED_MEMBER: &str = "http://example.org#M.sin.zzz";

    let accepted = read_fixture("accepted/unclassifiable_member_control.jsonld");
    let rejected = read_fixture("rejected/unsupported_instance_member.jsonld");
    let shape = |source: &str| {
        let document: serde_json::Value = serde_json::from_str(source).expect("valid fixture JSON");
        let graph = document["@graph"].as_array().expect("fixture graph");
        let context = document["@context"].as_object().expect("fixture context");
        let graph_ids: std::collections::BTreeSet<String> = graph
            .iter()
            .map(|node| node["@id"].as_str().expect("graph node id"))
            .map(|id| {
                if let Some(expanded) = context.get(id).and_then(serde_json::Value::as_str) {
                    return expanded.to_owned();
                }
                let Some((prefix, suffix)) = id.split_once(':') else {
                    return id.to_owned();
                };
                context
                    .get(prefix)
                    .and_then(serde_json::Value::as_str)
                    .map_or_else(|| id.to_owned(), |base| format!("{base}{suffix}"))
            })
            .collect();
        let member_count = graph
            .iter()
            .find(|node| node["@id"] == "http://example.org#M.sin")
            .expect("Sin instance")["S231:hasInstance"]
            .as_array()
            .expect("member array")
            .len();
        (member_count, graph_ids)
    };

    let (accepted_count, accepted_nodes) = shape(&accepted);
    let (rejected_count, rejected_nodes) = shape(&rejected);
    assert_eq!(accepted_count, 6, "accepted member count");
    assert_eq!(rejected_count, 6, "rejected member count");
    assert_eq!(
        accepted_nodes
            .iter()
            .filter(|id| id.starts_with("http://example.org#M.sin."))
            .count(),
        5,
        "accepted node-bearing member count"
    );
    assert_eq!(
        rejected_nodes
            .iter()
            .filter(|id| id.starts_with("http://example.org#M.sin."))
            .count(),
        5,
        "rejected node-bearing member count"
    );
    for member in [ACCEPTED_MEMBER, REJECTED_MEMBER] {
        assert!(!accepted_nodes.contains(member), "accepted node {member}");
        assert!(!rejected_nodes.contains(member), "rejected node {member}");
    }
    assert_eq!(accepted.matches(ACCEPTED_MEMBER).count(), 1);
    assert_eq!(rejected.matches(REJECTED_MEMBER).count(), 1);
    assert_eq!(
        accepted.replacen(ACCEPTED_MEMBER, REJECTED_MEMBER, 1),
        rejected,
        "the control pair must differ only by the final node-less member name"
    );
}

// ---- README index -----------------------------------------------------------------------------

#[test]
fn readme_index_lists_exactly_the_sorted_jsonld_corpus() {
    // Index scope contract (documented in the corpus README): the index covers `*.jsonld`
    // fixture files only — goldens live in the shared golden tree, outside the corpus dirs.
    // Every backtick-quoted token of the shape `accepted/<name>.jsonld`,
    // `warned/<name>.jsonld`, or `rejected/<name>.jsonld` is an index row; the sorted token set
    // must equal the sorted on-disk listing, with no duplicate rows.
    let readme = read_fixture("README.md");
    let mut indexed: Vec<String> = readme
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|token| {
            token.ends_with(".jsonld")
                && (token.starts_with("accepted/")
                    || token.starts_with("warned/")
                    || token.starts_with("rejected/"))
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
    on_disk.extend(sorted_fixture_listing("warned"));
    on_disk.extend(sorted_fixture_listing("rejected"));
    on_disk.sort();
    assert_eq!(
        indexed, on_disk,
        "the README index and the on-disk *.jsonld corpus must stay one-to-one"
    );
}
