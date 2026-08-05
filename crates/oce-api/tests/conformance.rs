//! Conformance harness for the hand-authored fixture corpus. Each fixture is driven through the
//! **full** `Engine::load_cxf` pipeline (CXF resolve → flatten → validate → BUILD) to assert the
//! end-to-end accept/reject/warn contract and full-pipeline determinism (`TESTING.md` pillars
//! 1, 2, and 4).
//!
//! This complements the resolver-layer unit tests (`oce-cxf` `resolve_errors`/`resolve_golden`) and
//! the `oce-validate` §7.10 unit tests: the §7.10 *fixtures* (`unit_mismatch`/`one_sided_unit`/
//! `display_unit_divergence`) only exercise the unification path **through the whole pipeline**, so
//! they are driven here. Determinism (#4) is re-affirmed end-to-end: identical input ⇒ bit-identical
//! trace and byte-identical diagnostic stream.

use oce_api::{Engine, LoadReport, OcError, PointDirection};
use oce_cxf::CxfError;
use oce_diag::DiagCode;

const MINIMAL_LOOP: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const UNIT_MISMATCH: &str =
    include_str!("../../oce-cxf/tests/fixtures/invalid/unit_mismatch.jsonld");
const ONE_SIDED: &str = include_str!("../../oce-cxf/tests/fixtures/invalid/one_sided_unit.jsonld");
const DISPLAY_DIV: &str =
    include_str!("../../oce-cxf/tests/fixtures/invalid/display_unit_divergence.jsonld");
const DOUBLE_DRIVEN: &str =
    include_str!("../../oce-cxf/tests/fixtures/invalid/double_driven.jsonld");
const NON_SUBSET: &str = include_str!("../../oce-cxf/tests/fixtures/invalid/non_subset.jsonld");
const BOUND_MISMATCH: &str =
    include_str!("../../oce-cxf/tests/fixtures/invalid/bound_mismatch.jsonld");
const TWO_UNDRIVEN: &str = include_str!("../../oce-cxf/tests/fixtures/invalid/two_undriven.jsonld");

fn load(src: &str) -> Result<LoadReport, OcError> {
    let mut eng = Engine::in_memory();
    eng.load_cxf(src.as_bytes())
}

/// The error-severity `DiagCode`s an `OcError` carries — across both the resolver (`OcError::Cxf`)
/// and the deep-gate (`OcError::Validate`) seams, the two paths a load rejection can take.
fn error_codes(e: &OcError) -> Vec<DiagCode> {
    let diags = match e {
        OcError::Cxf(CxfError::Validation(d)) => d.as_slice(),
        OcError::Validate(ve) => ve.diagnostics.as_slice(),
        _ => &[],
    };
    diags
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.code)
        .collect()
}

/// The **full** ordered error signature — `(code, subject, message)` per error diagnostic, in the
/// order the pipeline returns them. Comparing this (not just the code set) is what actually pins the
/// determinism contract: a reordering, a changed subject, or a changed message all show up here.
fn error_signature(e: &OcError) -> Vec<(DiagCode, Option<String>, String)> {
    let diags = match e {
        OcError::Cxf(CxfError::Validation(d)) => d.as_slice(),
        OcError::Validate(ve) => ve.diagnostics.as_slice(),
        _ => &[],
    };
    diags
        .iter()
        .filter(|d| d.is_error())
        .map(|d| {
            (
                d.code,
                d.subject.as_deref().map(str::to_owned),
                d.message.clone(),
            )
        })
        .collect()
}

#[track_caller]
fn assert_rejected_with(src: &str, code: DiagCode) {
    match load(src) {
        Err(e) => {
            let codes = error_codes(&e);
            assert!(
                codes.contains(&code),
                "expected rejection with {code:?}, got error {e:?} (error codes: {codes:?})"
            );
        }
        Ok(r) => panic!("expected rejection with {code:?}, but the load succeeded: {r:?}"),
    }
}

// ---- valid acceptance --------------------------------------------------------------------------

#[test]
fn valid_minimal_loop_loads_clean() {
    let r = load(MINIMAL_LOOP).expect("the canonical minimal_loop must load end-to-end");
    assert!(r.block_count >= 1, "a non-trivial model was built");
    // A clean load carries only should-level (warning/info) diagnostics, never an error.
    assert!(
        r.warnings.iter().all(|d| !d.is_error()),
        "a successful load must surface no error-severity diagnostics: {:?}",
        r.warnings
    );
}

// ---- rejection corpus, end-to-end through the full pipeline ------------------------------------

