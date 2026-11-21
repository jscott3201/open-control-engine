//! The cost of a diff in counted work, never in elapsed time (diff note, "Test plan", hostile
//! sizes; `TESTING.md`: cost is asserted with deterministic measures).
//!
//! Two promises are held here. A diff of two models stays within a fixed multiple of
//! `n * log2(n)` units for `n` statements, at the text form's statement limits and at a sixteenth
//! of them. And the rename pass costs what the links at the unmatched blocks cost, in every one
//! of its rounds, never what the candidate pairs would: a bucket of identical blocks at the pair
//! cap is measured against what an evaluation pair by pair would have to visit.
//!
//! The counts are a function of the inputs and of the ordered collections and the sort of the
//! pinned toolchain's `std`, so they are the same on every run and every host. They are held to
//! bounds with headroom, not to exact numbers, so that a toolchain whose `BTreeMap` searches a
//! node a little differently does not fail the test. The headroom hides no quadratic step: at
//! the statement limits a scan per lookup is over forty times the bound.
//!
//! What this cannot see: work that neither compares a counted key nor goes through
//! [`super::visited`], and the catalog lookups, which are bounded by the catalog. Every index,
//! group and sort over the models is keyed by [`super::Counted`]; a new one must be too.

use std::cell::Cell;

use oce_author::{
    Assignment, Block, Boundary, Connection, Endpoint, Identifier, Literal, ParamValue, Sequence,
    SignalType,
};

use crate::data::{MAX_RENAME_CANDIDATES, MAX_RENAME_PAIRS, MAX_RENAME_ROUNDS};
use crate::{Diff, DiffOptions, Note, diff};

thread_local! {
    /// The work this thread has counted.
    static WORK: Cell<u64> = const { Cell::new(0) };
}

/// Add `units` to this thread's count.
pub(crate) fn add(units: usize) {
    WORK.with(|work| work.set(work.get() + u64::try_from(units).unwrap_or(u64::MAX)));
}

/// Run `run` and return the work it counted on this thread, with its result.
fn work<R>(run: impl FnOnce() -> R) -> (u64, R) {
    WORK.with(|work| work.set(0));
    let result = run();
    (WORK.with(Cell::get), result)
}

fn measured(base: &Sequence, head: &Sequence, renames: bool) -> (u64, Diff) {
    let options = DiffOptions {
        renames,
        ..DiffOptions::default()
    };
    work(|| diff(base, head, &options))
}

/// The statements of a model: its parameters, boundaries, blocks, assignments and connections.
fn statements(model: &Sequence) -> usize {
    model.params.len()
        + model.inputs.len()
        + model.outputs.len()
        + model.blocks.len()
        + model
            .blocks
            .iter()
            .map(|block| block.assignments.len())
            .sum::<usize>()
        + model.connections.len()
}

/// `factor` units for each statement of the two models and each bit of their statement count.
fn bound(factor: u64, base: &Sequence, head: &Sequence) -> u64 {
    let statements = statements(base) + statements(head);
    let bits = u64::from(usize::BITS - statements.leading_zeros());
    factor * u64::try_from(statements).expect("fits") * bits
}

/// The most a diff may cost without a rename pass of several rounds [measured: at most 4.9 per
/// statement and bit over the chains below, 7.0 with one round over a bucket].
const DIFF_FACTOR: u64 = 12;

/// The most a diff may cost with a rename pass of [`MAX_RENAME_ROUNDS`] rounds, each of which
/// reads the links at every unmatched block again [measured: at most 17.3 per statement and bit
/// over the buckets below].
const ROUNDS_FACTOR: u64 = 32;

fn id(name: &str) -> Identifier {
    Identifier::unchecked(name)
}

fn link(source: &str, target: &str) -> Connection {
    Connection::new(Endpoint::parse(source), Endpoint::parse(target))
}

fn gain(name: &str, value: Literal) -> Block {
    let mut block = Block::new(id(name), "CDL.Reals.MultiplyByParameter");
    block
        .assignments
        .push(Assignment::new(id("k"), ParamValue::Literal(value)));
    block
}

