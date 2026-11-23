//! Hand-built graphs from a fixed seed, each exported and compared with its document built in
//! full (issue #803): every graph the plan accepts is written as `crate::write_document` writes
//! that document, and the counting pass counts those bytes. The graphs draw identities, names,
//! values, connector attributes, wiring and boundaries from small pools that include characters
//! JSON escapes, ids that collide, parameters named like ports and values the plan refuses, so
//! accepted and refused plans both occur.

use std::sync::Arc;

use oce_model::{
    Attrs, BlockId, BlockInstance, BoundaryInput, BoundaryOutput, Connection, Connector,
    ConnectorId, Dir, EnumClassId, IntAttrs, ModelGraph, ParamTable, RealAttrs, Value, ValueType,
};

use super::super::plan;
use super::tests::assert_written_as_built_in_full;

/// A xorshift generator: deterministic, so every run draws the same graphs.
struct Draw(u64);

impl Draw {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number below `n`.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// Whether a one-in-`n` chance came up.
    fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }

    fn pick<'a, T>(&mut self, pool: &'a [T]) -> &'a T {
        &pool[self.below(pool.len())]
    }
}

const BLOCK_IRIS: &[&str] = &[
    "urn:a",
    "urn:b",
    "urn:a.k",
    "urn:a.in0",
    "urn:\"q\"\\é\u{2028}",
    "urn:\u{1}\u{7f}\u{1F600}",
    "http://example.org#Long.block.with.dots",
];
const PORT_IRIS: &[&str] = &["urn:p1", "urn:p2", "urn:p\"3", "urn:a.in0", "urn:q\u{2029}"];
const BOUNDARY_IRIS: &[&str] = &["urn:u1", "urn:u\\2", "urn:a"];
const NAMES: &[&str] = &[
    "k",
    "k_1",
    "K_1_2",
    "in0",
    "out0",
    "n\u{2028}",
    "s\"\\",
    "n\u{1F600}",
    "",
    "a.b",
];
const REALS: &[f64] = &[
    0.0,
    -0.0,
    1.0,
    0.1,
    1e300,
    1e-300,
    5e-324,
    f64::MAX,
    -7.25e21,
];
const STRINGS: &[&str] = &["", "a\"b", "\\", "\u{0}\n", "é\u{1F600}\u{2028}"];

/// A spelling from `pool`: now and then as it is, so that spellings collide, otherwise made
/// distinct by `n`.
fn spelling(d: &mut Draw, pool: &[&str], n: usize) -> String {
    let base = *d.pick(pool);
    if d.one_in(8) {
        base.to_owned()
    } else {
        format!("{base}{n}")
    }
}

/// One parameter value: mostly bindable, now and then one the plan refuses or defers.
fn value(d: &mut Draw) -> Value {
    match d.below(20) {
        0 => Value::Real(f64::NAN),
        1 => Value::Enum {
            class: EnumClassId(0),
            ordinal: 1,
        },
        2..=7 => Value::Real(*d.pick(REALS)),
        8..=10 => Value::Integer(*d.pick(&[i64::MIN, 0, 42, i64::MAX])),
        11..=13 => Value::Boolean(d.one_in(2)),
        _ => Value::String(Arc::from(*d.pick(STRINGS))),
    }
}

/// The attributes of a connector of `value_type`: none, or a set the exporter writes.
fn attrs(d: &mut Draw, value_type: ValueType) -> Attrs {
    match value_type {
        ValueType::Real if d.one_in(3) => Attrs::Real(RealAttrs {
            unit: d.one_in(2).then(|| Arc::from("K")),
            quantity: d.one_in(3).then(|| Arc::from("ThermodynamicTemperature")),
            display_unit: d.one_in(3).then(|| Arc::from("degC")),
            min: d.one_in(2).then(|| *d.pick(REALS)),
            max: d.one_in(2).then(|| *d.pick(REALS)),
            nominal: d.one_in(3).then(|| *d.pick(REALS)),
            unbounded: None,
        }),
        ValueType::Integer if d.one_in(3) => Attrs::Integer(IntAttrs {
            min: Some(-3),
            max: d.one_in(2).then_some(i64::MAX),
        }),
        other => Attrs::default_for(other),
    }
}