#[test]
fn unit_mismatch_is_rejected_end_to_end() {
    // con.y(unit K) → add.u1(unit degC): both declared and divergent ⇒ §7.10 R13.1 hard error.
    // Fires only in oce-validate, reached through the whole load_cxf pipeline.
    assert_rejected_with(UNIT_MISMATCH, DiagCode::UnitQuantityMismatch);
}

#[test]
fn double_driven_input_is_rejected_end_to_end() {
    // An input with in-degree 2 ⇒ single-assignment violation (resolver structural fail-fast).
    assert_rejected_with(DOUBLE_DRIVEN, DiagCode::SingleAssignment);
}

#[test]
fn non_subset_class_is_rejected_end_to_end() {
    // An unregistered CDL class ⇒ ClassNotFound, never a panic.
    assert_rejected_with(NON_SUBSET, DiagCode::ClassNotFound);
}

#[test]
fn bound_mismatch_is_rejected_end_to_end() {
    // con.y declares S231:min 0.0; the driven peer add.u1 declares S231:min 5.0 — two distinct
    // declared bounds in one connected cluster ⇒ §7.10 R13.1 BoundMismatch. This is the bound twin
    // of unit_mismatch and proves the resolver's connector-bound grounding is load-bearing
    // end-to-end: without the min flowing onto Connector.attrs, this would (wrongly) load clean.
    assert_rejected_with(BOUND_MISMATCH, DiagCode::BoundMismatch);
}

// ---- propagation + warn-only -------------------------------------------------------------------

#[test]
fn one_sided_unit_propagates_to_peer() {
    // R13.2: con.y declares unit "K"; the driven peer input add.u1 declares none ⇒ load succeeds and
    // the peer INHERITS "K". Observed through the IO inventory (path-independent: an input now carries
    // the propagated unit, which no fixture input declared directly).
    let mut eng = Engine::in_memory();
    eng.load_cxf(ONE_SIDED.as_bytes())
        .expect("one-sided unit is R13.2 propagation, never an error");
    let propagated = eng
        .io()
        .iter()
        .filter(|p| p.direction == PointDirection::In && p.unit.as_deref() == Some("K"))
        .count();
    assert!(
        propagated >= 1,
        "R13.2 must propagate unit 'K' to the undeclared peer input; inventory: {:?}",
        eng.io().to_vec()
    );
}

#[test]
fn display_unit_divergence_is_warn_only() {
    // R13.3: units agree (K == K) but displayUnits diverge (degC vs K) ⇒ a should-Warning, NOT an
    // error. The load succeeds and surfaces a DisplayUnitDivergence warning.
    let r = load(DISPLAY_DIV).expect("displayUnit divergence is should-warn, never a hard error");
    assert!(
        r.warnings
            .iter()
            .any(|d| d.code == DiagCode::DisplayUnitDivergence && !d.is_error()),
        "expected a DisplayUnitDivergence warning, got {:?}",
        r.warnings
    );
}

// ---- determinism, end-to-end -------------------------------------------------------------------

#[test]
fn rejection_is_deterministic() {
    // The same invalid document rejects with the identical FULL error signature twice — (code,
    // subject, message) per diagnostic, in order — not merely the same code set (no HashMap-iteration
    // dependence anywhere in the rejection path: code, subject, or message).
    let e1 = load(UNIT_MISMATCH).expect_err("must reject");
    let e2 = load(UNIT_MISMATCH).expect_err("must reject");
    assert_eq!(error_signature(&e1), error_signature(&e2));
}

#[test]
fn multi_diagnostic_rejection_order_is_deterministic() {
    // A document that emits MORE THAN ONE diagnostic on a single load — two undriven inputs ⇒ two
    // SingleAssignment errors — is the only shape that can actually exercise diagnostic ORDER (a
    // single-diagnostic stream cannot). The full ordered (code, subject, message) signature must be
    // byte-identical across loads, and must genuinely carry ≥2 errors (guards against the assertion
    // going vacuous if the fixture ever stops emitting both).
    let s1 = error_signature(&load(TWO_UNDRIVEN).expect_err("two undriven inputs must reject"));
    let s2 = error_signature(&load(TWO_UNDRIVEN).expect_err("must reject"));
    assert!(
        s1.iter()
            .filter(|(c, ..)| *c == DiagCode::SingleAssignment)
            .count()
            >= 2,
        "fixture must surface ≥2 SingleAssignment errors to exercise ordering, got {s1:?}"
    );
    assert_eq!(
        s1, s2,
        "the multi-diagnostic error stream must be deterministically ordered"
    );
}