/// `length` gain blocks named `<prefix><i>` in a chain from input `u` to output `y`: `length`
/// blocks, `length` assignments and `length` connections. `value` gives block `i` its gain.
fn chain(prefix: &str, length: usize, value: impl Fn(usize) -> Literal) -> Sequence {
    let mut model = Sequence::new(id("chain"));
    model.inputs.push(Boundary::new(id("u"), SignalType::Real));
    model.outputs.push(Boundary::new(id("y"), SignalType::Real));
    for i in 0..length {
        model.blocks.push(gain(&format!("{prefix}{i}"), value(i)));
        let source = if i == 0 {
            "u".to_owned()
        } else {
            format!("{prefix}{}.y", i - 1)
        };
        model
            .connections
            .push(link(&source, &format!("{prefix}{i}.u")));
    }
    // The last block's input link makes room for the output link, so the connections are at
    // the limit and not one past it.
    model.connections.pop();
    model
        .connections
        .push(link(&format!("{prefix}{}.y", length - 2), "y"));
    model
}

#[test]
fn two_models_at_the_statement_limits_diff_within_the_bound() {
    let limit = oce_author::MAX_STATEMENTS_PER_KIND;
    for length in [limit, limit / 16] {
        let base = chain("b", length, |_| Literal::Real(1.0));
        assert_eq!(base.blocks.len(), length);
        assert_eq!(base.connections.len(), length);
        // The same model, every block retuned, and every block under another name (far past
        // the candidate cap, so the pass is skipped).
        let cases = [
            ("the same", chain("b", length, |_| Literal::Real(1.0)), 0),
            (
                "retuned",
                chain("b", length, |_| Literal::Real(2.0)),
                length,
            ),
            (
                "renamed",
                chain("h", length, |_| Literal::Real(1.0)),
                2 * length + 1,
            ),
        ];
        for (name, head, entries) in cases {
            let (work, diff) = measured(&base, &head, true);
            assert_eq!(diff.graph.len(), entries, "{name} at {length}");
            let bound = bound(DIFF_FACTOR, &base, &head);
            assert!(
                work <= bound,
                "{name} at {length}: {work} units, bound {bound}"
            );
        }
    }
}

#[test]
fn a_scan_per_lookup_would_break_the_bound_at_the_statement_limits() {
    // The control for the bound above: one comparison per block of the other side, for each
    // block, is what a lookup by scan costs. It is far over the bound, so the bound would catch
    // it.
    let limit = oce_author::MAX_STATEMENTS_PER_KIND;
    let base = chain("b", limit, |_| Literal::Real(1.0));
    let scan = u64::try_from(limit * limit).expect("fits");
    assert!(scan > 40 * bound(DIFF_FACTOR, &base, &base));
}

#[test]
fn the_rename_pass_at_the_candidate_cap_stays_within_the_bound() {
    // Every block under another name, each with a gain of its own, so the gated pairs are one
    // per block. The chain is anchored at both ends (the input `u` and the output `y`), so each
    // round matches one more block from each end and the pass runs all its rounds.
    let value = |i: usize| Literal::Integer(i64::try_from(i).expect("fits"));
    for length in [MAX_RENAME_CANDIDATES, MAX_RENAME_CANDIDATES / 16] {
        let (base, head) = (chain("b", length, value), chain("h", length, value));
        let (work, diff) = measured(&base, &head, true);
        assert_eq!(
            diff.notes,
            [Note::NamesDisjoint],
            "the pass ran at {length}"
        );
        assert_eq!(diff.summary.blocks.renamed, 2 * MAX_RENAME_ROUNDS);
        let bound = bound(ROUNDS_FACTOR, &base, &head);
        assert!(work <= bound, "at {length}: {work} units, bound {bound}");
    }
}

