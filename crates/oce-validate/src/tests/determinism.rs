//! Determinism (exit #4): byte-identical diagnostics across runs, the full sort key (primary id,
//! then code, then message tie-breakers), cluster-root ordering, and the `unify_and_validate`
//! end-to-end ordering.

use super::common::*;

// ---- T26: determinism (primary id key) ------------------------------------------------------

#[test]
fn t26_diagnostics_are_deterministic_and_id_sorted() {
    // Several violations across connectors at different ids; the stream must be byte-identical
    // across runs and ordered by ascending ConnectorId.0 (synthetic connector#N parsed numerically).
    let make = || ModelGraph {
        blocks: vec![block(0, "unknown.Class", &[0, 1, 2], &[])],
        connectors: vec![
            conn(0, 0, Dir::In, ValueType::Real), // undriven non-external → single-assignment
            conn(1, 0, Dir::In, ValueType::Real), // undriven non-external → single-assignment
            conn(2, 0, Dir::In, ValueType::Real), // undriven non-external → single-assignment
        ],
        connections: vec![],
        external_inputs: vec![],
    };
    let run1 = validate(&make())
        .expect_err("three undriven inputs")
        .diagnostics;
    let run2 = validate(&make())
        .expect_err("three undriven inputs")
        .diagnostics;
    assert_eq!(run1, run2, "validation must be deterministic");
    // Subjects ascend by connector id: connector#0, connector#1, connector#2.
    let subjects: Vec<&str> = run1.iter().filter_map(|d| d.subject.as_deref()).collect();
    assert_eq!(subjects, vec!["connector#0", "connector#1", "connector#2"]);
}

// ---- T27–T28: unify_and_validate end-to-end -------------------------------------------------

#[test]
fn t27_unify_and_validate_propagates_then_passes() {
    // A structurally valid Constant→Add graph where the Constant output declares "K" and the driven
    // Add input is unset → unify_and_validate propagates "K" AND passes every structural/type rule.
    let mut m = ModelGraph {
        blocks: vec![
            block(0, "CDL.Reals.Sources.Constant", &[], &[0]),
            block(1, "CDL.Reals.Add", &[1, 2], &[3]),
        ],
        connectors: vec![
            real_unit(0, 0, Dir::Out, Some("K")),
            real_unit(1, 1, Dir::In, None),
            real_unit(2, 1, Dir::In, None),
            real_unit(3, 1, Dir::Out, None),
        ],
        connections: vec![conn_edge(0, 1)],
        external_inputs: vec![ConnectorId(2)],
    };
    let warnings = unify_and_validate(&mut m).expect("valid graph loads clean");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        m.connectors[1].attrs.as_real().unwrap().unit(),
        "K",
        "R13.2 should have propagated the unit before validation"
    );
}

#[test]
fn t28_unify_and_validate_short_circuits_on_unit_conflict() {
    // The unify stage fails first → unify_and_validate returns its error (structural never runs).
    let mut m = ModelGraph {
        connectors: vec![
            real_unit(0, 0, Dir::Out, Some("K")),
            real_unit(1, 1, Dir::In, Some("degC")),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    };
    let err = unify_and_validate(&mut m).expect_err("unit conflict short-circuits");
    assert_eq!(
        codes(&err.diagnostics),
        vec![DiagCode::UnitQuantityMismatch]
    );
}

// ---- T31–T32: sort tie-breakers (code, then message) ----------------------------------------

#[test]
fn t31_same_subject_two_codes_sort_by_code_string() {
    // A connection to an OUTPUT typed differently from its source yields TWO diagnostics on the SAME
    // subject (connector#1): DirectionMismatch (to not In) + TypeMismatch (Real vs Boolean). Equal
    // key_cid and subject → the code tie-breaker decides: "direction-mismatch" < "type-mismatch".
    let make = || ModelGraph {
        blocks: vec![block(0, "unknown.Class", &[], &[0, 1])],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 0, Dir::Out, ValueType::Boolean),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    };
    let d1 = validate(&make()).expect_err("two diagnostics").diagnostics;
    let d2 = validate(&make()).expect_err("two diagnostics").diagnostics;
    assert_eq!(d1, d2, "must be deterministic");
    assert_eq!(
        codes(&d1),
        vec![DiagCode::DirectionMismatch, DiagCode::TypeMismatch],
        "code tie-breaker must order by DiagCode::as_str"
    );
    assert!(
        d1.iter()
            .all(|x| x.subject.as_deref() == Some("connector#1"))
    );
}

#[test]
fn t32_same_code_and_subject_sort_by_message() {
    // Two out-of-range connections → two MalformedDocument diagnostics, both subject=None (key_cid
    // u32::MAX), same code → the MESSAGE tie-breaker decides. Messages embed the connection index.
    let make = || ModelGraph {
        blocks: vec![block(0, "CDL.Reals.Sources.Constant", &[], &[0])],
        connectors: vec![conn(0, 0, Dir::Out, ValueType::Real)],
        connections: vec![conn_edge(0, 8), conn_edge(0, 9)], // both `to` out of range
        ..ModelGraph::new()
    };
    let d1 = validate(&make()).expect_err("two malformed").diagnostics;
    let d2 = validate(&make()).expect_err("two malformed").diagnostics;
    assert_eq!(d1, d2, "must be deterministic");
    assert_eq!(
        codes(&d1),
        vec![DiagCode::MalformedDocument, DiagCode::MalformedDocument]
    );
    assert!(
        d1[0].message.contains("#0"),
        "message tie-breaker: #0 before #1"
    );
    assert!(d1[1].message.contains("#1"));
}