#[test]
fn warning_stream_is_byte_identical_across_loads() {
    // The should-warning stream is byte-identical across loads — diagnostic order is pinned
    // (sorted), a load-bearing piece of the exit #4 determinism contract.
    let r1 = load(DISPLAY_DIV).expect("loads");
    let r2 = load(DISPLAY_DIV).expect("loads");
    assert_eq!(format!("{:?}", r1.warnings), format!("{:?}", r2.warnings));
}

// ---- composite-subset contract corpus, end-to-end -----------------------------------------------

/// Root of the composite-contract conformance corpus (`docs/cxf-composite-subset.md`), shared
/// with the resolver-layer drivers in `oce-cxf` — the files are read at run time, not embedded,
/// so an emitter-authored fixture dropped into the corpus is picked up without a rebuild.
fn composite_corpus_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../oce-cxf/tests/fixtures/composite_contract")
}

/// Drive one corpus fixture through the full `Engine::load_cxf` pipeline.
fn load_composite_fixture(rel: &str) -> Result<LoadReport, OcError> {
    let path = composite_corpus_dir().join(rel);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("corpus fixture {} must be readable: {e}", path.display()));
    Engine::in_memory().load_cxf(&bytes)
}

/// Deterministically sorted `*.jsonld` listing of one corpus subdirectory.
fn sorted_composite_listing(subdir: &str) -> Vec<String> {
    let dir = composite_corpus_dir().join(subdir);
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("corpus subdir {} must exist: {e}", dir.display()))
        .map(|entry| entry.expect("readable directory entry").file_name())
        .filter_map(|name| {
            let name = name.to_str().expect("UTF-8 fixture name").to_owned();
            name.ends_with(".jsonld").then_some(name)
        })
        .collect();
    names.sort();
    names
}

/// The end-to-end pin per rejected corpus fixture: its contract rule id (`None` for untagged
/// shared import machinery, whose messages carry no `composite/` tag) plus the exact ordered
/// (code, subject, message) triples `Engine::load_cxf` must surface. Subjects appear only where
/// the contract defines one — the pure-cycle zero-root rejection carries `None`. Most fixtures
/// pin one triple; `diamond_cycle` pins two (one per cycle re-entry).
type CompositeRejection = (
    &'static str,
    Option<&'static str>,
    &'static [(DiagCode, Option<&'static str>, &'static str)],
);

