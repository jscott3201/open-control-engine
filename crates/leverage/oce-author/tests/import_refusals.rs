//! The minimal importer refuses hostile and out-of-subset input with a typed error, never a
//! panic, and reads every emitted golden back to the same bytes.

mod support;

use oce_api::MAX_CXF_BYTES;
use oce_author::{EmitOptions, ImportError, emit, import_cxf};

use support::fixture;

/// A small valid document, edited per case by textual substitution.
fn document() -> String {
    std::fs::read_to_string(fixture("emit/econ_enable.cxf.json")).expect("golden")
}

fn edited(from: &str, to: &str) -> Vec<u8> {
    let text = document();
    assert!(text.contains(from), "{from}");
    text.replacen(from, to, 1).into_bytes()
}

fn malformed(bytes: &[u8]) {
    assert!(
        matches!(import_cxf(bytes), Err(ImportError::Malformed { .. })),
        "{:?}",
        import_cxf(bytes)
    );
}

fn unsupported(bytes: &[u8], construct: &str) {
    match import_cxf(bytes) {
        Err(error @ ImportError::Unsupported { .. }) => {
            assert!(error.to_string().contains(construct), "{error}");
        }
        other => panic!("expected `{construct}`, got {other:?}"),
    }
}

#[test]
fn the_size_bound_is_checked_before_parsing() {
    let over = vec![b' '; MAX_CXF_BYTES + 1];
    assert_eq!(
        import_cxf(&over),
        Err(ImportError::TooLarge {
            bytes: MAX_CXF_BYTES + 1,
            limit: MAX_CXF_BYTES
        })
    );
    // At the limit it is parsed (and refused as not JSON, not as too large).
    malformed(&vec![b' '; MAX_CXF_BYTES]);
}

#[test]
fn bytes_that_are_not_a_document_are_malformed() {
    for bytes in [
        &b""[..],
        b"\xff\xfe",
        b"[]",
        b"{}",
        b"{\"@graph\": {}}",
        b"{\"@graph\": [1]}",
        b"{\"@graph\": [{}]}",
        b"{\"@graph\": [{\"@id\": 7}]}",
        b"{\"@graph\": []}",
    ] {
        malformed(bytes);
    }
}

#[test]
fn deep_nesting_is_refused_without_overflowing_the_stack() {
    let mut deep = String::from("{\"@graph\": [");
    deep.push_str(&"[".repeat(100_000));
    malformed(deep.as_bytes());
}

#[test]
fn a_repeated_id_is_malformed() {
    let text = document();
    let doubled = text.replacen(
        "\"@id\": \"urn:oce:author:econ_enable.ecoEna\",",
        "\"@id\": \"urn:oce:author:econ_enable.TOut\",",
        1,
    );
    malformed(doubled.as_bytes());
}

#[test]
fn context_and_top_level_keys_outside_the_dialect_are_refused() {
    unsupported(
        &edited(
            "\"S231\": \"http://data.ashrae.org/S231P#\"",
            "\"S231\": \"http://data.ashrae.org/S231#\"",
        ),
        "`@context` term `S231`",
    );
    unsupported(
        &edited(
            "\"S231\": \"http://data.ashrae.org/S231P#\"",
            "\"S231\": \"http://data.ashrae.org/S231P#\", \"ex\": \"urn:x:\"",
        ),
        "`@context` term `ex`",
    );
    unsupported(
        &edited("\"@graph\": [", "\"@id\": \"urn:x\", \"@graph\": ["),
        "top-level key `@id`",
    );
}

