use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OracleError {
    ConnectorCycle,
    BlockCycle,
}

struct OracleSchedule {
    order: Vec<BlockId>,
    connector_order: Vec<ConnectorId>,
}

fn reference_schedule(
    model: &Model,
    blocks: &[Box<dyn Block>],
) -> Result<OracleSchedule, OracleError> {
    let dag = build_feedthrough_dag(model, blocks);
    let connector_order = reference_order(&dag, model).ok_or(OracleError::ConnectorCycle)?;
    let order = reference_block_order(model, blocks).ok_or(OracleError::BlockCycle)?;
    Ok(OracleSchedule {
        order,
        connector_order,
    })
}

fn reference_block_order(model: &Model, blocks: &[Box<dyn Block>]) -> Option<Vec<BlockId>> {
    let n = model.blocks.len();
    let mut succ = vec![Vec::<u32>::new(); n];
    let mut indeg = vec![0u32; n];

    for conn in &model.connections {
        if !reference_emit_consumes(model, blocks, conn.to) {
            continue;
        }
        let producer = model.connectors[conn.from.0 as usize].block.0;
        let consumer = model.connectors[conn.to.0 as usize].block.0;
        succ[producer as usize].push(consumer);
        indeg[consumer as usize] += 1;
    }

    let mut emitted = vec![false; n];
    let mut order = Vec::with_capacity(n);
    for _ in 0..n {
        let mut best: Option<(u32, u32)> = None;
        for (block_idx, &degree) in indeg.iter().enumerate() {
            if emitted[block_idx] || degree != 0 {
                continue;
            }
            let block_id = block_idx as u32;
            let key = (model.blocks[block_idx].decl_order, block_id);
            if best.is_none_or(|candidate| key < candidate) {
                best = Some(key);
            }
        }

        let Some((_, block_id)) = best else {
            break;
        };
        emitted[block_id as usize] = true;
        order.push(BlockId(block_id));
        for &next in &succ[block_id as usize] {
            indeg[next as usize] -= 1;
        }
    }

    (order.len() == n).then_some(order)
}

fn reference_emit_consumes(model: &Model, blocks: &[Box<dyn Block>], input: ConnectorId) -> bool {
    let connector = &model.connectors[input.0 as usize];
    let block_idx = connector.block.0 as usize;
    let block_impl = blocks[block_idx].as_ref();
    if block_impl.kind() == BlockKind::Algebraic {
        return true;
    }

    let block = &model.blocks[block_idx];
    let mut input_idx = None;
    for (idx, &candidate) in block.inputs.iter().enumerate() {
        if candidate == input {
            input_idx = Some(idx);
            break;
        }
    }

    let Some(input_idx) = input_idx else {
        return false;
    };
    for output_idx in 0..block.outputs.len() {
        if block_impl.feeds_through(input_idx, output_idx) {
            return true;
        }
    }
    false
}

fn assert_matches_reference(name: &str, builder: &ModelBuilder) {
    let oracle = reference_schedule(&builder.model, &builder.blocks)
        .unwrap_or_else(|err| panic!("{name}: reference oracle rejected acyclic case: {err:?}"));
    let schedule = compile(&builder.model, &builder.blocks).unwrap_or_else(|err| {
        panic!("{name}: production scheduler rejected acyclic case: {err:?}")
    });

    assert_eq!(oracle.order, schedule.order, "{name}: block order");
    assert_eq!(
        oracle.connector_order, schedule.connector_order,
        "{name}: connector order"
    );

    let dag = build_feedthrough_dag(&builder.model, &builder.blocks);
    assert!(
        is_valid_topo_order(&dag, &schedule.connector_order),
        "{name}: production connector order is not a valid topo order"
    );
    assert_eq!(
        reference_order(&dag, &builder.model).unwrap(),
        schedule.connector_order,
        "{name}: connector-level oracle disagrees"
    );
}

fn assert_cycle_agreement(name: &str, builder: &ModelBuilder, expected: OracleError) {
    let oracle = reference_schedule(&builder.model, &builder.blocks).map(|_| ());
    assert_eq!(
        oracle,
        Err(expected),
        "{name}: reference oracle cycle classification"
    );

    let production = compile(&builder.model, &builder.blocks);
    match (expected, production) {
        (OracleError::ConnectorCycle, Err(BuildError::AlgebraicLoop { .. })) => {}
        (OracleError::BlockCycle, Err(BuildError::BlockAlgebraicLoop { .. })) => {}
        (expected, other) => panic!("{name}: expected {expected:?}, got {other:?}"),
    }
}