const COMPOSITE_REJECTIONS: [CompositeRejection; 18] = [
    (
        "multi_root.jsonld",
        Some("root-count"),
        &[(
            DiagCode::MalformedDocument,
            Some("http://example.org#M"),
            "composite/root-count: expected exactly one top composite root after nested \
             classification, found 2 candidate roots: http://example.org#M, \
             http://example.org#M2",
        )],
    ),
    (
        "pure_cycle.jsonld",
        Some("root-count"),
        &[(
            DiagCode::MalformedDocument,
            None,
            "composite/root-count: expected exactly one top composite root after nested \
             classification, found zero candidate roots",
        )],
    ),
    (
        "reachable_cycle.jsonld",
        Some("contains-cycle"),
        &[(
            DiagCode::MalformedDocument,
            Some("http://example.org#A"),
            "composite/contains-cycle: cycle in nested composite containsBlock graph: \
             http://example.org#A -> http://example.org#B -> http://example.org#C -> \
             http://example.org#A",
        )],
    ),
    (
        "self_loop.jsonld",
        Some("contains-cycle"),
        &[(
            DiagCode::MalformedDocument,
            Some("http://example.org#A"),
            "composite/contains-cycle: cycle in nested composite containsBlock graph: \
             http://example.org#A -> http://example.org#A",
        )],
    ),
    (
        // ONE structural cycle {A, C} reachable via TWO containsBlock paths: the k-re-entry
        // contract end-to-end — one truthful path-ordered diagnostic per re-entry, in
        // post-finalize_diags subject order.
        "diamond_cycle.jsonld",
        Some("contains-cycle"),
        &[
            (
                DiagCode::MalformedDocument,
                Some("http://example.org#A"),
                "composite/contains-cycle: cycle in nested composite containsBlock graph: \
                 http://example.org#A -> http://example.org#C -> http://example.org#A",
            ),
            (
                DiagCode::MalformedDocument,
                Some("http://example.org#C"),
                "composite/contains-cycle: cycle in nested composite containsBlock graph: \
                 http://example.org#C -> http://example.org#A -> http://example.org#C",
            ),
        ],
    ),
    (
        // One reference cycle among the root's own declarations — one diagnostic per distinct
        // cycle, subject = the earliest participant in chained declaration order.
        "declaration_cycle.jsonld",
        Some("declaration-cycle"),
        &[(
            DiagCode::MalformedDocument,
            Some("http://example.org#M.a"),
            "composite/declaration-cycle: cycle in the block's own declaration references: \
             http://example.org#M.a -> http://example.org#M.b -> http://example.org#M.a",
        )],
    ),
    (
        // Self-reference is a length-1 declaration cycle, never an enclosing read.
        "self_reference.jsonld",
        Some("declaration-cycle"),
        &[(
            DiagCode::MalformedDocument,
            Some("http://example.org#M.sub.x"),
            "composite/declaration-cycle: cycle in the block's own declaration references: \
             http://example.org#M.sub.x -> http://example.org#M.sub.x",
        )],
    ),
    (
        // One local name bound twice in one chain: the later occurrence refuses, naming the
        // first; the first occurrence stays a normal binding.
        "duplicate_declaration.jsonld",
        Some("duplicate-declaration"),
        &[(
            DiagCode::MalformedDocument,
            Some("http://example.org#M.settings.k"),
            "composite/duplicate-declaration: own declaration \
             http://example.org#M.settings.k re-binds local name `k` first declared at \
             http://example.org#M.k",
        )],
    ),
    (
        "banned_key_bare.jsonld",
        Some("banned-modelica-key"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.c2"),
            "composite/banned-modelica-key: unsupported Modelica construct `redeclare` \
             survived CXF lowering",
        )],
    ),
    (
        "banned_key_prefixed.jsonld",
        Some("banned-modelica-key"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.c2"),
            "composite/banned-modelica-key: unsupported Modelica construct `S231:extendsFrom` \
             survived CXF lowering",
        )],
    ),
    (
        "banned_key_absolute_iri.jsonld",
        Some("banned-modelica-key"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.c2"),
            "composite/banned-modelica-key: unsupported Modelica construct \
             `http://data.ashrae.org/S231P#moSource` survived CXF lowering",
        )],
    ),
    (
        "array_parameter.jsonld",
        Some("array-parameter"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.p"),
            "composite/array-parameter: array-valued composite parameters are not supported \
             by this CXF lowering subset",
        )],
    ),
    (
        "array_connector.jsonld",
        Some("array-connector"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.c2.u"),
            "composite/array-connector: array-valued connector nodes are not supported; flatten \
             the array to one connector per element",
        )],
    ),
    (
        "array_instance.jsonld",
        Some("array-instance"),
        &[(
            DiagCode::NonSubsetConstruct,
            Some("http://example.org#M.c2"),
            "composite/array-instance: array-valued block-instance nodes are not supported; \
             flatten the array to one instance per element",
        )],
    ),
    (
        "replaceable.jsonld",
        Some("replaceable"),
        &[(
            DiagCode::UnresolvedPolymorphism,
            Some("http://example.org#M.c2"),
            "composite/replaceable: replaceable CXF components must be resolved before import",
        )],
    ),
    // Boundary-interface machinery (untagged, `None` rule id — not composite-shape rules): a
    // declared boundary output must not shadow an existing connector identity, and must not be
    // multiply driven.
    (
        "shadowed_output_child_connector.jsonld",
        None,
        &[(
            DiagCode::BoundaryOutputShadowsConnector,
            Some("http://example.org#M.c1.y"),
            "boundary output shadows an instance port connector",
        )],
    ),
    (
        "shadowed_output_input_output.jsonld",
        None,
        &[(
            DiagCode::BoundaryOutputShadowsConnector,
            Some("http://example.org#M.io"),
            "boundary output IRI is also a boundary input",
        )],
    ),
    (
        "multi_driven_boundary_output.jsonld",
        None,
        &[(
            DiagCode::SingleAssignment,
            Some("http://example.org#M.y"),
            "boundary output is multiply driven (distinct drivers 2)",
        )],
    ),
];