#[test]
fn t35_block_subjects_sort_by_numeric_block_id() {
    // Arity diagnostics use synthetic block#N subjects for hand-built graphs. The finalizer must
    // parse N numerically, otherwise block#10 sorts before block#3 lexicographically.
    let make = || {
        let mut blocks: Vec<BlockInstance> = (0..=10)
            .map(|id| block(id, "unknown.Class", &[], &[]))
            .collect();
        blocks[3] = block(3, "CDL.Reals.Add", &[0], &[1]);
        blocks[10] = block(10, "CDL.Reals.Add", &[2], &[3]);
        ModelGraph {
            blocks,
            connectors: vec![
                conn(0, 3, Dir::In, ValueType::Real),
                conn(1, 3, Dir::Out, ValueType::Real),
                conn(2, 10, Dir::In, ValueType::Real),
                conn(3, 10, Dir::Out, ValueType::Real),
            ],
            connections: vec![],
            external_inputs: vec![ConnectorId(0), ConnectorId(2)],
        }
    };

    let d1 = validate(&make())
        .expect_err("two block arity diagnostics")
        .diagnostics;
    let d2 = validate(&make())
        .expect_err("two block arity diagnostics")
        .diagnostics;
    assert_eq!(d1, d2, "block-subject sorting must be deterministic");
    assert_eq!(
        codes(&d1),
        vec![DiagCode::MalformedDocument, DiagCode::MalformedDocument]
    );
    let subjects: Vec<&str> = d1.iter().filter_map(|x| x.subject.as_deref()).collect();
    assert_eq!(subjects, vec!["block#3", "block#10"]);
}

// ---- T33: chained / cross-cluster transitive propagation + confluence -----------------------

#[test]
fn t33_chained_clusters_propagate_transitively_and_deterministically() {
    // C0(Out,"K") -> C1(unset) -> C2(unset): C1 is BOTH a driven member (of root C0) AND a cluster
    // root (driving C2). Roots sort [0,1]: cluster C0 fills C1="K", then cluster C1 (now "K") fills
    // C2="K" — the cross-cluster transitive path. (The graph also has a C1->C2 direction error, but
    // unify_attributes runs ONLY §7.10, so the cluster path is exercised in isolation.) Two runs
    // must be byte-identical with identical final units — the confluence claim, now under test.
    let make = || ModelGraph {
        connectors: vec![
            real_unit(0, 0, Dir::Out, Some("K")),
            real_unit(1, 1, Dir::In, None),
            real_unit(2, 2, Dir::In, None),
        ],
        connections: vec![conn_edge(0, 1), conn_edge(1, 2)],
        ..ModelGraph::new()
    };
    let mut m1 = make();
    let w1 = unify_attributes(&mut m1).expect("chain unifies");
    let mut m2 = make();
    let w2 = unify_attributes(&mut m2).expect("chain unifies");
    assert_eq!(w1, w2, "chained unification diagnostics deterministic");
    assert!(w1.is_empty());
    for m in [&m1, &m2] {
        assert_eq!(m.connectors[1].attrs.as_real().unwrap().unit(), "K");
        assert_eq!(
            m.connectors[2].attrs.as_real().unwrap().unit(),
            "K",
            "propagation must reach C2 transitively"
        );
    }
}

// ---- T34: multi-root cluster determinism golden ----------------------------------------------

#[test]
fn t34_multi_root_conflict_diagnostics_are_id_sorted_and_deterministic() {
    // Two independent conflicting clusters: C0("K")->C1("degC") and C2("Pa")->C3("bar"). Two
    // UnitQuantityMismatch on roots C0 and C2; subjects ascend connector#0, #2 and are byte-identical
    // across runs (the cluster-root-sort path T26 never reached — it had zero connections).
    let make = || ModelGraph {
        connectors: vec![
            real_unit(0, 0, Dir::Out, Some("K")),
            real_unit(1, 1, Dir::In, Some("degC")),
            real_unit(2, 2, Dir::Out, Some("Pa")),
            real_unit(3, 3, Dir::In, Some("bar")),
        ],
        connections: vec![conn_edge(0, 1), conn_edge(2, 3)],
        ..ModelGraph::new()
    };
    let mut m1 = make();
    let mut m2 = make();
    let d1 = unify_attributes(&mut m1)
        .expect_err("two conflicts")
        .diagnostics;
    let d2 = unify_attributes(&mut m2)
        .expect_err("two conflicts")
        .diagnostics;
    assert_eq!(d1, d2, "multi-root unification deterministic");
    assert_eq!(
        codes(&d1),
        vec![
            DiagCode::UnitQuantityMismatch,
            DiagCode::UnitQuantityMismatch
        ]
    );
    let subjects: Vec<&str> = d1.iter().filter_map(|x| x.subject.as_deref()).collect();
    assert_eq!(subjects, vec!["connector#0", "connector#2"]);
}
