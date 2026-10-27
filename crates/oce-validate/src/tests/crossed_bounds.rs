//! Crossed-bound (`min > max`) refusal on connectors, boundary declarations, a boundary input's
//! combined domain, and ranges that propagation creates (issue #378).
//!
//! Every expected diagnostic below is stated by hand from the rule, never taken from the engine.

use super::common::*;
use oce_model::BoundaryInput;

/// Real `(min, max)` pairs that admit no value: ordinary, adjacent-ulp, subnormal, signed-zero
/// neighbours and the full finite span reversed.
const CROSSED: [(f64, f64); 6] = [
    (1.0, 0.0),
    (1.0, 0.999_999_999_999_999_9),
    (5e-324, 0.0),
    (0.0, -5e-324),
    (f64::MAX, f64::MIN),
    (-1.0, -2.0),
];

/// Real `(min, max)` pairs that stay legal: equal bounds (one admissible value), IEEE-equal signed
/// zeros in both orders, and ordinary ranges.
const LEGAL: [(f64, f64); 8] = [
    (1.0, 1.0),
    (0.0, -0.0),
    (-0.0, 0.0),
    (f64::MAX, f64::MAX),
    (f64::MIN, f64::MIN),
    (5e-324, 5e-324),
    (0.0, 5e-324),
    (f64::MIN, f64::MAX),
];

fn bounds(min: f64, max: f64) -> RealAttrs {
    RealAttrs {
        min: Some(min),
        max: Some(max),
        ..RealAttrs::default()
    }
}

/// A well-formed Constant C0 -> Abs (C1 in, C2 out), with the given attributes on C1 only.
fn edge_model(input: RealAttrs) -> ModelGraph {
    ModelGraph {
        blocks: vec![
            constant_block(0, &[0]),
            block(1, "CDL.Reals.Abs", &[1], &[2]),
        ],
        connectors: vec![
            real_conn(0, 0, Dir::Out, RealAttrs::default()),
            real_conn(1, 1, Dir::In, input),
            real_conn(2, 1, Dir::Out, RealAttrs::default()),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    }
}

/// A root boundary input `urn:b:u` feeding Abs's external input C0, which carries the boundary's
/// path the way an elided child does.
fn boundary_model(declaration: Attrs, target: Attrs) -> ModelGraph {
    let mut model = real_to_real_model("CDL.Reals.Abs", vec![]);
    model.connectors[0].attrs = target;
    model.connectors[0].iri = Some(Arc::from("urn:b:u"));
    model.boundary_inputs = vec![BoundaryInput {
        iri: Arc::from("urn:b:u"),
        attrs: declaration,
    }];
    model
}

fn real_line(owner: &str, min: f64, max: f64) -> String {
    format!(
        "S231:min ({min}) exceeds S231:max ({max}) on a Real {owner}; the declared range admits \
         no value"
    )
}

fn rendered(error: &crate::ValidationError) -> Vec<(DiagCode, String, String)> {
    error
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.code,
                d.subject.as_deref().unwrap_or_default().to_owned(),
                d.message.clone(),
            )
        })
        .collect()
}

#[test]
fn every_crossed_real_connector_range_is_refused_before_unification() {
    for (min, max) in CROSSED {
        let mut model = edge_model(bounds(min, max));
        let error = unify_attributes(&mut model).expect_err("crossed bounds");
        assert_eq!(
            rendered(&error),
            [(
                DiagCode::CrossedBounds,
                "connector#1".to_owned(),
                real_line("connector", min, max)
            )],
            "({min}, {max})"
        );
        assert_eq!(error.diagnostics[0].severity, Severity::Error);
        // Nothing propagated to the unset peer: unification never ran.
        let peer = model.connectors[0].attrs.as_real().unwrap();
        assert!(peer.min.is_none() && peer.max.is_none());
        // The pure checker and the facade entry point report the same single cause.
        for error in [
            validate(&edge_model(bounds(min, max))).expect_err("validate"),
            unify_and_validate(&mut edge_model(bounds(min, max))).expect_err("unify+validate"),
        ] {
            assert_eq!(codes(&error.diagnostics), [DiagCode::CrossedBounds]);
        }
    }
}

