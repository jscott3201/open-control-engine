//! The cost of a CXF diff in counted work (`crate::work`), never in elapsed time.
//!
//! The promise: a diff of two topologies stays within a fixed multiple of `n * log2(n)` units for
//! `n` topology elements (blocks, ports, ground parameters, connections and boundaries), with the
//! rename pass running all its rounds or skipped at its cap. It is held at a chain of
//! [`MAX_RENAME_CANDIDATES`] blocks, the most the pass examines, one past it, and at a sixteenth
//! of it; a sixteenfold topology costing less than forty times the work is the control that no
//! step is quadratic.
//!
//! What the count cannot see: the facade's `class_ports` validates and builds each block once,
//! which when the diff succeeds costs as much as the counted ports and before a refusal is one
//! build of at most `MAX_RESOLVED_ARITY` (2^21) ports; and the catalog lookups, bounded by the
//! catalog.

use oce_api::{Engine, Topology};
use oce_author::{
    Assignment, Block, Boundary, Connection, EmitOptions, Endpoint, Identifier, Literal,
    ParamValue, Sequence, SignalType, emit,
};

use crate::data::{MAX_RENAME_CANDIDATES, MAX_RENAME_ROUNDS};
use crate::work::tests::work;
use crate::{Diff, DiffOptions, Note, diff_topology};

fn id(name: &str) -> Identifier {
    Identifier::unchecked(name)
}

/// `length` gain blocks named `<prefix><i>` in a chain from the input `u` to the output `y`,
/// block `i` with the gain `value(i)`, emitted and loaded.
fn chain(prefix: &str, length: usize, value: impl Fn(usize) -> f64) -> Topology {
    let mut model = Sequence::new(id("chain"));
    model.inputs.push(Boundary::new(id("u"), SignalType::Real));
    model.outputs.push(Boundary::new(id("y"), SignalType::Real));
    for i in 0..length {
        let mut block = Block::new(id(&format!("{prefix}{i}")), "CDL.Reals.MultiplyByParameter");
        block.assignments.push(Assignment::new(
            id("k"),
            ParamValue::Literal(Literal::Real(value(i))),
        ));
        model.blocks.push(block);
        let source = if i == 0 {
            "u".to_owned()
        } else {
            format!("{prefix}{}.y", i - 1)
        };
        model.connections.push(Connection::new(
            Endpoint::parse(&source),
            Endpoint::parse(&format!("{prefix}{i}.u")),
        ));
    }
    model.connections.push(Connection::new(
        Endpoint::parse(&format!("{prefix}{}.y", length - 1)),
        Endpoint::parse("y"),
    ));
    let bytes = emit(&model, &EmitOptions::default()).expect("the chain emits");
    let mut engine = Engine::new();
    engine.load_cxf(&bytes).expect("the chain loads");
    engine.topology()
}

/// The elements of a topology the diff reads.
fn elements(topology: &Topology) -> usize {
    topology.blocks.len()
        + topology
            .blocks
            .iter()
            .map(|block| block.inputs.len() + block.outputs.len() + block.params.len())
            .sum::<usize>()
        + topology.connections.len()
        + topology.boundary_inputs.len()
        + topology.boundary_outputs.len()
        + topology.pass_through.len()
}

/// `factor` units for each element of the two topologies and each bit of their element count.
fn bound(factor: u64, base: &Topology, head: &Topology) -> u64 {
    let elements = elements(base) + elements(head);
    let bits = u64::from(usize::BITS - elements.leading_zeros());
    factor * u64::try_from(elements).expect("fits") * bits
}

/// The most a CXF diff may cost, the rename pass's rounds included [measured: at most 2.9 per
/// element and bit without renames and 8.1 with the pass running all its rounds, over the chains
/// below].
const FACTOR: u64 = 16;

fn measured(base: &Topology, head: &Topology) -> (u64, Diff) {
    let (units, diff) = work(|| diff_topology(base, head, &DiffOptions::default()));
    (units, diff.expect("the chains diff"))
}

#[test]
fn two_chains_at_the_rename_cap_diff_within_the_bound() {
    for length in [MAX_RENAME_CANDIDATES, MAX_RENAME_CANDIDATES / 16] {
        let base = chain("b", length, |_| 1.0);
        let cases = [
            ("the same", chain("b", length, |_| 1.0), 0, 0),
            ("retuned", chain("b", length, |_| 2.0), length, 0),
            // Every block renamed, each with a gain of its own, anchored at both ends: the pass
            // runs every round and matches one block from each end per round.
            (
                "renamed",
                chain("h", length, |i| i as f64),
                0,
                2 * MAX_RENAME_ROUNDS,
            ),
        ];
        for (name, head, reparameterized, renamed) in cases {
            let base = if name == "renamed" {
                chain("b", length, |i| i as f64)
            } else {
                base.clone()
            };
            let (units, diff) = measured(&base, &head);
            let count = |kind: &str| {
                let entries = diff.graph.iter();
                entries
                    .filter(|e| format!("{e:?}").starts_with(kind))
                    .count()
            };
            assert_eq!(
                count("Reparameterized"),
                reparameterized,
                "{name} at {length}"
            );
            assert_eq!(diff.summary.blocks.renamed, renamed, "{name} at {length}");
            if name == "renamed" {
                assert_eq!(diff.notes, [Note::NamesDisjoint], "the pass ran");
            }
            let bound = bound(FACTOR, &base, &head);
            assert!(
                units <= bound,
                "{name} at {length}: {units} units, bound {bound}"
            );
        }
    }
}

#[test]
fn past_the_rename_cap_the_pass_is_skipped_and_the_diff_stays_within_the_bound() {
    let length = MAX_RENAME_CANDIDATES + 1;
    let (base, head) = (chain("b", length, |_| 1.0), chain("h", length, |_| 1.0));
    let (units, diff) = measured(&base, &head);
    assert_eq!(diff.notes, [Note::NamesDisjoint, Note::RenamePassSkipped]);
    let bound = bound(FACTOR, &base, &head);
    assert!(units <= bound, "{units} units, bound {bound}");
}

#[test]
fn sixteen_times_the_elements_cost_less_than_forty_times_the_work() {
    // The control for the bound above: a step quadratic in the elements (a lookup by scan) would
    // make sixteen times the elements cost 256 times the work; `n * log2(n)` makes it about 22.
    let small = MAX_RENAME_CANDIDATES / 16;
    for prefix in ["b", "h"] {
        let cost = |length: usize| {
            let base = chain("b", length, |i| i as f64);
            let head = chain(prefix, length, |i| i as f64);
            measured(&base, &head).0
        };
        let (at_small, at_cap) = (cost(small), cost(MAX_RENAME_CANDIDATES));
        assert!(
            at_cap < 40 * at_small,
            "{prefix}: {at_cap} units at the cap, {at_small} at a sixteenth"
        );
    }
}

#[test]
fn the_count_is_the_same_on_every_run() {
    let (base, head) = (chain("b", 256, |i| i as f64), chain("h", 256, |i| i as f64));
    let (first, _) = measured(&base, &head);
    for _ in 0..3 {
        assert_eq!(measured(&base, &head).0, first);
    }
    assert!(first > 0);
}
