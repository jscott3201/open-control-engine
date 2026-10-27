//! Atomic block firing refuses CDL-legal feedback through every registered mixed-feedthrough
//! class (GitHub issue #347).
//!
//! For each of the twelve classes `docs/cdl-coverage.md` lists, a real registry instance has one
//! output fed back, through a pass-through block (a threshold where the input is Boolean), to an
//! input that output does not depend on. The
//! connector graph is acyclic, so CDL §7.16 is satisfied, yet `compile` must refuse with
//! [`BuildError::BlockAlgebraicLoop`] — fail closed, never a mis-ordered schedule. The contrast
//! case feeds the same output back to an input it *does* depend on, which is a true connector-level
//! loop and must be refused as [`BuildError::AlgebraicLoop`] instead.

use super::*;

/// Add a real registry block whose connector arity and value types come from its *resolved*
/// signature, so parameter-width classes get their configured port counts.
fn block_resolved(
    b: &mut ModelBuilder,
    blk: Box<dyn Block>,
) -> (BlockId, Vec<ConnectorId>, Vec<ConnectorId>) {
    let id = BlockId(b.model.blocks.len() as u32);
    let (class_path, input_kinds, output_kinds) = {
        let sig = blk.resolved_signature();
        (sig.class_path, sig.inputs.to_vec(), sig.outputs.to_vec())
    };
    let mut push = |kind: PortKind, dir: Dir| {
        let cid = ConnectorId(b.next_conn);
        let value_type = match kind {
            PortKind::Real => ValueType::Real,
            PortKind::Integer => ValueType::Integer,
            PortKind::Boolean => ValueType::Boolean,
        };
        b.model
            .connectors
            .push(Connector::new(cid, id, dir, value_type, b.next_conn));
        b.next_conn += 1;
        cid
    };
    let inputs: Vec<ConnectorId> = input_kinds.iter().map(|&k| push(k, Dir::In)).collect();
    let outputs: Vec<ConnectorId> = output_kinds.iter().map(|&k| push(k, Dir::Out)).collect();
    b.model.blocks.push(BlockInstance {
        id,
        class_iri: Arc::from(class_path),
        inputs: inputs.clone(),
        outputs: outputs.clone(),
        params: ParamTable::default(),
        decl_order: id.0,
        instance_iri: None,
    });
    b.blocks.push(blk);
    (id, inputs, outputs)
}

/// A one-input, one-output feedthrough block from port type `from` to port type `to`.
fn pass_through(from: PortKind, to: PortKind) -> Box<dyn Block> {
    match (from, to) {
        (PortKind::Real, PortKind::Real) => {
            make("CDL.Reals.MultiplyByParameter", &[("k", Value::Real(0.5))])
        }
        (PortKind::Integer, PortKind::Integer) => make("CDL.Integers.Abs", &[]),
        (PortKind::Boolean, PortKind::Boolean) => make("CDL.Logical.Not", &[]),
        (PortKind::Real, PortKind::Boolean) => {
            make("CDL.Reals.GreaterThreshold", &[("t", Value::Real(0.5))])
        }
        other => panic!("no pass-through fixture for {other:?}"),
    }
}

fn controller(member: &str) -> Value {
    Value::String(Arc::from(format!(
        "Buildings.Controls.OBC.CDL.Types.SimpleController.{member}"
    )))
}

