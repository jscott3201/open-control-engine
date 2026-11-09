//! Stage-1 validation cost stays near-linear in the document: a chain of blocks filling most of the
//! 4 MiB wire bound validates clean, and quadrupling the chain costs well under the sixteen-fold
//! a per-endpoint scan would (the issue #463 review measured 3.4 s for 30,000 blocks before the
//! name indexes).

mod support;

use std::time::{Duration, Instant};

use oce_author::{
    Block, Boundary, Connection, Endpoint, MAX_WIRE_BYTES, Sequence, SignalType, from_wire_str,
    to_wire_string, validate_author_side,
};

use support::id;

/// `n` `CDL.Reals.Abs` blocks in a chain from input `u` to output `y`.
fn chain(n: usize) -> Sequence {
    let mut s = Sequence::new(id("chain"));
    s.inputs.push(Boundary::new(id("u"), SignalType::Real));
    s.outputs.push(Boundary::new(id("y"), SignalType::Real));
    for i in 0..n {
        s.blocks
            .push(Block::new(id(&format!("b{i}")), "CDL.Reals.Abs"));
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

fn timed(sequence: &Sequence) -> Duration {
    let start = Instant::now();
    assert!(validate_author_side(sequence).is_empty());
    start.elapsed()
}

/// The fastest of several runs of each size, the two sizes interleaved so a burst of load from
/// tests running beside this one (the crate's mutation sweep, the 8 MiB emitter bound) inflates
/// both minimums alike rather than only the larger, slower run's.
fn fastest_pair(small: &Sequence, large: &Sequence) -> (Duration, Duration) {
    let mut best = (Duration::MAX, Duration::MAX);
    for _ in 0..7 {
        best.0 = best.0.min(timed(small));
        best.1 = best.1.min(timed(large));
    }
    best
}

#[test]
fn a_document_at_the_wire_bound_validates_clean() {
    let n = 30_000;
    let text = to_wire_string(&chain(n));
    assert!(text.len() <= MAX_WIRE_BYTES, "{} bytes", text.len());
    assert!(
        text.len() > MAX_WIRE_BYTES / 2,
        "{} bytes is near the bound",
        text.len()
    );
    let model = from_wire_str(&text).expect("reads");
    assert!(validate_author_side(&model).is_empty());
}

#[test]
fn quadrupling_the_document_costs_well_under_a_quadratic_factor() {
    let (small, large) = fastest_pair(&chain(7_500), &chain(30_000));
    let ratio = large.as_secs_f64() / small.as_secs_f64().max(1e-6);
    assert!(
        ratio < 10.0,
        "4x the blocks took {ratio:.1}x the time ({small:?} vs {large:?})"
    );
}
