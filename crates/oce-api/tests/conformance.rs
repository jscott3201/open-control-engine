//! M1 **exit #4 + #2/#3** conformance harness: drive the hand-authored fixture corpus through the
//! **full** `Engine::load_cxf` pipeline (CXF resolve → flatten → validate → BUILD) and assert the
//! end-to-end accept/reject/warn contract, plus full-pipeline determinism (`TESTING.md` pillars
//! 1 + 2 + 4).
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

// ---- rejection corpus (exit #2/#3, end-to-end through the full pipeline) ------------------------

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
    // An unregistered CDL class (PID is not in the M1 registry) ⇒ ClassNotFound, never a panic.
    assert_rejected_with(NON_SUBSET, DiagCode::ClassNotFound);
}

#[test]
fn bound_mismatch_is_rejected_end_to_end() {
    // con.y declares S231:min 0.0; the driven peer add.u1 declares S231:min 5.0 — two distinct
    // declared bounds in one connected cluster ⇒ §7.10 R13.1 BoundMismatch. This is the bound twin
    // of unit_mismatch and proves the resolver's connector-bound grounding (M1-PR-11) is load-bearing
    // end-to-end: without the min flowing onto Connector.attrs, this would (wrongly) load clean.
    assert_rejected_with(BOUND_MISMATCH, DiagCode::BoundMismatch);
}

// ---- propagation + warn-only (exit #3) ---------------------------------------------------------

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

// ---- determinism (exit #4, end-to-end) ---------------------------------------------------------

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
