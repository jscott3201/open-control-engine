//! A document near the wire bound validates clean: a chain of blocks filling most of the 4 MiB
//! bound passes stage 1.
//!
//! What stage 1 costs at that size is pinned in key comparisons by the unit tests of
//! `src/counted.rs` (issue #478), not here and not in elapsed time: the comparison counter is
//! compiled into the library's own test build only. The issue #463 review measured 3.4 s for
//! 30,000 blocks before the name indexes.

use crate::support;

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