#[test]
fn equal_and_ordered_real_ranges_stay_legal_on_every_owner() {
    for (min, max) in LEGAL {
        unify_and_validate(&mut edge_model(bounds(min, max)))
            .unwrap_or_else(|e| panic!("connector ({min}, {max}): {e:?}"));
        let real = |a| Attrs::Real(a);
        unify_and_validate(&mut boundary_model(
            real(bounds(min, max)),
            real(RealAttrs::default()),
        ))
        .unwrap_or_else(|e| panic!("boundary ({min}, {max}): {e:?}"));
        // Split across the declaration and its target, the combined range is the same.
        unify_and_validate(&mut boundary_model(
            real(RealAttrs {
                min: Some(min),
                ..RealAttrs::default()
            }),
            real(RealAttrs {
                max: Some(max),
                ..RealAttrs::default()
            }),
        ))
        .unwrap_or_else(|e| panic!("combined ({min}, {max}): {e:?}"));
    }
}

#[test]
fn a_one_sided_bound_is_never_crossed() {
    for value in [f64::MAX, f64::MIN, 0.0, -0.0] {
        for attrs in [
            RealAttrs {
                min: Some(value),
                ..RealAttrs::default()
            },
            RealAttrs {
                max: Some(value),
                ..RealAttrs::default()
            },
        ] {
            unify_and_validate(&mut edge_model(attrs)).expect("one-sided Real bound");
        }
    }
}

#[test]
fn a_non_finite_bound_is_reported_once_as_non_finite_never_also_as_crossed() {
    for (min, max) in [
        (f64::INFINITY, 1.0),
        (1.0, f64::NEG_INFINITY),
        (f64::NAN, 1.0),
        (f64::INFINITY, f64::NEG_INFINITY),
    ] {
        let error = unify_and_validate(&mut edge_model(bounds(min, max))).expect_err("refused");
        assert!(
            codes(&error.diagnostics)
                .iter()
                .all(|code| *code == DiagCode::NonFiniteBound),
            "({min}, {max}): {:?}",
            error.diagnostics
        );
        let error = validate(&edge_model(bounds(min, max))).expect_err("refused");
        assert!(!codes(&error.diagnostics).contains(&DiagCode::CrossedBounds));
    }
}

#[test]
fn crossed_boundary_input_and_output_declarations_are_refused() {
    let default = Attrs::Real(RealAttrs::default());
    let mut model = boundary_model(Attrs::Real(bounds(2.0, 1.0)), default.clone());
    let error = unify_and_validate(&mut model).expect_err("boundary input");
    assert_eq!(
        rendered(&error),
        [(
            DiagCode::CrossedBounds,
            "urn:b:u".to_owned(),
            real_line("boundary input", 2.0, 1.0)
        )]
    );

    let mut model = ModelGraph {
        blocks: vec![constant_block(0, &[0])],
        connectors: vec![real_conn(0, 0, Dir::Out, RealAttrs::default())],
        boundary_outputs: vec![boundary_output("urn:b:y", 0, Attrs::Real(bounds(2.0, 1.0)))],
        ..ModelGraph::new()
    };
    let error = unify_and_validate(&mut model).expect_err("boundary output");
    assert_eq!(
        rendered(&error),
        [(
            DiagCode::CrossedBounds,
            "urn:b:y".to_owned(),
            real_line("boundary output", 2.0, 1.0)
        )]
    );
}

#[test]
fn a_crossed_elided_target_is_reported_once_on_the_boundary_path() {
    // The elided child carries the boundary's IRI, so its own refusal already names the path the
    // host supplies; the combined check adds nothing more.
    let declaration = Attrs::Real(RealAttrs::default());
    let mut model = boundary_model(declaration, Attrs::Real(bounds(2.0, 1.0)));
    let error = unify_and_validate(&mut model).expect_err("crossed target");
    assert_eq!(
        rendered(&error),
        [(
            DiagCode::CrossedBounds,
            "urn:b:u".to_owned(),
            real_line("connector", 2.0, 1.0)
        )]
    );
}