/// A bucket of `size` identical blocks, each driving `degree` others of the bucket, beside a
/// chain of [`MAX_RENAME_ROUNDS`] blocks with a gain each, anchored at the input `u`. Against
/// its copy under other names the bucket is `size * size` gated pairs that no round can tell
/// apart, and the chain makes the pass run every round, each reading the whole bucket again.
fn bucket(prefix: &str, size: usize, degree: usize) -> Sequence {
    let mut model = Sequence::new(id("bucket"));
    model.inputs.push(Boundary::new(id("u"), SignalType::Real));
    for i in 0..size {
        model
            .blocks
            .push(Block::new(id(&format!("{prefix}{i}")), "CDL.Reals.Abs"));
    }
    for i in 0..size {
        for d in 1..=degree {
            model.connections.push(link(
                &format!("{prefix}{i}.y"),
                &format!("{prefix}{}.u{d}", (i + d) % size),
            ));
        }
    }
    for i in 0..MAX_RENAME_ROUNDS {
        let value = Literal::Integer(i64::try_from(i).expect("fits"));
        model.blocks.push(gain(&format!("{prefix}chain{i}"), value));
        let source = if i == 0 {
            "u".to_owned()
        } else {
            format!("{prefix}chain{}.y", i - 1)
        };
        model
            .connections
            .push(link(&source, &format!("{prefix}chain{i}.u")));
    }
    model
}

#[test]
fn a_worst_case_bucket_costs_its_links_not_its_pairs() {
    // 255 x 255 identical blocks and the chain's 8 pairs: 65,033 gated pairs, under the cap.
    for (size, degree) in [(16, 8), (255, 8), (255, 128)] {
        let (base, head) = (bucket("b", size, degree), bucket("h", size, degree));
        let pairs = size * size + MAX_RENAME_ROUNDS;
        assert!(pairs <= MAX_RENAME_PAIRS);
        let (on, diff) = measured(&base, &head, true);
        let (off, _) = measured(&base, &head, false);
        assert_eq!(diff.notes, [Note::NamesDisjoint], "the pass ran");
        assert_eq!(
            diff.summary.blocks.renamed, MAX_RENAME_ROUNDS,
            "the chain is matched in as many rounds; the bucket never is"
        );
        let bound = bound(ROUNDS_FACTOR, &base, &head);
        assert!(
            on <= bound,
            "{size} x {size}, degree {degree}: {on} units, bound {bound}"
        );
        // What the pass itself cost, against the least an evaluation pair by pair would visit:
        // both blocks' link ends (`degree` in and `degree` out on each side), for every pair,
        // in every round.
        let pass = on - off;
        let per_pair = u64::try_from(pairs * MAX_RENAME_ROUNDS * 4 * degree).expect("fits");
        if size == 255 {
            assert!(
                8 * pass < per_pair,
                "{size} x {size}, degree {degree}: the pass cost {pass} units; pair by pair is {per_pair}"
            );
        }
    }
}

#[test]
fn a_bucket_past_the_pair_cap_costs_no_round_at_all() {
    // 2,040 x 2,040 identical blocks: over four million gated pairs. The pass counts the gates,
    // refuses, and reads no link.
    let size = MAX_RENAME_CANDIDATES - MAX_RENAME_ROUNDS;
    let (base, head) = (bucket("b", size, 8), bucket("h", size, 8));
    assert_eq!(base.blocks.len(), MAX_RENAME_CANDIDATES);
    let (on, diff) = measured(&base, &head, true);
    let (off, _) = measured(&base, &head, false);
    assert_eq!(diff.notes, [Note::NamesDisjoint, Note::RenamePassSkipped]);
    assert_eq!(diff.summary.blocks.renamed, 0);
    let bound = bound(DIFF_FACTOR, &base, &head);
    assert!(on <= bound, "{on} units, bound {bound}");
    // Counting the gates is the whole difference: a few comparisons for each unmatched block.
    let gates = on - off;
    let blocks = u64::try_from(2 * MAX_RENAME_CANDIDATES).expect("fits");
    assert!(
        gates <= 16 * blocks,
        "the refusal cost {gates} units for {blocks} blocks"
    );
}

#[test]
fn the_count_is_the_same_on_every_run() {
    let (base, head) = (bucket("b", 64, 8), bucket("h", 64, 8));
    let (first, _) = measured(&base, &head, true);
    for _ in 0..3 {
        assert_eq!(measured(&base, &head, true).0, first);
    }
    assert!(first > 0);
}