fn chain_case() -> ModelBuilder {
    let mut b = ModelBuilder::default();
    let (_source, _, source_out) = b.block("test.Source", 0, 1, false, false);
    let (_pass, pass_in, pass_out) = b.block("test.Pass", 1, 1, true, false);
    let (_sink, sink_in, _) = b.block("test.Sink", 1, 0, true, false);
    b.connect(source_out[0], pass_in[0]);
    b.connect(pass_out[0], sink_in[0]);
    b
}

fn fan_in_out_case() -> ModelBuilder {
    let mut b = ModelBuilder::default();
    let (_left, _, left_out) = b.block("test.LeftSource", 0, 1, false, false);
    let (_right, _, right_out) = b.block("test.RightSource", 0, 1, false, false);
    let (_join_split, join_in, join_out) = b.block("test.JoinSplit", 2, 2, true, false);
    let (_left_sink, left_sink_in, _) = b.block("test.LeftSink", 1, 0, true, false);
    let (_right_sink, right_sink_in, _) = b.block("test.RightSink", 1, 0, true, false);
    b.connect(left_out[0], join_in[0]);
    b.connect(right_out[0], join_in[1]);
    b.connect(join_out[0], left_sink_in[0]);
    b.connect(join_out[1], right_sink_in[0]);
    b
}

fn diamond_case() -> ModelBuilder {
    let mut b = ModelBuilder::default();
    let (_source, _, source_out) = b.block("test.Source", 0, 1, false, false);
    let (_left, left_in, left_out) = b.block("test.LeftBranch", 1, 1, true, false);
    let (_right, right_in, right_out) = b.block("test.RightBranch", 1, 1, true, false);
    let (_join, join_in, _) = b.block("test.Join", 2, 1, true, false);
    b.connect(source_out[0], left_in[0]);
    b.connect(source_out[0], right_in[0]);
    b.connect(left_out[0], join_in[0]);
    b.connect(right_out[0], join_in[1]);
    b
}

fn diamond_case_with_connections(order: &[usize]) -> ModelBuilder {
    let mut b = ModelBuilder::default();
    let (_source, _, source_out) = b.block("test.Source", 0, 1, false, false);
    let (_left, left_in, left_out) = b.block("test.LeftBranch", 1, 1, true, false);
    let (_right, right_in, right_out) = b.block("test.RightBranch", 1, 1, true, false);
    let (_join, join_in, _) = b.block("test.Join", 2, 1, true, false);
    let edges = [
        (source_out[0], left_in[0]),
        (source_out[0], right_in[0]),
        (left_out[0], join_in[0]),
        (right_out[0], join_in[1]),
    ];
    for &idx in order {
        let (from, to) = edges[idx];
        b.connect(from, to);
    }
    b
}

fn mixed_feedthrough_case() -> ModelBuilder {
    let mut b = ModelBuilder::default();
    let (_consumer, consumer_in, _consumer_out) =
        b.block_mixed("test.Consumer", 1, 1, false, &[(0, 0)]);
    let (_source, _, source_out) = b.block("test.Source", 0, 1, false, false);
    let (_mixed, mixed_in, mixed_out) = b.block_mixed("test.MixedPre", 1, 2, true, &[(0, 1)]);
    b.connect(source_out[0], mixed_in[0]);
    b.connect(mixed_out[0], consumer_in[0]);
    b
}

fn pre_and_unit_delay_loop_cut_case() -> ModelBuilder {
    let mut b = ModelBuilder::default();
    let (_pre, pre_in, pre_out) = b.block_real(make("CDL.Logical.Pre", &[]));
    let (_source, _, source_out) = b.block("test.Source", 0, 1, false, false);
    let (_add, add_in, add_out) = b.block("test.Add", 2, 1, true, false);
    let (_delay, delay_in, delay_out) = b.block_real(make(
        "CDL.Discrete.UnitDelay",
        &[("y_start", Value::Real(0.0))],
    ));

    b.connect(delay_out[0], pre_in[0]);
    b.connect(pre_out[0], add_in[0]);
    b.connect(source_out[0], add_in[1]);
    b.connect(add_out[0], delay_in[0]);
    b
}

fn realistic_ahu_loop_case() -> ModelBuilder {
    realistic_ahu_loop_case_with_connections(&[
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17,
    ])
}