/// One hand-built graph drawn from `d`.
fn graph(d: &mut Draw) -> ModelGraph {
    let mut g = ModelGraph::new();
    let types = [ValueType::Real, ValueType::Integer, ValueType::Boolean];
    for bi in 0..1 + d.below(5) {
        let id = BlockId(bi as u32);
        let mut block = BlockInstance {
            id,
            class_iri: Arc::from("CDL.Reals.Sources.Constant"),
            inputs: Vec::new(),
            outputs: Vec::new(),
            params: ParamTable {
                values: (0..d.below(6))
                    .map(|j| (Arc::from(spelling(d, NAMES, j)), value(d)))
                    .collect(),
            },
            decl_order: id.0,
            instance_iri: (!d.one_in(25)).then(|| Arc::from(spelling(d, BLOCK_IRIS, bi))),
        };
        // Now and then a block of 12 ports a side, so minted ids reach two-digit positions.
        let wide = d.one_in(12);
        let count = |d: &mut Draw| if wide { 12 } else { d.below(3) };
        for (dir, count) in [(Dir::In, count(d)), (Dir::Out, count(d))] {
            for k in 0..count {
                let cid = ConnectorId(g.connectors.len() as u32);
                let value_type = *d.pick(&types);
                let mut c = Connector::new(cid, id, dir, value_type, k as u32);
                c.iri = d
                    .one_in(2)
                    .then(|| Arc::from(spelling(d, PORT_IRIS, cid.0 as usize)));
                c.attrs = attrs(d, value_type);
                g.connectors.push(c);
                match dir {
                    Dir::In => block.inputs.push(cid),
                    Dir::Out => block.outputs.push(cid),
                }
            }
        }
        g.blocks.push(block);
    }
    let inputs: Vec<&Connector> = g.connectors.iter().filter(|c| c.dir == Dir::In).collect();
    let outputs: Vec<&Connector> = g.connectors.iter().filter(|c| c.dir == Dir::Out).collect();
    let mut connections = Vec::new();
    let mut external = Vec::new();
    let mut boundary_inputs = Vec::new();
    let mut boundary_outputs = Vec::new();
    for input in &inputs {
        if !outputs.is_empty() && d.one_in(2) {
            connections.push(Connection {
                from: d.pick(&outputs).id,
                to: input.id,
            });
        } else if d.one_in(2) {
            // A boundary-driven input, named by a boundary IRI, sometimes with a declaration.
            let iri: Arc<str> = Arc::from(*d.pick(BOUNDARY_IRIS));
            external.push((input.id, iri.clone()));
            if d.one_in(4) {
                boundary_inputs.push(BoundaryInput {
                    iri,
                    attrs: Attrs::default_for(input.value_type),
                });
            }
        }
    }
    for output in &outputs {
        if d.one_in(4) {
            boundary_outputs.push(BoundaryOutput {
                iri: Arc::from(*d.pick(BOUNDARY_IRIS)),
                source: output.id,
                attrs: Attrs::default_for(output.value_type),
            });
        }
    }
    for (cid, iri) in external {
        g.connectors[cid.0 as usize].iri = Some(iri);
        g.external_inputs.push(cid);
    }
    g.connections = connections;
    g.boundary_inputs = boundary_inputs;
    g.boundary_outputs = boundary_outputs;
    g
}

#[test]
fn graphs_drawn_from_a_seed_are_written_as_built_in_full() {
    let mut d = Draw(0x9e37_79b9_7f4a_7c15);
    let (mut accepted, mut refused, mut params) = (0, 0, 0);
    let (mut with_boundary, mut with_output, mut with_strings, mut wide) = (0, 0, 0, 0);
    for i in 0..4_000 {
        let g = graph(&mut d);
        if plan(&g).is_err() {
            refused += 1;
            continue;
        }
        accepted += 1;
        with_boundary += usize::from(!g.external_inputs.is_empty());
        with_output += usize::from(!g.boundary_outputs.is_empty());
        with_strings += usize::from(g.blocks.iter().any(|b| {
            b.params
                .values
                .iter()
                .any(|(_, v)| matches!(v, Value::String(_)))
        }));
        wide += usize::from(g.blocks.iter().any(|b| b.inputs.len() > 10));
        params += assert_written_as_built_in_full(&g, &format!("graph {i}"));
    }
    // Floors, so a generator that stops drawing a shape cannot pass by comparing nothing.
    // [measured: 1,058 accepted and 2,942 refused; 3,290 parameters written; 440 with a boundary
    // input, 415 with a boundary output, 710 with a string, 62 with ports at two-digit positions]
    assert!(
        accepted >= 1_000 && refused >= 1_000,
        "{accepted} accepted, {refused} refused"
    );
    assert!(params >= 1_000, "{params} parameters written");
    assert!(wide >= 30, "{wide} with ports at two-digit positions");
    assert!(
        with_boundary >= 50 && with_output >= 50 && with_strings >= 50,
        "{with_boundary} with a boundary input, {with_output} with a boundary output, \
         {with_strings} with a string"
    );
}
