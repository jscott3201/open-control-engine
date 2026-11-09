//! The emitter's bounds (authoring note, "Writer" and test plan): a document of exactly
//! `oce_api::MAX_CXF_BYTES` is written and loads; one byte more is refused before any engine sees
//! it ([`EmitError::TooLarge`]), and stage 2 reports it as `document-too-large`. A namespace is
//! refused unless it ends in `#`, `/` or `:` and is printable ASCII without IRI-reserved
//! characters.

mod support;

use oce_api::MAX_CXF_BYTES;
use oce_author::{
    Assignment, Block, Boundary, Connection, EmitError, EmitOptions, Endpoint, Literal,
    MAX_NAMESPACE_BYTES, ParamValue, Sequence, SignalType, emit, validate,
};

use support::cxf::load;
use support::{econ_enable, id};

/// A chain of `n` `Reals.Abs` blocks from input `x` to output `y`, plus one `Utilities.Assert`
/// on Boolean input `b` whose message is `message_len` letters — the byte-exact padding knob
/// (a letter is written as one byte).
fn chain(n: usize, message_len: usize) -> Sequence {
    let mut s = Sequence::new(id("bound"));
    s.inputs.push(Boundary::new(id("x"), SignalType::Real));
    s.inputs.push(Boundary::new(id("b"), SignalType::Boolean));
    s.outputs.push(Boundary::new(id("y"), SignalType::Real));
    let mut previous = "x".to_owned();
    for i in 0..n {
        let name = format!("a{i}");
        s.blocks.push(Block::new(id(&name), "CDL.Reals.Abs"));
        s.connections.push(Connection::new(
            Endpoint::parse(&previous),
            Endpoint::parse(&format!("{name}.u")),
        ));
        previous = format!("{name}.y");
    }
    s.connections.push(Connection::new(
        Endpoint::parse(&previous),
        Endpoint::parse("y"),
    ));
    let mut assert = Block::new(id("check"), "CDL.Utilities.Assert");
    assert.assignments.push(Assignment::new(
        id("message"),
        ParamValue::Literal(Literal::Text("m".repeat(message_len))),
    ));
    s.blocks.push(assert);
    s.connections.push(Connection::new(
        Endpoint::parse("b"),
        Endpoint::parse("check.u"),
    ));
    s
}

fn size(s: &Sequence) -> usize {
    match emit(s, &EmitOptions::new()) {
        Ok(bytes) => bytes.len(),
        Err(EmitError::TooLarge { bytes, .. }) => bytes,
        Err(other) => panic!("{other}"),
    }
}

/// A sequence that emits to exactly `target` bytes.
fn sized(target: usize) -> Sequence {
    let per_block = size(&chain(2, 1)) - size(&chain(1, 1));
    let fixed = size(&chain(0, 1));
    assert!(per_block < 4_000, "one block must fit in the padding range");
    // Names grow with the block count, so the estimate is corrected until the padding fits.
    let mut n = (target - fixed) / per_block;
    let mut base = size(&chain(n, 1));
    for _ in 0..16 {
        if base > target {
            n -= (base - target).div_ceil(per_block);
        } else if target - base >= 4_000 {
            n += (target - base) / (2 * per_block);
        } else {
            break;
        }
        base = size(&chain(n, 1));
    }
    assert!(
        base <= target && target - base < 4_000,
        "{base} vs {target}"
    );
    let s = chain(n, 1 + target - base);
    assert_eq!(size(&s), target);
    s
}

#[test]
fn a_document_at_the_limit_is_written_and_loads() {
    let s = sized(MAX_CXF_BYTES);
    let bytes = emit(&s, &EmitOptions::new()).expect("at the limit");
    assert_eq!(bytes.len(), MAX_CXF_BYTES);
    let (_, report) = load(&bytes).expect("loads");
    assert!(report.warnings.is_empty());
}

#[test]
fn one_byte_over_the_limit_is_refused_before_load() {
    let s = sized(MAX_CXF_BYTES + 1);
    assert_eq!(
        emit(&s, &EmitOptions::new()),
        Err(EmitError::TooLarge {
            bytes: MAX_CXF_BYTES + 1,
            limit: MAX_CXF_BYTES
        })
    );
    let findings = validate(&s);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "document-too-large");
    assert_eq!(findings[0].subject, "sequence");
}

#[test]
fn a_malformed_namespace_is_refused() {
    let s = econ_enable();
    for (namespace, problem) in [
        ("", "is empty"),
        ("urn:oce:author", "does not end"),
        ("http://example.org#g36.source.", "does not end"),
        ("urn:a b:", "outside printable ASCII"),
        ("urn:caf\u{e9}:", "outside printable ASCII"),
        ("urn:a\"b:", "IRI-reserved"),
        ("urn:<a>:", "IRI-reserved"),
        ("urn:a\\b:", "IRI-reserved"),
        ("urn:a\n:", "outside printable ASCII"),
    ] {
        match emit(&s, &EmitOptions::new().with_namespace(namespace)) {
            Err(error @ EmitError::InvalidNamespace { .. }) => {
                assert!(
                    error.to_string().contains(problem),
                    "{namespace:?}: {error}"
                );
            }
            other => panic!("{namespace:?}: {other:?}"),
        }
    }
}

#[test]
fn the_namespace_length_is_bounded_at_its_limit() {
    let s = econ_enable();
    let at_limit = format!("urn:{}:", "n".repeat(MAX_NAMESPACE_BYTES - 5));
    assert_eq!(at_limit.len(), MAX_NAMESPACE_BYTES);
    assert!(emit(&s, &EmitOptions::new().with_namespace(&*at_limit)).is_ok());
    let over = format!("urn:{}:", "n".repeat(MAX_NAMESPACE_BYTES - 4));
    assert!(matches!(
        emit(&s, &EmitOptions::new().with_namespace(over)),
        Err(EmitError::InvalidNamespace { .. })
    ));
}

#[test]
fn every_delimiter_ending_is_accepted() {
    let s = econ_enable();
    for namespace in ["urn:x:", "http://example.org/x/", "http://example.org#"] {
        assert!(emit(&s, &EmitOptions::new().with_namespace(namespace)).is_ok());
    }
}

#[test]
fn a_stage_one_error_is_refused_with_every_finding() {
    let mut s = econ_enable();
    s.blocks[0].class_path = "CDL.Reals.Nope".to_owned();
    match emit(&s, &EmitOptions::new()) {
        Err(EmitError::Invalid(findings)) => {
            assert_eq!(findings, oce_author::validate_author_side(&s));
        }
        other => panic!("{other:?}"),
    }
}