fn realistic_ahu_loop_case_with_connections(order: &[usize]) -> ModelBuilder {
    let mut b = ModelBuilder::default();
    let (_feedback_delay, delay_in, delay_out) = b.block_real(make(
        "CDL.Discrete.UnitDelay",
        &[("y_start", Value::Real(0.25))],
    ));
    let (_setpoint, _, setpoint_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(21.0))],
    ));
    let (_fallback, _, fallback_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(0.15))],
    ));
    let (_occupied, _, occupied_out) = b.block_real(make(
        "CDL.Logical.Sources.Constant",
        &[("k", Value::Boolean(true))],
    ));
    let (_error, error_in, error_out) = b.block_real(make("CDL.Reals.Subtract", &[]));
    let (_cooling_demand, demand_in, demand_out) = b.block_real(make(
        "CDL.Reals.Hysteresis",
        &[
            ("uLow", Value::Real(0.25)),
            ("uHigh", Value::Real(0.75)),
            ("pre_y_start", Value::Boolean(false)),
        ],
    ));
    let (_demand_timer, timer_in, timer_out) =
        b.block_real(make("CDL.Logical.Timer", &[("t", Value::Real(2.0))]));
    let (_timer_trim, trim_in, trim_out) = b.block_real(make(
        "CDL.Reals.MultiplyByParameter",
        &[("k", Value::Real(0.05))],
    ));
    let (_enabled, enabled_in, enabled_out) = b.block_real(make("CDL.Logical.And", &[]));
    let (_controller, controller_in, controller_out) = b.block_real(make(
        "CDL.Reals.PIDWithReset",
        &[
            ("k", Value::Real(0.8)),
            ("Ti", Value::Real(30.0)),
            ("Td", Value::Real(0.1)),
            ("Nd", Value::Real(10.0)),
            ("yMin", Value::Real(0.0)),
            ("yMax", Value::Real(1.0)),
        ],
    ));
    let (_limited, limited_in, limited_out) = b.block_real(make(
        "CDL.Reals.Limiter",
        &[("uMin", Value::Real(0.0)), ("uMax", Value::Real(1.0))],
    ));
    let (_trimmed, trimmed_in, trimmed_out) = b.block_real(make("CDL.Reals.Add", &[]));
    let (_command, command_in, command_out) = b.block_real(make("CDL.Reals.Switch", &[]));

    let edges = [
        (setpoint_out[0], error_in[0]),
        (delay_out[0], error_in[1]),
        (error_out[0], demand_in[0]),
        (demand_out[0], timer_in[0]),
        (timer_out[0], trim_in[0]),
        (occupied_out[0], enabled_in[0]),
        (timer_out[1], enabled_in[1]),
        (setpoint_out[0], controller_in[0]),
        (delay_out[0], controller_in[1]),
        (timer_out[1], controller_in[2]),
        (controller_out[0], limited_in[0]),
        (limited_out[0], trimmed_in[0]),
        (trim_out[0], trimmed_in[1]),
        (trimmed_out[0], command_in[0]),
        (enabled_out[0], command_in[1]),
        (fallback_out[0], command_in[2]),
        // Real feedback edges that must not create same-tick emit dependencies: UnitDelay cuts its
        // input, and PIDWithReset's reset value is a non-feedthrough state-update input.
        (command_out[0], delay_in[0]),
        (command_out[0], controller_in[3]),
    ];
    for &idx in order {
        let (from, to) = edges[idx];
        b.connect(from, to);
    }
    b
}

