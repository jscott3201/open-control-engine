//! §7.10 attribute unification (doc 02 §9 R13.1–R13.4): one-sided propagation, conflict (no
//! mutation), fan-out, exact-string and signed-zero tripwires, displayUnit divergence, the
//! tag-invariant skip, the no-overwrite guard, and min/max bound unification (Real + Integer).

use super::common::*;

// ---- T14–T20: §7.10 unit/quantity unification (cluster algorithm) ---------------------------

#[test]
fn t14_one_sided_unit_propagates_to_unset_peer() {
    // Output C0 declares unit "K"; input C1 is unset → R13.2 propagates "K" to C1.
    let mut m = ModelGraph {
        blocks: vec![],
        connectors: vec![
            real_unit(0, 0, Dir::Out, Some("K")),
            real_unit(1, 1, Dir::In, None),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    };
    let warnings = unify_attributes(&mut m).expect("one-sided unit unifies cleanly");
    assert!(warnings.is_empty());
    assert_eq!(m.connectors[1].attrs.as_real().unwrap().unit(), "K");
    // The declaring end still reads "K" (this is its own value; the `is_none()` no-overwrite guard
    // is pinned separately by `t29_propagation_does_not_overwrite_already_set_peer` via Arc identity).
    assert_eq!(m.connectors[0].attrs.as_real().unwrap().unit(), "K");
}

#[test]
fn t15_one_sided_unit_propagates_input_to_output() {
    // Reverse: input declares "Pa", output unset → propagate to the output too (both ends unify).
    let mut m = ModelGraph {
        connectors: vec![
            real_unit(0, 0, Dir::Out, None),
            real_unit(1, 1, Dir::In, Some("Pa")),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    };
    unify_attributes(&mut m).expect("propagation succeeds");
    assert_eq!(m.connectors[0].attrs.as_real().unwrap().unit(), "Pa");
}

#[test]
fn t16_conflicting_units_are_a_hard_error() {
    // Output "K", input "degC", both declared & differ → R13.1 UnitQuantityMismatch (no mutation).
    let mut m = ModelGraph {
        connectors: vec![
            real_unit(0, 0, Dir::Out, Some("K")),
            real_unit(1, 1, Dir::In, Some("degC")),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    };
    let err = unify_attributes(&mut m).expect_err("unit conflict must fail");
    assert_eq!(
        codes(&err.diagnostics),
        vec![DiagCode::UnitQuantityMismatch]
    );
    // Sorted, deterministic value listing in the message.
    assert!(err.diagnostics[0].message.contains("[\"K\", \"degC\"]"));
    // R13.1 NO-MUTATION invariant: a rejected graph must NOT be partially rewritten. Both declared
    // units stay exactly as authored — neither is clobbered by an errant propagation.
    assert_eq!(m.connectors[0].attrs.as_real().unwrap().unit(), "K");
    assert_eq!(m.connectors[1].attrs.as_real().unwrap().unit(), "degC");
}

#[test]
fn t17_conflicting_quantity_is_a_hard_error() {
    let mut m = ModelGraph {
        connectors: vec![
            real_conn(
                0,
                0,
                Dir::Out,
                RealAttrs {
                    quantity: Some(Arc::from("Power")),
                    ..RealAttrs::default()
                },
            ),
            real_conn(
                1,
                1,
                Dir::In,
                RealAttrs {
                    quantity: Some(Arc::from("Temperature")),
                    ..RealAttrs::default()
                },
            ),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    };
    let err = unify_attributes(&mut m).expect_err("quantity conflict must fail");
    assert_eq!(
        codes(&err.diagnostics),
        vec![DiagCode::UnitQuantityMismatch]
    );
    assert!(err.diagnostics[0].message.contains("quantity"));
    // R13.1 no-mutation: the conflicting quantities are left exactly as authored.
    assert_eq!(m.connectors[0].attrs.as_real().unwrap().quantity(), "Power");
    assert_eq!(
        m.connectors[1].attrs.as_real().unwrap().quantity(),
        "Temperature"
    );
}

#[test]
fn t18_fanout_propagates_to_all_unset_inputs() {
    // One output "K" drives two unset inputs → both receive "K" (star cluster).
    let mut m = ModelGraph {
        connectors: vec![
            real_unit(0, 0, Dir::Out, Some("K")),
            real_unit(1, 1, Dir::In, None),
            real_unit(2, 2, Dir::In, None),
        ],
        connections: vec![conn_edge(0, 1), conn_edge(0, 2)],
        ..ModelGraph::new()
    };
    unify_attributes(&mut m).expect("fan-out propagation succeeds");
    assert_eq!(m.connectors[1].attrs.as_real().unwrap().unit(), "K");
    assert_eq!(m.connectors[2].attrs.as_real().unwrap().unit(), "K");
}

#[test]
fn t19_fanout_with_disagreeing_inputs_is_a_conflict() {
    // Output unset; two driven inputs declare different units → the star has 2 distinct values.
    // A naive single-sweep would blame whichever connection it visited last; the cluster algorithm
    // reports it deterministically regardless of connection order.
    let mut m = ModelGraph {
        connectors: vec![
            real_unit(0, 0, Dir::Out, None),
            real_unit(1, 1, Dir::In, Some("K")),
            real_unit(2, 2, Dir::In, Some("degC")),
        ],
        connections: vec![conn_edge(0, 1), conn_edge(0, 2)],
        ..ModelGraph::new()
    };
    let err = unify_attributes(&mut m).expect_err("disagreeing fan-out must fail");
    assert_eq!(
        codes(&err.diagnostics),
        vec![DiagCode::UnitQuantityMismatch]
    );
    // No-mutation on conflict: the unset output stays unset (not clobbered by either branch value),
    // and each driven input keeps its own declared unit.
    assert_eq!(m.connectors[0].attrs.as_real().unwrap().unit(), "");
    assert_eq!(m.connectors[1].attrs.as_real().unwrap().unit(), "K");
    assert_eq!(m.connectors[2].attrs.as_real().unwrap().unit(), "degC");
}

#[test]
fn t20_exact_string_match_no_normalization_tripwire() {
    // "K" vs "K " (trailing space) MUST be a conflict — no trim/normalize/casefold.
    let mut m = ModelGraph {
        connectors: vec![
            real_unit(0, 0, Dir::Out, Some("K")),
            real_unit(1, 1, Dir::In, Some("K ")),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    };
    let err = unify_attributes(&mut m).expect_err("'K' != 'K ' — exact comparison");
    assert_eq!(
        codes(&err.diagnostics),
        vec![DiagCode::UnitQuantityMismatch]
    );
}

// ---- T21–T22: R13.3 displayUnit divergence (warning, never error) ---------------------------

#[test]
fn t21_divergent_display_unit_is_a_warning_not_an_error() {
    let mut m = ModelGraph {
        connectors: vec![
            real_conn(
                0,
                0,
                Dir::Out,
                RealAttrs {
                    display_unit: Some(Arc::from("degC")),
                    ..RealAttrs::default()
                },
            ),
            real_conn(
                1,
                1,
                Dir::In,
                RealAttrs {
                    display_unit: Some(Arc::from("K")),
                    ..RealAttrs::default()
                },
            ),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    };
    let warnings = unify_attributes(&mut m).expect("displayUnit divergence is advisory, not fatal");
    assert_eq!(codes(&warnings), vec![DiagCode::DisplayUnitDivergence]);
    assert_eq!(warnings[0].severity, Severity::Warning);
}

#[test]
fn t22_matching_display_unit_emits_no_warning() {
    let mut m = ModelGraph {
        connectors: vec![
            real_conn(
                0,
                0,
                Dir::Out,
                RealAttrs {
                    display_unit: Some(Arc::from("degC")),
                    ..RealAttrs::default()
                },
            ),
            real_conn(
                1,
                1,
                Dir::In,
                RealAttrs {
                    display_unit: Some(Arc::from("degC")),
                    ..RealAttrs::default()
                },
            ),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    };
    assert!(
        unify_attributes(&mut m)
            .expect("matching displayUnit")
            .is_empty()
    );
}

// ---- T24: tag-invariant violation is skipped (panic-free) -----------------------------------

#[test]
fn t24_tag_invariant_violation_is_skipped_not_unwrapped() {
    // A struct-literal connector with value_type=Real but Attrs::Integer (bypasses with_attrs).
    // as_real() yields None → §7.10 skips it; no unwrap, no panic.
    let bad = Connector {
        id: ConnectorId(0),
        block: BlockId(0),
        dir: Dir::Out,
        value_type: ValueType::Real,
        attrs: Attrs::Integer(IntAttrs::default()),
        decl_order: 0,
        iri: None,
    };
    let mut m = ModelGraph {
        connectors: vec![bad, real_unit(1, 1, Dir::In, Some("K"))],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    };
    // C0 contributes no unit (tag mismatch → skipped); C1's "K" is the lone declared value, so it
    // is the only cluster member with a real attr slot — propagation is a no-op, no conflict.
    let warnings = unify_attributes(&mut m).expect("tag-mismatch connector is skipped, not fatal");
    assert!(warnings.is_empty());
}

// ---- T29: §7.10 no-overwrite guard (Arc identity) -------------------------------------------

#[test]
fn t29_propagation_does_not_overwrite_already_set_peer() {
    // C0(Out,"K") drives C1(unset) and C2("K" — same CONTENT, a DISTINCT Arc). Propagation fills C1
    // but must NOT replace C2's already-set slot (the `is_none()` guard). Proven by Arc pointer
    // identity: a dropped guard would replace C2's Arc with C0's. (T14 alone cannot show this — its
    // declarer re-affirms its own value.)
    let c2_unit: Arc<str> = Arc::from("K");
    let mut m = ModelGraph {
        connectors: vec![
            real_unit(0, 0, Dir::Out, Some("K")),
            real_unit(1, 1, Dir::In, None),
            real_conn(
                2,
                2,
                Dir::In,
                RealAttrs {
                    unit: Some(Arc::clone(&c2_unit)),
                    ..RealAttrs::default()
                },
            ),
        ],
        connections: vec![conn_edge(0, 1), conn_edge(0, 2)],
        ..ModelGraph::new()
    };
    unify_attributes(&mut m).expect("same-content cluster unifies");
    assert_eq!(m.connectors[1].attrs.as_real().unwrap().unit(), "K"); // C1 filled by R13.2
    let c2_after = m.connectors[2]
        .attrs
        .as_real()
        .unwrap()
        .unit
        .clone()
        .unwrap();
    assert!(
        Arc::ptr_eq(&c2_unit, &c2_after),
        "an already-set peer must not be overwritten (is_none guard)"
    );
}

// ---- T35–T38: §7.10 min/max bound unification (R13.1/R13.2 for min/max) ----------------------

#[test]
fn t35_conflicting_real_min_bound_is_a_bound_mismatch() {
    // R13.1 applies to min/max identically to unit/quantity (doc 02 §9). min 0.0 vs 5.0 → conflict.
    let mut m = ModelGraph {
        connectors: vec![
            real_conn(
                0,
                0,
                Dir::Out,
                RealAttrs {
                    min: Some(0.0),
                    ..RealAttrs::default()
                },
            ),
            real_conn(
                1,
                1,
                Dir::In,
                RealAttrs {
                    min: Some(5.0),
                    ..RealAttrs::default()
                },
            ),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    };
    let err = unify_attributes(&mut m).expect_err("conflicting min bounds must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::BoundMismatch]);
    assert!(err.diagnostics[0].message.contains("min"));
    // No-mutation: both bounds stay as authored (compared by bits per the float convention).
    assert_eq!(
        m.connectors[0].attrs.as_real().unwrap().min().to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(
        m.connectors[1].attrs.as_real().unwrap().min().to_bits(),
        5.0_f64.to_bits()
    );
}

#[test]
fn t38_real_bound_signed_zero_is_a_conflict() {
    // PROOF tripwire for the bit-exact numeric path: min 0.0 vs -0.0 have identical IEEE value
    // (0.0 == -0.0) but DISTINCT bit patterns. With to_bits these are a conflict; a regression to
    // `==` would FALSELY agree and silently unify. Mirrors T20 for the string path.
    let mut m = ModelGraph {
        connectors: vec![
            real_conn(
                0,
                0,
                Dir::Out,
                RealAttrs {
                    min: Some(0.0),
                    ..RealAttrs::default()
                },
            ),
            real_conn(
                1,
                1,
                Dir::In,
                RealAttrs {
                    min: Some(-0.0),
                    ..RealAttrs::default()
                },
            ),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    };
    let err = unify_attributes(&mut m).expect_err("0.0 vs -0.0 differ by bits -> conflict");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::BoundMismatch]);
}

#[test]
fn t36_one_sided_real_max_bound_propagates() {
    // R13.2: output declares max=100.0, input unset → propagate the bound to the input.
    let mut m = ModelGraph {
        connectors: vec![
            real_conn(
                0,
                0,
                Dir::Out,
                RealAttrs {
                    max: Some(100.0),
                    ..RealAttrs::default()
                },
            ),
            real_unit(1, 1, Dir::In, None),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    };
    assert!(
        unify_attributes(&mut m)
            .expect("one-sided max propagates")
            .is_empty()
    );
    assert_eq!(
        m.connectors[1].attrs.as_real().unwrap().max().to_bits(),
        100.0_f64.to_bits()
    );
}

#[test]
fn t37_integer_bounds_conflict_and_propagate() {
    // Integer carries no unit/quantity (§3.1 matrix) but min/max ARE unified (R13.1/R13.2).
    // Conflict: min 1 vs 7 → BoundMismatch.
    let mut conflict = ModelGraph {
        connectors: vec![
            int_conn(
                0,
                0,
                Dir::Out,
                IntAttrs {
                    min: Some(1),
                    max: None,
                },
            ),
            int_conn(
                1,
                1,
                Dir::In,
                IntAttrs {
                    min: Some(7),
                    max: None,
                },
            ),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    };
    let err = unify_attributes(&mut conflict).expect_err("int min conflict must fail");
    assert_eq!(codes(&err.diagnostics), vec![DiagCode::BoundMismatch]);

    // Propagation: one-sided max=10 → input receives it.
    let mut prop = ModelGraph {
        connectors: vec![
            int_conn(
                0,
                0,
                Dir::Out,
                IntAttrs {
                    min: None,
                    max: Some(10),
                },
            ),
            int_conn(1, 1, Dir::In, IntAttrs::default()),
        ],
        connections: vec![conn_edge(0, 1)],
        ..ModelGraph::new()
    };
    assert!(
        unify_attributes(&mut prop)
            .expect("int max propagates")
            .is_empty()
    );
    assert_eq!(prop.connectors[1].attrs.as_integer().unwrap().max(), 10);
}

#[test]
fn boundary_alias_and_connector_attrs_propagate_across_the_whole_cluster() {
    let mut model = ModelGraph {
        connectors: vec![
            real_conn(
                0,
                0,
                Dir::Out,
                RealAttrs {
                    unit: Some(Arc::from("K")),
                    min: Some(200.0),
                    ..RealAttrs::default()
                },
            ),
            real_conn(1, 1, Dir::In, RealAttrs::default()),
        ],
        connections: vec![conn_edge(0, 1)],
        boundary_outputs: vec![boundary_output(
            "urn:test:boundary",
            0,
            Attrs::Real(RealAttrs {
                quantity: Some(Arc::from("ThermodynamicTemperature")),
                max: Some(330.0),
                ..RealAttrs::default()
            }),
        )],
        ..ModelGraph::new()
    };

    assert!(
        unify_attributes(&mut model)
            .expect("boundary alias cluster unifies")
            .is_empty()
    );
    for attrs in [
        &model.connectors[0].attrs,
        &model.connectors[1].attrs,
        &model.boundary_outputs[0].attrs,
    ] {
        let real = attrs.as_real().expect("Real cluster member");
        assert_eq!(real.unit(), "K");
        assert_eq!(real.quantity(), "ThermodynamicTemperature");
        assert_eq!(real.min().to_bits(), 200.0_f64.to_bits());
        assert_eq!(real.max().to_bits(), 330.0_f64.to_bits());
    }
}

#[test]
fn boundary_alias_unification_scales_linearly_with_alias_count() {
    const COUNT: u32 = 50_000;
    let mut model = ModelGraph {
        connectors: (0..COUNT)
            .map(|id| real_unit(id, id, Dir::Out, Some("K")))
            .collect(),
        boundary_outputs: (0..COUNT)
            .map(|id| {
                boundary_output(
                    &format!("urn:test:boundary:{id}"),
                    id,
                    Attrs::Real(RealAttrs::default()),
                )
            })
            .collect(),
        ..ModelGraph::new()
    };

    unify_attributes(&mut model).expect("large boundary alias graph unifies");
    assert_eq!(model.boundary_outputs.len(), COUNT as usize);
    for index in [0, COUNT as usize / 2, COUNT as usize - 1] {
        assert_eq!(
            model.boundary_outputs[index]
                .attrs
                .as_real()
                .unwrap()
                .unit(),
            "K"
        );
    }
}

#[test]
fn boundary_alias_conflict_is_hard_error_without_partial_rewrite() {
    let mut model = ModelGraph {
        connectors: vec![real_unit(0, 0, Dir::Out, Some("K"))],
        boundary_outputs: vec![boundary_output(
            "urn:test:boundary",
            0,
            Attrs::Real(RealAttrs {
                unit: Some(Arc::from("Pa")),
                ..RealAttrs::default()
            }),
        )],
        ..ModelGraph::new()
    };

    let error = unify_attributes(&mut model).expect_err("boundary unit conflict must fail");
    assert_eq!(
        codes(&error.diagnostics),
        vec![DiagCode::UnitQuantityMismatch]
    );
    assert!(error.diagnostics[0].message.contains("[\"K\", \"Pa\"]"));
    assert_eq!(model.connectors[0].attrs.as_real().unwrap().unit(), "K");
    assert_eq!(
        model.boundary_outputs[0].attrs.as_real().unwrap().unit(),
        "Pa"
    );
}

#[test]
fn later_attribute_conflict_rolls_back_earlier_propagation() {
    let mut model = ModelGraph {
        connectors: vec![real_conn(
            0,
            0,
            Dir::Out,
            RealAttrs {
                quantity: Some(Arc::from("ThermodynamicTemperature")),
                ..RealAttrs::default()
            },
        )],
        boundary_outputs: vec![boundary_output(
            "urn:test:boundary",
            0,
            Attrs::Real(RealAttrs {
                unit: Some(Arc::from("K")),
                quantity: Some(Arc::from("Pressure")),
                ..RealAttrs::default()
            }),
        )],
        ..ModelGraph::new()
    };

    let error = unify_attributes(&mut model).expect_err("quantity conflict must fail");
    assert_eq!(
        codes(&error.diagnostics),
        vec![DiagCode::UnitQuantityMismatch]
    );
    assert!(model.connectors[0].attrs.as_real().unwrap().unit.is_none());
}

#[test]
fn later_cluster_conflict_rolls_back_earlier_cluster_propagation() {
    let mut model = ModelGraph {
        connectors: vec![
            real_unit(0, 0, Dir::Out, Some("K")),
            real_unit(1, 1, Dir::Out, Some("K")),
        ],
        boundary_outputs: vec![
            boundary_output(
                "urn:test:propagated-first",
                0,
                Attrs::Real(RealAttrs::default()),
            ),
            boundary_output(
                "urn:test:conflict-second",
                1,
                Attrs::Real(RealAttrs {
                    unit: Some(Arc::from("Pa")),
                    ..RealAttrs::default()
                }),
            ),
        ],
        ..ModelGraph::new()
    };

    unify_attributes(&mut model).expect_err("later cluster conflict must fail");
    assert!(
        model.boundary_outputs[0]
            .attrs
            .as_real()
            .unwrap()
            .unit
            .is_none()
    );
}

#[test]
fn integer_bounds_propagate_between_source_and_boundary_alias() {
    let mut model = ModelGraph {
        connectors: vec![int_conn(
            0,
            0,
            Dir::Out,
            IntAttrs {
                min: Some(1),
                max: None,
            },
        )],
        boundary_outputs: vec![boundary_output(
            "urn:test:integer-boundary",
            0,
            Attrs::Integer(IntAttrs {
                min: None,
                max: Some(10),
            }),
        )],
        ..ModelGraph::new()
    };

    unify_attributes(&mut model).expect("Integer boundary bounds unify");
    let source = model.connectors[0].attrs.as_integer().unwrap();
    let boundary = model.boundary_outputs[0].attrs.as_integer().unwrap();
    assert_eq!((source.min(), source.max()), (1, 10));
    assert_eq!((boundary.min(), boundary.max()), (1, 10));
}

#[test]
fn boundary_display_unit_divergence_is_advisory() {
    let mut model = ModelGraph {
        connectors: vec![real_conn(
            0,
            0,
            Dir::Out,
            RealAttrs {
                display_unit: Some(Arc::from("K")),
                ..RealAttrs::default()
            },
        )],
        boundary_outputs: vec![boundary_output(
            "urn:test:boundary",
            0,
            Attrs::Real(RealAttrs {
                display_unit: Some(Arc::from("degC")),
                ..RealAttrs::default()
            }),
        )],
        ..ModelGraph::new()
    };

    let warnings = unify_attributes(&mut model).expect("displayUnit remains advisory");
    assert_eq!(codes(&warnings), vec![DiagCode::DisplayUnitDivergence]);
    assert_eq!(warnings[0].severity, Severity::Warning);
}

#[test]
fn boundary_alias_order_does_not_change_conflict_diagnostics() {
    let aliases = || {
        vec![
            boundary_output(
                "urn:test:z",
                0,
                Attrs::Real(RealAttrs {
                    unit: Some(Arc::from("K")),
                    ..RealAttrs::default()
                }),
            ),
            boundary_output(
                "urn:test:a",
                0,
                Attrs::Real(RealAttrs {
                    unit: Some(Arc::from("Pa")),
                    ..RealAttrs::default()
                }),
            ),
        ]
    };
    let mut first = ModelGraph {
        connectors: vec![real_unit(0, 0, Dir::Out, None)],
        boundary_outputs: aliases(),
        ..ModelGraph::new()
    };
    let mut second = first.clone();
    second.boundary_outputs.reverse();

    let first_error = unify_attributes(&mut first).expect_err("alias conflict");
    let second_error = unify_attributes(&mut second).expect_err("permuted alias conflict");
    assert_eq!(first_error.diagnostics, second_error.diagnostics);
}

#[test]
fn malformed_boundary_aliases_are_panic_free() {
    let mut model = ModelGraph {
        connectors: vec![real_unit(0, 0, Dir::Out, Some("K"))],
        boundary_outputs: vec![
            boundary_output(
                "urn:test:out-of-range",
                99,
                Attrs::Real(RealAttrs {
                    unit: Some(Arc::from("Pa")),
                    ..RealAttrs::default()
                }),
            ),
            boundary_output(
                "urn:test:tag-mismatch",
                0,
                Attrs::Integer(IntAttrs {
                    min: Some(1),
                    max: None,
                }),
            ),
        ],
        ..ModelGraph::new()
    };

    assert!(
        unify_attributes(&mut model)
            .expect("malformed aliases remain total")
            .is_empty()
    );
    assert!(matches!(model.boundary_outputs[1].attrs, Attrs::Integer(_)));
}
