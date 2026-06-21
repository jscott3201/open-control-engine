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