fn economizer_sequence_case() -> ModelBuilder {
    let mut b = ModelBuilder::default();
    let (_oa_temp, _, oa_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(12.0))],
    ));
    let (_ra_temp, _, ra_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(22.0))],
    ));
    let (_delta, delta_in, delta_out) = b.block_real(make("CDL.Reals.Subtract", &[]));
    let (_favorable, favorable_in, favorable_out) = b.block_real(make(
        "CDL.Reals.LessThreshold",
        &[
            ("t", Value::Real(-2.0)),
            ("h", Value::Real(0.5)),
            ("pre_y_start", Value::Boolean(false)),
        ],
    ));
    let (_delay, delay_in, delay_out) = b.block_real(make(
        "CDL.Logical.TrueDelay",
        &[
            ("delayTime", Value::Real(1.0)),
            ("delayOnInit", Value::Boolean(false)),
        ],
    ));
    let (_clear, _, clear_out) = b.block_real(make(
        "CDL.Logical.Sources.Constant",
        &[("k", Value::Boolean(false))],
    ));
    let (_latched, latched_in, latched_out) = b.block_real(make("CDL.Logical.Latch", &[]));
    let (_open_cmd, _, open_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(0.85))],
    ));
    let (_closed_cmd, _, closed_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(0.10))],
    ));
    let (_damper_switch, switch_in, switch_out) = b.block_real(make("CDL.Reals.Switch", &[]));
    let (_line_x1, _, x1_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(-20.0))],
    ));
    let (_line_f1, _, f1_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(0.0))],
    ));
    let (_line_x2, _, x2_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(20.0))],
    ));
    let (_line_f2, _, f2_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(0.5))],
    ));
    let (_reset_line, line_in, line_out) = b.block_real(make("CDL.Reals.Line", &[]));
    let (_max_cmd, max_in, max_out) = b.block_real(make("CDL.Reals.Max", &[]));
    let (_limited_cmd, limited_in, _) = b.block_real(make(
        "CDL.Reals.Limiter",
        &[("uMin", Value::Real(0.0)), ("uMax", Value::Real(1.0))],
    ));

    b.connect(oa_out[0], delta_in[0]);
    b.connect(ra_out[0], delta_in[1]);
    b.connect(delta_out[0], favorable_in[0]);
    b.connect(favorable_out[0], delay_in[0]);
    b.connect(delay_out[0], latched_in[0]);
    b.connect(clear_out[0], latched_in[1]);
    b.connect(open_out[0], switch_in[0]);
    b.connect(latched_out[0], switch_in[1]);
    b.connect(closed_out[0], switch_in[2]);
    b.connect(x1_out[0], line_in[0]);
    b.connect(f1_out[0], line_in[1]);
    b.connect(x2_out[0], line_in[2]);
    b.connect(f2_out[0], line_in[3]);
    b.connect(oa_out[0], line_in[4]);
    b.connect(switch_out[0], max_in[0]);
    b.connect(line_out[0], max_in[1]);
    b.connect(max_out[0], limited_in[0]);
    b
}

fn connector_cycle_case() -> ModelBuilder {
    let mut b = ModelBuilder::default();
    let (_left, left_in, left_out) = b.block("test.LeftPass", 1, 1, true, false);
    let (_right, right_in, right_out) = b.block("test.RightPass", 1, 1, true, false);
    b.connect(left_out[0], right_in[0]);
    b.connect(right_out[0], left_in[0]);
    b
}

fn block_cycle_case() -> ModelBuilder {
    let mut b = ModelBuilder::default();
    let (_mixed, mixed_in, mixed_out) = b.block_mixed("test.MixedPre", 1, 2, true, &[(0, 1)]);
    let (_pass, pass_in, pass_out) = b.block_mixed("test.Pass", 1, 1, false, &[(0, 0)]);
    b.connect(mixed_out[0], pass_in[0]);
    b.connect(pass_out[0], mixed_in[0]);
    b
}

fn real_connector_cycle_case() -> ModelBuilder {
    let mut b = ModelBuilder::default();
    let (_add, add_in, add_out) = b.block_real(make("CDL.Reals.Add", &[]));
    let (_subtract, subtract_in, subtract_out) = b.block_real(make("CDL.Reals.Subtract", &[]));
    let (_bias, _, bias_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(1.0))],
    ));
    b.connect(add_out[0], subtract_in[0]);
    b.connect(subtract_out[0], add_in[0]);
    b.connect(bias_out[0], add_in[1]);
    b.connect(bias_out[0], subtract_in[1]);
    b
}

#[test]
fn schedule_matches_independent_oracle_for_diverse_acyclic_topologies() {
    let cases = vec![
        ("chain", chain_case()),
        ("fan-in/out", fan_in_out_case()),
        ("diamond", diamond_case()),
        ("mixed-feedthrough", mixed_feedthrough_case()),
        (
            "Pre + UnitDelay loop cut",
            pre_and_unit_delay_loop_cut_case(),
        ),
    ];

    for (name, builder) in cases {
        assert_matches_reference(name, &builder);
    }
}

#[test]
fn schedule_matches_oracle_for_realistic_control_sequences() {
    let cases = vec![
        (
            "realistic AHU loop with UnitDelay cut",
            realistic_ahu_loop_case(),
        ),
        ("realistic economizer sequence", economizer_sequence_case()),
    ];

    for (name, builder) in cases {
        assert_matches_reference(name, &builder);
    }
}