#[test]
fn a_declaration_and_target_that_combine_to_an_empty_domain_are_refused() {
    let combined = |min: &str, max: &str, kind: &str| {
        format!(
            "the combined S231:min ({min}) exceeds the combined S231:max ({max}) across the \
             {kind} boundary input and the child inputs it drives; no input value satisfies \
             them all"
        )
    };
    let lower = |v| {
        Attrs::Real(RealAttrs {
            min: Some(v),
            ..RealAttrs::default()
        })
    };
    let upper = |v| {
        Attrs::Real(RealAttrs {
            max: Some(v),
            ..RealAttrs::default()
        })
    };
    let subnormal = 5e-324_f64.to_string();
    for (declaration, target, min, max) in [
        (lower(5.0), upper(3.0), "5", "3"),
        (upper(3.0), lower(5.0), "5", "3"),
        (lower(5e-324), upper(0.0), subnormal.as_str(), "0"),
    ] {
        let mut model = boundary_model(declaration, target);
        let error = unify_and_validate(&mut model).expect_err("empty combined domain");
        assert_eq!(
            rendered(&error),
            [(
                DiagCode::CrossedBounds,
                "urn:b:u".to_owned(),
                combined(min, max, "Real")
            )]
        );
    }
    // The tighter of two bounds on one side is what counts.
    let mut model = boundary_model(Attrs::Real(bounds(0.0, 10.0)), upper(-1.0));
    assert_eq!(
        rendered(&unify_and_validate(&mut model).expect_err("combined")),
        [(
            DiagCode::CrossedBounds,
            "urn:b:u".to_owned(),
            combined("0", "-1", "Real")
        )]
    );
    // A combination that only meets at one value stays legal.
    unify_and_validate(&mut boundary_model(lower(3.0), upper(3.0))).expect("single value");
    // Integer declaration and target combine the same way, after the CDL defaults.
    let mut model = one_block_model(
        "CDL.Integers.Abs",
        &[ValueType::Integer],
        &[ValueType::Integer],
        vec![],
    );
    model.connectors[0].attrs = Attrs::Integer(IntAttrs {
        min: None,
        max: Some(3),
    });
    model.connectors[0].iri = Some(Arc::from("urn:b:n"));
    model.boundary_inputs = vec![BoundaryInput {
        iri: Arc::from("urn:b:n"),
        attrs: Attrs::Integer(IntAttrs {
            min: Some(4),
            max: None,
        }),
    }];
    assert_eq!(
        rendered(&unify_and_validate(&mut model).expect_err("Integer combined")),
        [(
            DiagCode::CrossedBounds,
            "urn:b:n".to_owned(),
            combined("4", "3", "Integer")
        )]
    );
}

#[test]
fn integer_ranges_compare_their_effective_bounds() {
    let int_line = |min: &str, max: &str| {
        format!(
            "S231:min ({min}) exceeds S231:max ({max}) on an Integer connector; the declared \
             range admits no value"
        )
    };
    let default_max = "2147483647, the CDL Integer default";
    let default_min = "-2147483648, the CDL Integer default";
    for (min, max, expected) in [
        (Some(5), Some(4), Some(int_line("5", "4"))),
        (
            Some(i64::MAX),
            Some(i64::MIN),
            Some(int_line(&i64::MAX.to_string(), &i64::MIN.to_string())),
        ),
        // One past the CDL Integer limit crosses the undeclared opposite side.
        (
            Some(2_147_483_648),
            None,
            Some(int_line("2147483648", default_max)),
        ),
        (
            None,
            Some(-2_147_483_649),
            Some(int_line(default_min, "-2147483649")),
        ),
        // Legal: equal, the limits themselves, and the full i64 span when both are declared.
        (Some(7), Some(7), None),
        (Some(2_147_483_647), None, None),
        (None, Some(-2_147_483_648), None),
        (Some(i64::MIN), Some(i64::MAX), None),
        (Some(i64::MAX), Some(i64::MAX), None),
        (None, None, None),
    ] {
        let mut model = one_block_model(
            "CDL.Integers.Abs",
            &[ValueType::Integer],
            &[ValueType::Integer],
            vec![],
        );
        model.connectors[0] = int_conn(0, 0, Dir::In, IntAttrs { min, max });
        let result = unify_and_validate(&mut model);
        match expected {
            Some(message) => assert_eq!(
                rendered(&result.expect_err("crossed Integer range")),
                [(DiagCode::CrossedBounds, "connector#0".to_owned(), message)],
                "({min:?}, {max:?})"
            ),
            None => {
                result.unwrap_or_else(|e| panic!("({min:?}, {max:?}): {e:?}"));
            }
        }
    }
}

