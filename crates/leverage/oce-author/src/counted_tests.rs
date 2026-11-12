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
//! What this cannot see: a lookup that compares names without going through [`Counted`] (a plain
//! `==` over the declarations), and the lookups the header of `counted.rs` lists, which are
//! bounded by the catalog. Nor does it pin the cost of parsing: the text cases run the parser, but
//! the parser compares no key, so only stage 1 and the placement are counted there (issue #589).
//! Every index stage 1 and the text placement build over the document is keyed by [`Counted`]; a
//! new lookup must be too.

use std::cell::Cell;
use std::cmp::Ordering;

use super::Counted;
use crate::identifier::Identifier;
use crate::model::{
    Assignment, Block, Boundary, Connection, Endpoint, Literal, ParamValue, Sequence,
    SequenceParameter, SignalType,
};
use crate::text::validate_text_author_side;
use crate::validate::{
    CLASS_NOT_FOUND, DUPLICATE_NAME, Finding, UNIT_SUFFIX_MISMATCH, UNKNOWN_PARAMETER_REFERENCE,
    UNKNOWN_PORT, validate_author_side,
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
/// each statement and each bit of the statement count [measured: at most 19.9 for stage 1, on the
/// block of `n` ports and `n` assignments at 30,000, and 8.6 for text placement, over the shapes
/// below].
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

/// [`fan_in`] with the gain of every input assigned too (`k_1` to `k_n`): one block of `n` ports
/// and `n + 1` assignments.
fn weighted_fan_in(n: usize) -> Sequence {
    let mut s = fan_in(n, false);
    for i in 0..n {
        s.blocks[0].assignments.push(Assignment::new(
            id(&format!("k_{}", i + 1)),
            Literal::Real(1.0),
        ));
    }
    s
}

/// [`chain`] of `CDL.Logical.TrueDelay` between Boolean boundaries, every `delayTime` carrying the
/// value `base`, the unit suffix `suffix` and the written number `written` (issue #501): `n`
/// blocks, `n` suffixed assignments and `n + 1` connections.
fn suffixed_chain(n: usize, base: f64, suffix: &str, written: Option<Literal>) -> Sequence {
    let mut s = chain(n, "CDL.Logical.TrueDelay");
    s.inputs[0] = Boundary::new(id("u"), SignalType::Boolean);
    s.outputs[0] = Boundary::new(id("y"), SignalType::Boolean);
    for block in &mut s.blocks {
        let mut delay_time = Assignment::new(id("delayTime"), Literal::Real(base));
        delay_time.suffix = Some(suffix.to_owned());
        delay_time.scaled_from.clone_from(&written);
        block.assignments.push(delay_time);
    }
    s
}

/// `n` Real sequence parameters, each referenced by the gain of a block of its own: `n`
/// parameters, `n` blocks and no connection.
fn referenced_parameters(n: usize) -> Sequence {
    let mut s = Sequence::new(id("gains"));
    for i in 0..n {
        s.params.push(SequenceParameter::new(
            id(&format!("p{i}")),
            SignalType::Real,
            Literal::Real(1.0),
        ));
        let mut gain = Block::new(id(&format!("g{i}")), "CDL.Reals.MultiplyByParameter");
        gain.assignments.push(Assignment::new(
            id("k"),
            ParamValue::Ref(id(&format!("p{i}"))),
        ));
        s.blocks.push(gain);
    }
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

/// A text document of one `MultiSum` block with `elements` gains, each an element assignment
/// (`k_1` to `k_<elements>`) that references the undeclared `q`, one per line after `nin`.
fn array_text(elements: usize) -> String {
    let mut text = format!(
        "sequence s\nderived_from none\nblock m = CDL.Reals.MultiSum(\n  nin = {elements},\n"
    );
    for i in 1..=elements {
        let separator = if i == elements { "" } else { "," };
        text.push_str(&format!("  k_{i} = q{separator}\n"));
    }
    text.push_str(")\n");
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
        // The same block with a gain assigned for every port: the block's own assignment sets
        // take the further lookups [377,750 and 8,911,022].
        let weighted = stage_one(&weighted_fan_in(n), 0, "");
        assert!(
            weighted <= bound(n),
            "{n} assignments: {weighted} comparisons"
        );
        // `n` sequence parameters, each referenced once: the parameter index takes the lookups
        // [237,594 and 5,525,091].
        let referenced = stage_one(&referenced_parameters(n), 0, "");
        assert!(
            referenced <= bound(n),
            "{n} parameters: {referenced} comparisons"
        );
        // A suffix on every block's `delayTime` (issue #501): the agreement check and the
        // declared-unit lookup are per assignment and bounded by the catalog, so they add no
        // counted comparison. Scaled suffixes that fit cost what the clean chain costs [259,363
        // and 6,061,407]; base suffixes that do not fit, and scaled suffixes whose value
        // disagrees, are one finding per block [191,971 and 4,457,992 each].
        let scaled = stage_one(
            &suffixed_chain(n, 3_600.0, "h", Some(Literal::Integer(1))),
            0,
            "",
        );
        assert!(
            scaled <= bound(n),
            "{n} scaled suffixes: {scaled} comparisons"
        );
        let mismatched = stage_one(&suffixed_chain(n, 5.0, "K", None), n, UNIT_SUFFIX_MISMATCH);
        assert!(
            mismatched <= bound(n),
            "{n} mismatched suffixes: {mismatched} comparisons"
        );
        let disagreeing = stage_one(
            &suffixed_chain(n, 1.0, "h", Some(Literal::Integer(1))),
            n,
            UNIT_SUFFIX_MISMATCH,
        );
        assert!(
            disagreeing <= bound(n),
            "{n} disagreeing suffixes: {disagreeing} comparisons"
        );
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
        // Every block after the first repeats one name: one subject at every location, each
        // repeat placed by its own key, the subject and the block's position (GitHub #585; a map
        // keyed by the subject alone made 24,570 and 393,210, but put a finding about a
        // declaration named like a header word on the header) [193,195 and 4,039,343].
        let text = blocks_text(n, true);
        let (count, findings) = comparisons(|| validate_text_author_side(&text).expect("parses"));
        assert_findings(&findings, n - 1, DUPLICATE_NAME);
        // The blocks are lines 3 to `n + 2`; the repeats are every one but the first.
        for (k, finding) in findings.iter().enumerate() {
            let line = finding.span.expect("placed").line;
            assert_eq!(usize::try_from(line).expect("fits"), k + 4);
        }
        assert!(count <= bound(n), "{n} repeats: {count} comparisons");
        // One block of `nin` and `n - 1` array elements, the most assignments a block holds,
        // each element a finding: every element also looks its array's base up once in the
        // index, to hold it at the first (GitHub #617) [480,519 and 10,559,655; without the base,
        // 431,370 and 9,511,084].
        let text = array_text(n - 1);
        let (count, findings) = comparisons(|| validate_text_author_side(&text).expect("parses"));
        assert_findings(&findings, n - 1, UNKNOWN_PARAMETER_REFERENCE);
        assert!(findings.iter().all(|f| f.span.is_some()));
        assert!(count <= bound(n), "{n} array elements: {count} comparisons");
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
}