/// One mixed-feedthrough parameterization: class, parameters, and the `(input, output)` pair
/// (0-based) whose output does not depend on that input.
struct Case {
    class_path: &'static str,
    params: Vec<(&'static str, Value)>,
    cut_pair: (usize, usize),
    feeding_pair: (usize, usize),
}

fn cases() -> Vec<Case> {
    let routing = |[extract, filter, replicator]: [&'static str; 3]| {
        let two = || vec![("nin", Value::Integer(2)), ("nout", Value::Integer(2))];
        [
            // y1 = u1, y2 = u2: y1 does not depend on u2.
            Case {
                class_path: extract,
                params: two(),
                cut_pair: (1, 0),
                feeding_pair: (0, 0),
            },
            // msk = {true, true}: y1 = u1, y2 = u2.
            Case {
                class_path: filter,
                params: two(),
                cut_pair: (1, 0),
                feeding_pair: (0, 0),
            },
            // y[r, c] = u[c]: y[1, 1] does not depend on u2.
            Case {
                class_path: replicator,
                params: vec![("nin", Value::Integer(2)), ("nout", Value::Integer(1))],
                cut_pair: (1, 0),
                feeding_pair: (0, 0),
            },
        ]
    };
    let mut cases = Vec::new();
    cases.extend(routing([
        "CDL.Routing.RealExtractSignal",
        "CDL.Routing.RealVectorFilter",
        "CDL.Routing.RealVectorReplicator",
    ]));
    cases.extend(routing([
        "CDL.Routing.IntegerExtractSignal",
        "CDL.Routing.IntegerVectorFilter",
        "CDL.Routing.IntegerVectorReplicator",
    ]));
    cases.extend(routing([
        "CDL.Routing.BooleanExtractSignal",
        "CDL.Routing.BooleanVectorFilter",
        "CDL.Routing.BooleanVectorReplicator",
    ]));
    // Row reduction of a 2x1 matrix: y1 = min(u[1, 1]) does not depend on u[2, 1].
    cases.push(Case {
        class_path: "CDL.Reals.MatrixMin",
        params: vec![
            ("nRow", Value::Integer(2)),
            ("nCol", Value::Integer(1)),
            ("rowMin", Value::Boolean(true)),
        ],
        cut_pair: (1, 0),
        feeding_pair: (0, 0),
    });
    // Column reduction of a 1x2 matrix: y1 = max(u[1, 1]) does not depend on u[1, 2].
    cases.push(Case {
        class_path: "CDL.Reals.MatrixMax",
        params: vec![
            ("nRow", Value::Integer(1)),
            ("nCol", Value::Integer(2)),
            ("rowMax", Value::Boolean(false)),
        ],
        cut_pair: (1, 0),
        feeding_pair: (0, 0),
    });
    // The other two matrix shapes, so both classes are refused on both axes.
    cases.push(Case {
        class_path: "CDL.Reals.MatrixMax",
        params: vec![
            ("nRow", Value::Integer(2)),
            ("nCol", Value::Integer(2)),
            ("rowMax", Value::Boolean(true)),
        ],
        // Row 2 is inputs 2 and 3; y1 reduces row 1 only.
        cut_pair: (3, 0),
        feeding_pair: (1, 0),
    });
    cases.push(Case {
        class_path: "CDL.Reals.MatrixMin",
        params: vec![
            ("nRow", Value::Integer(2)),
            ("nCol", Value::Integer(2)),
            ("rowMin", Value::Boolean(false)),
        ],
        // Column 2 is inputs 1 and 3; y1 reduces column 1 only.
        cut_pair: (3, 0),
        feeding_pair: (2, 0),
    });
    // A reset PID in P mode holds no state, so it is [A] and reads `trigger` at emit time, yet
    // `y` depends only on `u_s` and `u_m`. Inputs: u_s, u_m, trigger.
    cases.push(Case {
        class_path: "CDL.Reals.PIDWithReset",
        params: vec![("controllerType", controller("P"))],
        cut_pair: (2, 0),
        feeding_pair: (0, 0),
    });
    cases
}

/// Wire `class.out[pair.1] → pass → class.in[pair.0]` and compile.
fn compile_feedback(
    case: &Case,
    pair: (usize, usize),
) -> (ModelBuilder, Result<Schedule, BuildError>) {
    let mut b = ModelBuilder::default();
    let (input, output) = pair;
    let blk = make(case.class_path, &case.params);
    let (from, to) = {
        let sig = blk.resolved_signature();
        (sig.outputs[output], sig.inputs[input])
    };
    let (_, ins, outs) = block_resolved(&mut b, blk);
    let (_, pass_in, pass_out) = block_resolved(&mut b, pass_through(from, to));
    assert!(
        b.blocks[0].feeds_through(input, output) == (pair == case.feeding_pair),
        "{}: fixture pair {pair:?} has the wrong feedthrough",
        case.class_path
    );
    b.connect(outs[output], pass_in[0]);
    b.connect(pass_out[0], ins[input]);
    let result = compile(&b.model, &b.blocks);
    (b, result)
}

#[test]
fn feedback_into_an_input_the_output_skips_is_refused_as_a_block_loop() {
    let cases = cases();
    let classes: std::collections::BTreeSet<&str> = cases.iter().map(|c| c.class_path).collect();
    assert_eq!(classes.len(), 12, "every listed class is exercised");
    for case in &cases {
        let (b, result) = compile_feedback(case, case.cut_pair);
        let dag = build_feedthrough_dag(&b.model, &b.blocks);
        assert!(
            reference_order(&dag, &b.model).is_some(),
            "{}: the connector graph must be acyclic (CDL-legal feedback)",
            case.class_path
        );
        let err = result.expect_err("atomic firing cannot schedule this feedback");
        let BuildError::BlockAlgebraicLoop { members } = err else {
            panic!(
                "{}: expected BlockAlgebraicLoop, got {err:?}",
                case.class_path
            )
        };
        let rendered: Vec<String> = members.iter().map(ToString::to_string).collect();
        let pass_class = b.model.blocks[1].class_iri.to_string();
        assert_eq!(
            rendered,
            [
                format!("{}#b0", case.class_path),
                format!("{pass_class}#b1")
            ],
            "{}: cycle members in loop order",
            case.class_path
        );
    }
}

#[test]
fn feedback_into_an_input_the_output_reads_is_a_connector_level_loop() {
    for case in &cases() {
        let (_, result) = compile_feedback(case, case.feeding_pair);
        let err = result.expect_err("a real feedthrough cycle must be refused");
        assert!(
            matches!(err, BuildError::AlgebraicLoop { .. }),
            "{}: expected the connector-level AlgebraicLoop, got {err:?}",
            case.class_path
        );
    }
}

#[test]
fn single_output_reduction_feedback_is_a_connector_level_loop() {
    // MatrixMin with one row reduces every input into its one output, so it is not mixed: any
    // feedback is a true loop and never reaches the block-granularity check.
    let case = Case {
        class_path: "CDL.Reals.MatrixMin",
        params: vec![
            ("nRow", Value::Integer(1)),
            ("nCol", Value::Integer(2)),
            ("rowMin", Value::Boolean(true)),
        ],
        cut_pair: (1, 0),
        feeding_pair: (1, 0),
    };
    let (_, result) = compile_feedback(&case, case.cut_pair);
    assert!(
        matches!(result, Err(BuildError::AlgebraicLoop { .. })),
        "{result:?}"
    );
}

#[test]
fn reset_pid_trigger_feedback_compiles_outside_p_mode() {
    // With an integrator the reset PID is [S] and `trigger` is read only by the state update, so
    // the same feedback is cut and schedules.
    for member in ["PI", "PD", "PID"] {
        let case = Case {
            class_path: "CDL.Reals.PIDWithReset",
            params: vec![("controllerType", controller(member))],
            cut_pair: (2, 0),
            feeding_pair: (0, 0),
        };
        let (_, result) = compile_feedback(&case, case.cut_pair);
        let schedule = result.unwrap_or_else(|err| panic!("{member}: {err:?}"));
        assert_eq!(schedule.order, [BlockId(0), BlockId(1)], "{member}");
    }
}

#[test]
fn each_class_without_the_feedback_compiles() {
    // Control: the refused instances are schedulable once the fed-back connection is removed, so
    // the refusal comes from the feedback, not from the class or its parameters.
    for case in &cases() {
        let mut b = ModelBuilder::default();
        let blk = make(case.class_path, &case.params);
        let from = blk.resolved_signature().outputs[case.cut_pair.1];
        let (_, _, outs) = block_resolved(&mut b, blk);
        let (_, pass_in, _) = block_resolved(&mut b, pass_through(from, from));
        b.connect(outs[case.cut_pair.1], pass_in[0]);
        let schedule = compile(&b.model, &b.blocks)
            .unwrap_or_else(|err| panic!("{}: {err:?}", case.class_path));
        assert_eq!(
            schedule.order,
            [BlockId(0), BlockId(1)],
            "{}",
            case.class_path
        );
    }
}
