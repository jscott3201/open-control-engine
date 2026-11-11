//! The cost of validation in key comparisons, never in elapsed time (issue #478): stage 1 and the
//! placement of findings on text stay within a fixed multiple of `n * log2(n)` comparisons for a
//! document of `n` statements, at the wire and text bounds and at a sixteenth of them, and a scan
//! per lookup, which is what the indexes replaced, is measured to break the same bound.
//!
//! The counts are a function of the input and of the ordered collections and the sort of the
//! pinned toolchain's `std`, so they are the same on every run and every host. They are held to a
//! bound with headroom, not to an exact number, so that a toolchain whose `BTreeMap` searches a
//! node a little differently does not fail the test; a scan is eighty times over the bound at
//! the wire bound, so the headroom hides no quadratic lookup.
//!
//! What this cannot see: a lookup that compares names without going through
//! [`Counted`] (a plain `==` over the declarations). Every ordered index of stage 1 and of the
//! text placement is keyed by it; a new lookup must be too.

use std::cell::Cell;
use std::cmp::Ordering;

use super::Counted;
use crate::identifier::Identifier;
use crate::model::{
    Assignment, Block, Boundary, Connection, Endpoint, Literal, Sequence, SignalType,
};
use crate::text::validate_text_author_side;
use crate::validate::{
    CLASS_NOT_FOUND, DUPLICATE_NAME, Finding, UNKNOWN_PORT, validate_author_side,
};

thread_local! {
    /// The ordering comparisons this thread has made through [`Counted`] and
    /// [`compared`](super::compared).
    pub(super) static COMPARISONS: Cell<u64> = const { Cell::new(0) };
}

/// Run `run` and return the comparisons it made on this thread, with its result.
fn comparisons<R>(run: impl FnOnce() -> R) -> (u64, R) {
    COMPARISONS.with(|count| count.set(0));
    let result = run();
    (COMPARISONS.with(Cell::get), result)
}

/// The most comparisons a document of `statements` statements may cost: twenty-four for
/// each statement and each bit of the statement count [measured: at most 16.2 for stage 1 and 8.6
/// for text placement, over the shapes below].
fn bound(statements: usize) -> u64 {
    let bits = u64::from(usize::BITS - statements.leading_zeros());
    24 * u64::try_from(statements).expect("fits") * bits
}

fn id(name: &str) -> Identifier {
    Identifier::unchecked(name)
}

/// `n` blocks of `class` in a chain from input `u` to output `y`, each block's `u` driven by the
/// `y` of the one before: `n` blocks and `n + 1` connections.
fn chain(n: usize, class: &str) -> Sequence {
    let mut s = Sequence::new(id("chain"));
    s.inputs.push(Boundary::new(id("u"), SignalType::Real));
    s.outputs.push(Boundary::new(id("y"), SignalType::Real));
    for i in 0..n {
        s.blocks.push(Block::new(id(&format!("b{i}")), class));
        let source = if i == 0 {
            "u".to_owned()
        } else {
            format!("b{}.y", i - 1)
        };
        s.connections.push(Connection::new(
            Endpoint::parse(&source),
            Endpoint::parse(&format!("b{i}.u")),
        ));
    }
    s.connections.push(Connection::new(
        Endpoint::parse(&format!("b{}.y", n - 1)),
        Endpoint::parse("y"),
    ));
    s
}

/// One `CDL.Reals.MultiSum` of `n` inputs, each driven by a boundary input of its own. With
/// `misnamed`, every connection names a port the block does not have.
fn fan_in(n: usize, misnamed: bool) -> Sequence {
    let mut s = Sequence::new(id("fan"));
    s.outputs.push(Boundary::new(id("y"), SignalType::Real));
    let mut sum = Block::new(id("sum"), "CDL.Reals.MultiSum");
    sum.assignments.push(Assignment::new(
        id("nin"),
        Literal::Integer(i64::try_from(n).expect("fits")),
    ));
    s.blocks.push(sum);
    for i in 0..n {
        s.inputs
            .push(Boundary::new(id(&format!("u{i}")), SignalType::Real));
        let port = if misnamed {
            format!("sum.v{}", i + 1)
        } else {
            format!("sum.u{}", i + 1)
        };
        s.connections.push(Connection::new(
            Endpoint::parse(&format!("u{i}")),
            Endpoint::parse(&port),
        ));
    }
    s.connections.push(Connection::new(
        Endpoint::parse("sum.y"),
        Endpoint::parse("y"),
    ));
    s
}