#[test]
fn composite_contract_rejections_carry_their_pinned_tagged_triples_end_to_end() {
    // The published contract, proven through the FULL pipeline: every rejected corpus file
    // surfaces exactly its ordered (code, subject, message) triples, and every tagged rule's
    // message begins with the `composite/<rule-id>: ` tag the catalog publishes for it. An
    // untagged (`None`) entry is shared import machinery: its message must NOT wear a contract
    // tag it has no catalog row for.
    for (file, rule_id, expected) in COMPOSITE_REJECTIONS {
        let err = load_composite_fixture(&format!("rejected/{file}"))
            .expect_err("every rejected corpus fixture must fail to load");
        let signature = error_signature(&err);
        let expected: Vec<(DiagCode, Option<String>, String)> = expected
            .iter()
            .map(|(code, subject, message)| {
                (*code, subject.map(str::to_owned), (*message).to_owned())
            })
            .collect();
        assert_eq!(
            signature, expected,
            "rejected/{file}: the end-to-end error signature must match its published pins"
        );
        match rule_id {
            Some(rule_id) => {
                let prefix = format!("composite/{rule_id}: ");
                assert!(
                    signature.iter().all(|(_, _, msg)| msg.starts_with(&prefix)),
                    "rejected/{file}: every contract rejection must start with {prefix:?}"
                );
            }
            None => assert!(
                signature
                    .iter()
                    .all(|(_, _, msg)| !msg.starts_with("composite/")),
                "rejected/{file}: untagged machinery must not wear a composite/ contract tag"
            ),
        }
    }
}

#[test]
fn composite_contract_warned_fixture_surfaces_exactly_its_pinned_warning_end_to_end() {
    // The warned corpus through the FULL pipeline: the load succeeds and LoadReport.warnings
    // carries exactly the published advisory — nothing upstream (resolver), midstream
    // (validate), or downstream (semantics) adds to or reorders it.
    let report = load_composite_fixture("warned/undriven_boundary_output.jsonld")
        .expect("the warned fixture must load end-to-end");
    let rendered: Vec<(DiagCode, bool, Option<String>, String)> = report
        .warnings
        .iter()
        .map(|d| {
            (
                d.code,
                d.is_error(),
                d.subject.as_deref().map(str::to_owned),
                d.message.clone(),
            )
        })
        .collect();
    assert_eq!(
        rendered,
        vec![(
            DiagCode::UndrivenBoundaryOutput,
            false,
            Some("http://example.org#M.y".to_owned()),
            "declared boundary output has no internal driver".to_owned(),
        )],
        "warned/undriven_boundary_output.jsonld: exactly one advisory, nothing else"
    );

    let mut listing = sorted_composite_listing("warned");
    listing.sort();
    assert_eq!(
        listing,
        vec!["undriven_boundary_output.jsonld".to_owned()],
        "the on-disk warned corpus and this pin must stay one-to-one"
    );
}

#[test]
fn composite_contract_rejected_listing_matches_the_pinned_table() {
    // No unpinned fixture, no phantom pin: the on-disk rejected corpus and the expectation
    // table stay one-to-one, so a new emitter-facing fixture cannot land silently untested.
    let mut pinned: Vec<String> = COMPOSITE_REJECTIONS
        .iter()
        .map(|(file, ..)| (*file).to_owned())
        .collect();
    pinned.sort();
    assert_eq!(sorted_composite_listing("rejected"), pinned);
}

#[test]
fn composite_contract_accepted_fixtures_load_warning_free_end_to_end() {
    // The accepted corpus is warning-FREE by contract (not merely error-free): the same
    // documents the resolver goldens bless must also build through flatten/validate/BUILD
    // with an empty diagnostic stream.
    let listing = sorted_composite_listing("accepted");
    assert!(!listing.is_empty(), "the accepted corpus must not be empty");
    for file in listing {
        let report = load_composite_fixture(&format!("accepted/{file}"))
            .unwrap_or_else(|e| panic!("accepted/{file} must load end-to-end: {e:?}"));
        assert!(
            report.block_count >= 1,
            "accepted/{file}: a non-trivial model was built"
        );
        assert!(
            report.warnings.is_empty(),
            "accepted/{file}: the corpus promises a warning-free load, got {:?}",
            report.warnings
        );
    }
}

#[test]
fn full_pipeline_trace_is_bit_deterministic() {
    // exit #4 through the WHOLE pipeline: two independent load_cxf + multi-tick runs produce a
    // bit-identical output trace (complements resolve_golden's resolver-only render compare).
    let run = || {
        let mut eng = Engine::in_memory();
        eng.load_cxf(MINIMAL_LOOP.as_bytes()).expect("loads");
        let mut frames = Vec::new();
        for k in 0..6 {
            eng.tick(k as f64).expect("monotonic ticks");
            frames.push(eng.outputs().to_map());
        }
        frames
    };
    let a = run();
    let b = run();
    assert_eq!(a.len(), b.len());
    for (fa, fb) in a.iter().zip(&b) {
        assert_eq!(fa.len(), fb.len(), "same output cardinality each frame");
        for ((pa, va), (pb, vb)) in fa.iter().zip(fb) {
            assert_eq!(pa, pb, "output paths must match in order");
            assert!(va.bit_eq(vb), "output {pa} diverged: {va:?} vs {vb:?}");
        }
    }
}