#[test]
fn constructs_outside_the_authoring_subset_are_refused() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "\"@type\": \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Subtract\",",
            "\"@type\": \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Subtract\", \"S231:isConditionalComponent\": true,",
            "`S231:isConditionalComponent`",
        ),
        (
            "\"@type\": \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Subtract\",",
            "\"@type\": \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Subtract\", \"S231:hasInstance\": [],",
            "`S231:hasInstance`",
        ),
        (
            "\"@type\": \"S231:Block\",",
            "\"@type\": \"S231:Block\", \"S231:hasConstant\": [],",
            "`S231:hasConstant`",
        ),
        (
            "\"S231:displayUnit\": \"degC\",",
            "\"S231:displayUnit\": \"degC\", \"S231:min\": 0,",
            "`S231:min`",
        ),
        (
            "\"S231:value\": \"dTHys\"",
            "\"S231:value\": \"dTHys + 1\"",
            "an expression",
        ),
        (
            "\"S231:value\": \"dTHys\"",
            "\"S231:value\": \"Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes.occupied\"",
            "a library enumeration",
        ),
        (
            "\"S231:value\": \"dTHys\"",
            "\"S231:value\": \"\\\"open\"",
            "unterminated string",
        ),
        (
            "\"S231:value\": \"dTHys\"",
            "\"S231:value\": \"\\\"a\\\\tb\\\"\"",
            "a string escape",
        ),
        (
            "\"@value\": \"0.0\",",
            "\"@value\": \"NaN\",",
            "not a finite number",
        ),
        (
            "\"@type\": \"http://www.w3.org/2001/XMLSchema#double\"\n      }\n    },\n    {\n      \"@id\": \"urn:oce:author:econ_enable.hys.uHigh\"",
            "\"@type\": \"http://www.w3.org/2001/XMLSchema#date\"\n      }\n    },\n    {\n      \"@id\": \"urn:oce:author:econ_enable.hys.uHigh\"",
            "unsupported datatype",
        ),
        (
            "\"@type\": \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Subtract\"",
            "\"@type\": \"http://example.org#Modelica.Blocks.Math.Add\"",
            "outside `Buildings.Controls.OBC`",
        ),
        (
            "\"@type\": \"S231:BooleanOutput\",\n      \"S231:isOfDataType\": {\n        \"@id\": \"S231:Boolean\"\n      }\n    }\n  ]",
            "\"@type\": \"S231:BooleanOutput\",\n      \"S231:isOfDataType\": {\n        \"@id\": \"S231:Boolean\"\n      },\n      \"S231:isConnectedTo\": [{\"@id\": \"urn:oce:author:econ_enable.hys.u\"}]\n    }\n  ]",
            "a boundary output that drives",
        ),
        (
            "\"@type\": \"S231:BooleanOutput\",\n      \"S231:isOfDataType\": {\n        \"@id\": \"S231:Boolean\"\n      }\n    }\n  ]",
            "\"@type\": \"S231:BooleanOutput\",\n      \"S231:isOfDataType\": {\n        \"@id\": \"S231:Real\"\n      }\n    }\n  ]",
            "a connector that is not",
        ),
        (
            "\"@type\": \"S231:BooleanOutput\",\n      \"S231:isOfDataType\": {\n        \"@id\": \"S231:Boolean\"\n      }\n    }\n  ]",
            "\"@type\": \"S231:BooleanOutput\",\n      \"S231:isOfDataType\": {\n        \"@id\": \"S231:Boolean\"\n      }\n    },\n    {\"@id\": \"urn:oce:author:econ_enable.stray\"}\n  ]",
            "a node outside the root's relations",
        ),
        (
            "\"@id\": \"urn:oce:author:econ_enable.sub\"\n        },",
            "\"@id\": \"urn:oce:author:econ_enable.sub\"\n        },\n        {\n          \"@id\": \"urn:oce:author:econ_enable.sub\"\n        },",
            "a node referenced twice",
        ),
        (
            "\"@type\": \"S231:Block\",",
            "\"@type\": \"S231:Block\", \"S231:isConnectedTo\": [],",
            "`S231:isConnectedTo`",
        ),
    ];
    for (from, to, construct) in cases {
        unsupported(&edited(from, to), construct);
    }
}

#[test]
fn a_dangling_reference_is_unresolved() {
    let dangling = edited(
        "\"S231:isConnectedTo\": [\n        {\n          \"@id\": \"urn:oce:author:econ_enable.hys.u\"",
        "\"S231:isConnectedTo\": [\n        {\n          \"@id\": \"urn:oce:author:econ_enable.hys.nope\"",
    );
    assert!(
        matches!(import_cxf(&dangling), Err(ImportError::Unresolved { .. })),
        "{:?}",
        import_cxf(&dangling)
    );
    let foreign = edited(
        "\"S231:isConnectedTo\": [\n        {\n          \"@id\": \"urn:oce:author:econ_enable.hys.u\"",
        "\"S231:isConnectedTo\": [\n        {\n          \"@id\": \"urn:other:hys.u\"",
    );
    assert!(matches!(
        import_cxf(&foreign),
        Err(ImportError::Unresolved { .. })
    ));
}

#[test]
fn every_truncation_and_byte_flip_is_refused_or_read_without_panic() {
    let bytes = document().into_bytes();
    // Every proper prefix short of the closing brace is refused (the last byte is the newline).
    for end in 0..bytes.len() - 1 {
        assert!(import_cxf(&bytes[..end]).is_err(), "truncated at {end}");
    }
    for at in 0..bytes.len() {
        for flip in [0x01u8, 0x20, 0x80] {
            let mut mutated = bytes.clone();
            mutated[at] ^= flip;
            let _ = import_cxf(&mutated);
        }
    }
}

#[test]
fn every_emitted_golden_reimports_to_the_same_bytes() {
    for entry in std::fs::read_dir(fixture("emit")).expect("goldens") {
        let path = entry.expect("entry").path();
        let bytes = std::fs::read(&path).expect("read");
        let sequence = import_cxf(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            emit(&sequence, &EmitOptions::new()).expect("emit"),
            bytes,
            "{}",
            path.display()
        );
    }
}