#[test]
fn a_crossing_created_by_propagation_is_refused_and_rolled_back() {
    // C0 (source) declares only min = 5; its peer C1 only max = 3. Neither is crossed alone, but
    // §7.10 propagation would give both the empty range [5, 3].
    let mut model = edge_model(RealAttrs {
        max: Some(3.0),
        ..RealAttrs::default()
    });
    model.connectors[0] = real_conn(
        0,
        0,
        Dir::Out,
        RealAttrs {
            min: Some(5.0),
            ..RealAttrs::default()
        },
    );
    let error = unify_attributes(&mut model).expect_err("propagated crossing");
    // One refusal for the cluster, on its source output, naming the propagated range — never a
    // "declared range" on a member that declared only one side.
    assert_eq!(
        rendered(&error),
        [(
            DiagCode::CrossedBounds,
            "connector#0".to_owned(),
            propagated_line("2 connectors", "5", "3")
        )]
    );
    // Transactional: the authored one-sided bounds are back.
    let source = model.connectors[0].attrs.as_real().unwrap();
    assert_eq!(source.min.map(f64::to_bits), Some(5.0_f64.to_bits()));
    assert!(source.max.is_none());
    let target = model.connectors[1].attrs.as_real().unwrap();
    assert!(target.min.is_none());
    assert_eq!(target.max.map(f64::to_bits), Some(3.0_f64.to_bits()));
    // The facade entry point refuses identically.
    let mut again = model.clone();
    assert_eq!(
        rendered(&unify_and_validate(&mut again).expect_err("propagated crossing")),
        rendered(&error)
    );
}

fn propagated_line(members: &str, min: &str, max: &str) -> String {
    format!(
        "§7.10 propagation across the connected Real cluster driven by this output ({members}) \
         gives the effective range S231:min ({min}) above S231:max ({max}), which admits no \
         value; no single member declares both"
    )
}

#[test]
fn a_propagated_crossing_is_reported_once_per_cluster_with_its_members_counted() {
    // Source C0 declares nothing; target C1 declares max = 3 and the aliasing boundary output
    // declares min = 5. Propagation would give all three [5, 3].
    let mut model = edge_model(RealAttrs {
        max: Some(3.0),
        ..RealAttrs::default()
    });
    model.boundary_outputs = vec![boundary_output(
        "urn:b:y",
        0,
        Attrs::Real(RealAttrs {
            min: Some(5.0),
            ..RealAttrs::default()
        }),
    )];
    let error = unify_and_validate(&mut model).expect_err("propagated crossing");
    assert_eq!(
        rendered(&error),
        [(
            DiagCode::CrossedBounds,
            "connector#0".to_owned(),
            propagated_line("2 connectors and 1 boundary output", "5", "3")
        )]
    );
    assert!(model.connectors[0].attrs.as_real().unwrap().min.is_none());
    assert!(
        model.boundary_outputs[0]
            .attrs
            .as_real()
            .unwrap()
            .max
            .is_none()
    );
    // Meeting at one value is legal after propagation too.
    let mut model = edge_model(RealAttrs {
        max: Some(5.0),
        ..RealAttrs::default()
    });
    model.connectors[0] = real_conn(
        0,
        0,
        Dir::Out,
        RealAttrs {
            min: Some(5.0),
            ..RealAttrs::default()
        },
    );
    unify_and_validate(&mut model).expect("propagated single value");
}

#[test]
fn crossed_bound_refusals_are_deterministic_and_ordered() {
    let mut model = boundary_model(Attrs::Real(bounds(9.0, 8.0)), Attrs::Real(bounds(2.0, 1.0)));
    model.connectors[1].attrs = Attrs::Real(bounds(4.0, 3.0));
    let first = unify_attributes(&mut model.clone()).expect_err("crossed");
    for _ in 0..3 {
        assert_eq!(
            unify_attributes(&mut model.clone())
                .expect_err("crossed")
                .diagnostics,
            first.diagnostics
        );
    }
    assert_eq!(
        rendered(&first),
        [
            (
                DiagCode::CrossedBounds,
                "urn:b:u".to_owned(),
                real_line("connector", 2.0, 1.0)
            ),
            // The declaration shares its path with connector 0, so it sorts with it.
            (
                DiagCode::CrossedBounds,
                "urn:b:u".to_owned(),
                real_line("boundary input", 9.0, 8.0)
            ),
            (
                DiagCode::CrossedBounds,
                "connector#1".to_owned(),
                real_line("connector", 4.0, 3.0)
            ),
        ]
    );
}