#[test]
fn schedule_is_independent_of_connection_insertion_order_for_tied_diamond() {
    let forward = diamond_case_with_connections(&[0, 1, 2, 3]);
    let reversed = diamond_case_with_connections(&[3, 2, 1, 0]);
    let rotated = diamond_case_with_connections(&[2, 0, 3, 1]);
    let cases = [
        ("forward", forward),
        ("reversed", reversed),
        ("rotated", rotated),
    ];

    let baseline = reference_schedule(&cases[0].1.model, &cases[0].1.blocks)
        .expect("diamond reference schedule");
    assert_eq!(
        baseline.order,
        vec![BlockId(0), BlockId(1), BlockId(2), BlockId(3)],
        "fixture must contain a real block-order tie between the two diamond branches"
    );
    assert_eq!(
        baseline.connector_order,
        vec![
            ConnectorId(0),
            ConnectorId(1),
            ConnectorId(2),
            ConnectorId(3),
            ConnectorId(4),
            ConnectorId(5),
            ConnectorId(6),
            ConnectorId(7),
        ],
        "fixture must contain real connector-order ties after the shared source emits"
    );

    for (name, builder) in &cases {
        let oracle =
            reference_schedule(&builder.model, &builder.blocks).expect("acyclic diamond oracle");
        let schedule = compile(&builder.model, &builder.blocks).unwrap_or_else(|err| {
            panic!("{name}: production scheduler rejected acyclic diamond: {err:?}")
        });

        assert_eq!(oracle.order, baseline.order, "{name}: oracle block order");
        assert_eq!(
            oracle.connector_order, baseline.connector_order,
            "{name}: oracle connector order"
        );
        assert_eq!(
            schedule.order, baseline.order,
            "{name}: production block order"
        );
        assert_eq!(
            schedule.connector_order, baseline.connector_order,
            "{name}: production connector order"
        );
    }
}

#[test]
fn real_control_sequence_schedule_is_independent_of_connection_insertion_order() {
    let forward = realistic_ahu_loop_case_with_connections(&[
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17,
    ]);
    let reversed = realistic_ahu_loop_case_with_connections(&[
        17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
    ]);
    let rotated = realistic_ahu_loop_case_with_connections(&[
        10, 14, 1, 16, 5, 12, 3, 8, 17, 0, 6, 11, 4, 13, 2, 15, 7, 9,
    ]);
    let cases = [
        ("forward", forward),
        ("reversed", reversed),
        ("rotated", rotated),
    ];

    let baseline = reference_schedule(&cases[0].1.model, &cases[0].1.blocks)
        .expect("realistic AHU reference schedule");
    // After the real `CDL.Logical.Timer` emits, both the timer-trim branch and enable branch are
    // ready at once; declaration order must choose `BlockId(7)` before `BlockId(8)`, regardless of
    // which timer-output connection was inserted first.
    assert_eq!(
        baseline.order,
        (0..13).map(BlockId).collect::<Vec<_>>(),
        "fixture must contain real ready-set ties resolved by declaration order"
    );

    for (name, builder) in &cases {
        let oracle = reference_schedule(&builder.model, &builder.blocks)
            .expect("acyclic realistic AHU oracle");
        let schedule = compile(&builder.model, &builder.blocks).unwrap_or_else(|err| {
            panic!("{name}: production scheduler rejected acyclic realistic AHU: {err:?}")
        });

        assert_eq!(oracle.order, baseline.order, "{name}: oracle block order");
        assert_eq!(
            oracle.connector_order, baseline.connector_order,
            "{name}: oracle connector order"
        );
        assert_eq!(
            schedule.order, baseline.order,
            "{name}: production block order"
        );
        assert_eq!(
            schedule.connector_order, baseline.connector_order,
            "{name}: production connector order"
        );

        let dag = build_feedthrough_dag(&builder.model, &builder.blocks);
        assert!(
            is_valid_topo_order(&dag, &schedule.connector_order),
            "{name}: production connector order is not a valid topo order"
        );
    }
}

#[test]
fn cycle_rejection_matches_independent_oracle() {
    assert_cycle_agreement(
        "connector-level algebraic loop",
        &connector_cycle_case(),
        OracleError::ConnectorCycle,
    );
    assert_cycle_agreement(
        "block-granularity algebraic loop",
        &block_cycle_case(),
        OracleError::BlockCycle,
    );
}

#[test]
fn real_connector_cycle_rejection_matches_independent_oracle() {
    assert_cycle_agreement(
        "real Reals.Add/Subtract algebraic loop",
        &real_connector_cycle_case(),
        OracleError::ConnectorCycle,
    );
}