/// A text document of `n` block statements: of an unregistered class under names of their own,
/// or, with `one_name`, of a registered class all under the name `b`.
fn blocks_text(n: usize, one_name: bool) -> String {
    let mut text = String::from("sequence s\nderived_from none\n");
    for i in 0..n {
        if one_name {
            text.push_str("block b = CDL.Reals.Abs\n");
        } else {
            text.push_str(&format!("block b{i} = CDL.Reals.Nope\n"));
        }
    }
    text
}

/// The comparisons a scan per lookup makes: each of `wanted` found in `declared` by comparing it
/// with the declarations in order, the way stage 1 resolved an endpoint before its indexes.
fn scanned(declared: &[&str], wanted: &[&str]) -> u64 {
    comparisons(|| {
        for name in wanted {
            let found = declared
                .iter()
                .position(|d| Counted(*d).cmp(&Counted(*name)) == Ordering::Equal);
            assert!(found.is_some(), "{name} is declared");
        }
    })
    .0
}

/// The block names of `sequence`, and the block each connection endpoint names.
fn declared_and_wanted(sequence: &Sequence) -> (Vec<&str>, Vec<&str>) {
    let declared = sequence.blocks.iter().map(|b| b.name.as_str()).collect();
    let wanted = sequence
        .connections
        .iter()
        .flat_map(|c| [&c.source, &c.target])
        .filter_map(|endpoint| match endpoint {
            Endpoint::Port { block, .. } => Some(block.as_str()),
            Endpoint::Boundary(_) => None,
        })
        .collect();
    (declared, wanted)
}

/// Stage 1 over `sequence`, checked to return `expected` findings all of code `code`.
fn stage_one(sequence: &Sequence, expected: usize, code: &str) -> u64 {
    let (count, findings) = comparisons(|| validate_author_side(sequence));
    assert_findings(&findings, expected, code);
    count
}

fn assert_findings(findings: &[Finding], expected: usize, code: &str) {
    assert_eq!(findings.len(), expected);
    assert!(findings.iter().all(|f| f.code == code), "every one {code}");
}

#[test]
fn a_counted_key_orders_as_its_value_and_counts_each_comparison() {
    let names = ["", "b", "b1", "b10", "b2", "B", "u", "é"];
    for a in names {
        for b in names {
            let (count, ordering) = comparisons(|| Counted(a).cmp(&Counted(b)));
            assert_eq!(ordering, a.cmp(b), "{a:?} against {b:?}");
            assert_eq!(count, 1);
            let (count, ordering) = comparisons(|| Counted(a).partial_cmp(&Counted(b)));
            assert_eq!(ordering, Some(a.cmp(b)));
            assert_eq!(count, 1);
            // Equality is the value's and is not counted.
            let (count, equal) = comparisons(|| Counted(a) == Counted(b));
            assert_eq!(equal, a == b);
            assert_eq!(count, 0);
        }
    }
    // A pair of keys compares its second key only when the first ties.
    let (count, ordering) =
        comparisons(|| (Counted("b"), Counted("u")).cmp(&(Counted("b"), Counted("y"))));
    assert_eq!((ordering, count), (Ordering::Less, 2));
    let (count, ordering) =
        comparisons(|| (Counted("a"), Counted("u")).cmp(&(Counted("b"), Counted("u"))));
    assert_eq!((ordering, count), (Ordering::Less, 1));
}

#[test]
fn stage_one_compares_names_near_linearly_in_the_document() {
    // 30,000 blocks fill most of the 4 MiB wire bound (`tests/validate_cost.rs`).
    for n in [1_875, 30_000] {
        // A clean chain: every endpoint resolved against the block index and the port sets, every
        // target entered in the single-assignment index [measured: 259,363 and 6,061,407].
        let clean = stage_one(&chain(n, "CDL.Reals.Abs"), 0, "");
        assert!(clean <= bound(n), "{n} clean blocks: {clean} comparisons");
        // Every block a finding, so the findings are sorted too [188,221 and 4,397,992].
        let refused = stage_one(&chain(n, "CDL.Reals.Nope"), n, CLASS_NOT_FOUND);
        assert!(refused <= bound(n), "{n} findings: {refused} comparisons");
        // One block of `n` ports under `n` boundary inputs: the boundary index and the port sets
        // take the lookups; first resolved [309,288 and 7,258,280], then with every port
        // misnamed [172,484 and 3,874,149].
        let wide = stage_one(&fan_in(n, false), 0, "");
        assert!(wide <= bound(n), "{n} ports: {wide} comparisons");
        let misnamed = stage_one(&fan_in(n, true), n, UNKNOWN_PORT);
        assert!(misnamed <= bound(n), "{n} ports: {misnamed} comparisons");
    }
}

#[test]
fn placing_findings_on_text_compares_subjects_near_linearly_in_the_document() {
    // 65,536 is `MAX_STATEMENTS_PER_KIND`, the most blocks a text document declares.
    for n in [4_096, crate::text::MAX_STATEMENTS_PER_KIND] {
        // One finding on each block, each placed by its own subject [436,187 and 9,506,416].
        let text = blocks_text(n, false);
        let (count, findings) = comparisons(|| validate_text_author_side(&text).expect("parses"));
        assert_findings(&findings, n, CLASS_NOT_FOUND);
        assert!(findings.iter().all(|f| f.span.is_some()));
        assert!(count <= bound(n), "{n} findings: {count} comparisons");
        // Every block after the first repeats one name: one subject holding every location, and
        // the count of repeats looked up per finding [24,570 and 393,210].
        let text = blocks_text(n, true);
        let (count, findings) = comparisons(|| validate_text_author_side(&text).expect("parses"));
        assert_findings(&findings, n - 1, DUPLICATE_NAME);
        let last = findings.last().expect("findings").span.expect("placed");
        assert_eq!(usize::try_from(last.line).expect("fits"), n + 2);
        assert!(count <= bound(n), "{n} repeats: {count} comparisons");
    }
}

#[test]
fn a_scan_per_lookup_breaks_the_bound_the_indexes_keep() {
    // The control: the same counter, on the lookup the indexes replaced. A scan finds block `i`
    // after `i + 1` comparisons, and a chain names each block twice, so `n` blocks cost
    // `n * (n + 1)` comparisons: four times as many for twice the blocks, and over the bound
    // from the smallest size measured above.
    let small = chain(1_875, "CDL.Reals.Abs");
    let (declared, wanted) = declared_and_wanted(&small);
    let scan_small = scanned(&declared, &wanted);
    assert_eq!(scan_small, 1_875 * 1_876);
    assert!(scan_small > 7 * bound(1_875), "{scan_small}");

    let double = chain(3_750, "CDL.Reals.Abs");
    let (declared, wanted) = declared_and_wanted(&double);
    let scan_double = scanned(&declared, &wanted);
    assert_eq!(scan_double, 3_750 * 3_751);
    assert!(scan_double > 13 * bound(3_750), "{scan_double}");

    // Stage 1 on the same two documents: under five halves for twice the blocks
    // [measured: 259,363 and 599,502, a factor of 2.31].
    let index_small = stage_one(&small, 0, "");
    let index_double = stage_one(&double, 0, "");
    assert!(
        2 * index_double <= 5 * index_small,
        "{index_small} then {index_double}"
    );
    // At the wire bound a scan would make 900,030,000 comparisons, eighty times the bound.
    assert!(30_000 * 30_001 > 80 * bound(30_000));
}
