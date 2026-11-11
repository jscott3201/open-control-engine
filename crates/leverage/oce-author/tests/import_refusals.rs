//! The CXF importer refuses hostile and out-of-subset input with a typed error whose kind names
//! the refusal, never a panic, and reads every emitted golden back to the same bytes.

use crate::support;

use oce_api::MAX_CXF_BYTES;
use oce_author::{
    EmitOptions, ImportError, ImportErrorKind, ImportOptions, Imported, SourceForm, emit,
    import_cxf,
};

use support::fixture;

fn import(bytes: &[u8]) -> Result<Imported, ImportError> {
    import_cxf(bytes, &ImportOptions::new())
}

/// The code `bytes` is refused with.
fn code(bytes: &[u8]) -> &'static str {
    match import(bytes) {
        Err(error) => error.code(),
        Ok(_) => "imported",
    }
}

/// A small valid document, edited per case by textual substitution.
fn document() -> String {
    std::fs::read_to_string(fixture("emit/econ_enable.cxf.json")).expect("golden")
}

fn edited(from: &str, to: &str) -> Vec<u8> {
    let text = document();
    assert!(text.contains(from), "{from}");
    text.replacen(from, to, 1).into_bytes()
}

/// Assert `bytes` is refused with `expected` at `subject`.
fn refused(bytes: &[u8], expected: &str, subject: &str) {
    match import(bytes) {
        Err(error) => {
            assert_eq!(error.source, SourceForm::Cxf);
            assert_eq!(
                (error.code(), error.subject.as_str()),
                (expected, subject),
                "{error}"
            );
        }
        Ok(_) => panic!("expected `{expected}` at `{subject}`, but it imported"),
    }
}

#[test]
fn the_size_bound_is_checked_before_parsing() {
    let over = vec![b' '; MAX_CXF_BYTES + 1];
    assert_eq!(
        import(&over).map(|_| ()).map_err(|e| e.kind),
        Err(ImportErrorKind::TooLarge {
            bytes: MAX_CXF_BYTES + 1,
            limit: MAX_CXF_BYTES
        })
    );
    // At the limit it is parsed (and refused as not JSON, not as too large).
    assert_eq!(code(&vec![b' '; MAX_CXF_BYTES]), "not-strict-json");
}

#[test]
fn bytes_that_are_not_a_document_are_refused_with_their_kind() {
    for (bytes, expected) in [
        (&b""[..], "not-strict-json"),
        (b"\xff\xfe", "not-strict-json"),
        (b"[]", "wrong-shape"),
        (b"{}", "missing-key"),
        (b"{\"@graph\": {}}", "wrong-shape"),
        (b"{\"@graph\": [1]}", "wrong-shape"),
        (b"{\"@graph\": [{}]}", "missing-key"),
        (b"{\"@graph\": [{\"@id\": 7}]}", "wrong-shape"),
        (b"{\"@graph\": []}", "no-root"),
        (b"{\"@context\": [], \"@graph\": []}", "wrong-shape"),
    ] {
        assert_eq!(code(bytes), expected, "{}", String::from_utf8_lossy(bytes));
    }
}

#[test]
fn deep_nesting_is_refused_without_overflowing_the_stack() {
    let mut deep = String::from("{\"@graph\": [");
    deep.push_str(&"[".repeat(100_000));
    assert_eq!(code(deep.as_bytes()), "not-strict-json");
}

#[test]
fn a_repeated_id_is_refused() {
    let text = document();
    let doubled = text.replacen(
        "\"@id\": \"urn:oce:author:econ_enable.ecoEna\",",
        "\"@id\": \"urn:oce:author:econ_enable.TOut\",",
        1,
    );
    refused(
        doubled.as_bytes(),
        "duplicate-id",
        "urn:oce:author:econ_enable.TOut",
    );
}

#[test]
fn context_and_top_level_keys_outside_the_dialect_are_refused() {
    for (from, to, key) in [
        (
            "\"S231\": \"http://data.ashrae.org/S231#\"",
            "\"S231\": \"http://data.ashrae.org/S231#\", \"ex\": \"urn:x:\"",
            "ex",
        ),
        // The strict dialect reads compact `S231:` keys only, so a second name for the namespace
        // is a term it does not define, not a namespace refusal.
        (
            "\"S231\": \"http://data.ashrae.org/S231#\"",
            "\"S231\": \"http://data.ashrae.org/S231#\", \"cdl\": \"http://data.ashrae.org/S231#\"",
            "cdl",
        ),
        (
            "\"S231\": \"http://data.ashrae.org/S231#\"",
            "\"S231\": 7",
            "S231",
        ),
        ("\"@graph\": [", "\"@id\": \"urn:x\", \"@graph\": [", "@id"),
    ] {
        let error = import(&edited(from, to)).map(|_| ()).expect_err("refused");
        assert_eq!(
            error.kind,
            ImportErrorKind::UnknownKey {
                key: key.to_owned()
            }
        );
        assert_eq!(error.subject, "document");
    }
}

/// The worked example's node `@id`s.
const ROOT: &str = "urn:oce:author:econ_enable";

fn node(local: &str) -> String {
    format!("{ROOT}.{local}")
}

#[test]
fn constructs_outside_the_authoring_subset_are_refused_with_their_kind() {
    let sub = "\"@type\": \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Subtract\",";
    let output_tail = "\"@type\": \"S231:BooleanOutput\",\n      \"S231:isOfDataType\": {\n        \"@id\": \"S231:Boolean\"\n      }\n    }\n  ]";
    let cases: Vec<(String, String, &str, String)> = vec![
        (
            sub.to_owned(),
            format!("{sub} \"S231:isConditionalComponent\": true,"),
            "conditional-component",
            node("sub"),
        ),
        (
            sub.to_owned(),
            format!("{sub} \"S231:hasInstance\": [],"),
            "instance-member",
            node("sub"),
        ),
        (
            "\"@type\": \"S231:Block\",".to_owned(),
            "\"@type\": \"S231:Block\", \"S231:hasConstant\": [],".to_owned(),
            "constant-member",
            ROOT.to_owned(),
        ),
        (
            "\"S231:displayUnit\": \"degC\",".to_owned(),
            "\"S231:displayUnit\": \"degC\", \"S231:nominal\": 300,".to_owned(),
            "connector-attribute",
            node("TOut"),
        ),
        (
            "\"S231:displayUnit\": \"degC\",".to_owned(),
            "\"S231:displayUnit\": \"degC\", \"S231:unbounded\": true,".to_owned(),
            "connector-attribute",
            node("TOut"),
        ),
        (
            "\"S231:displayUnit\": \"degC\",".to_owned(),
            "\"S231:displayUnit\": \"degC\", \"S231:isArray\": true,".to_owned(),
            "array-connector",
            node("TOut"),
        ),
        (
            "\"S231:value\": \"dTHys\"".to_owned(),
            "\"S231:value\": \"dTHys + 1\"".to_owned(),
            "expression-value",
            node("hys.uHigh"),
        ),
        (
            "\"S231:value\": \"dTHys\"".to_owned(),
            "\"S231:value\": \"Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes.occupied\""
                .to_owned(),
            "datatype-mismatch",
            node("hys.uHigh"),
        ),
        (
            "\"S231:value\": \"dTHys\"".to_owned(),
            "\"S231:value\": \"Buildings.Controls.OBC.ASHRAE.G36.Types.NoSuchPackage.member\""
                .to_owned(),
            "unknown-member-type",
            node("hys.uHigh"),
        ),
        (
            "\"S231:value\": \"dTHys\"".to_owned(),
            "\"S231:value\": \"\\\"open\"".to_owned(),
            "malformed-literal",
            node("hys.uHigh"),
        ),
        (
            "\"S231:value\": \"dTHys\"".to_owned(),
            "\"S231:value\": \"\\\"a\\\\tb\\\"\"".to_owned(),
            "malformed-literal",
            node("hys.uHigh"),
        ),
        (
            "\"S231:value\": \"dTHys\"".to_owned(),
            "\"S231:value\": \"\\\"a\\\" + \\\"b\\\"\"".to_owned(),
            "expression-value",
            node("hys.uHigh"),
        ),
        (
            "\"@value\": \"0.0\",".to_owned(),
            "\"@value\": \"NaN\",".to_owned(),
            "malformed-literal",
            node("hys.uLow"),
        ),
        (
            "\"@value\": \"0.0\",".to_owned(),
            "\"@value\": \" 0.0\",".to_owned(),
            "malformed-literal",
            node("hys.uLow"),
        ),
        (
            "\"@type\": \"http://www.w3.org/2001/XMLSchema#double\"\n      }\n    },\n    {\n      \"@id\": \"urn:oce:author:econ_enable.hys.uHigh\"".to_owned(),
            "\"@type\": \"http://www.w3.org/2001/XMLSchema#date\"\n      }\n    },\n    {\n      \"@id\": \"urn:oce:author:econ_enable.hys.uHigh\"".to_owned(),
            "malformed-literal",
            node("hys.uLow"),
        ),
        (
            "\"@type\": \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Subtract\"".to_owned(),
            "\"@type\": \"http://example.org#Modelica.Blocks.Math.Add\"".to_owned(),
            "class-outside-obc",
            node("sub"),
        ),
        (
            output_tail.to_owned(),
            "\"@type\": \"S231:BooleanOutput\",\n      \"S231:isOfDataType\": {\n        \"@id\": \"S231:Boolean\"\n      },\n      \"S231:isConnectedTo\": [{\"@id\": \"urn:oce:author:econ_enable.hys.u\"}]\n    }\n  ]".to_owned(),
            "driving-sink",
            node("ecoEna"),
        ),
        (
            output_tail.to_owned(),
            "\"@type\": \"S231:BooleanOutput\",\n      \"S231:isOfDataType\": {\n        \"@id\": \"S231:Real\"\n      }\n    }\n  ]".to_owned(),
            "datatype-mismatch",
            node("ecoEna"),
        ),
        (
            output_tail.to_owned(),
            "\"@type\": \"S231:BooleanOutput\",\n      \"S231:isOfDataType\": {\n        \"@id\": \"S231:Boolean\"\n      }\n    },\n    {\"@id\": \"urn:oce:author:econ_enable.stray\"}\n  ]".to_owned(),
            "stray-node",
            node("stray"),
        ),
        (
            "\"@id\": \"urn:oce:author:econ_enable.sub\"\n        },".to_owned(),
            "\"@id\": \"urn:oce:author:econ_enable.sub\"\n        },\n        {\n          \"@id\": \"urn:oce:author:econ_enable.sub\"\n        },".to_owned(),
            "stray-node",
            node("sub"),
        ),
        (
            "\"@type\": \"S231:Block\",".to_owned(),
            "\"@type\": \"S231:Block\", \"S231:isConnectedTo\": [],".to_owned(),
            "unknown-key",
            ROOT.to_owned(),
        ),
        (
            "\"@id\": \"urn:oce:author:econ_enable.hys.u\",\n      \"@type\": \"S231:RealInput\",".to_owned(),
            "\"@id\": \"urn:oce:author:econ_enable.hys.u\",\n      \"@type\": \"S231:RealInput\", \"S231:isConnectedTo\": [{\"@id\": \"urn:oce:author:econ_enable.ecoEna\"}],".to_owned(),
            "driving-sink",
            node("hys.u"),
        ),
        (
            "\"@id\": \"urn:oce:author:econ_enable.hys.u\",\n      \"@type\": \"S231:RealInput\",".to_owned(),
            "\"@id\": \"urn:oce:author:econ_enable.hys.u\",\n      \"@type\": \"S231:RealInput\", \"S231:isArray\": true,".to_owned(),
            "array-connector",
            node("hys.u"),
        ),
        (
            "\"@id\": \"urn:oce:author:econ_enable.hys.u\",\n      \"@type\": \"S231:RealInput\",".to_owned(),
            "\"@id\": \"urn:oce:author:econ_enable.hys.u\",\n      \"@type\": \"S231:RealInput\", \"S231:max\": 1,".to_owned(),
            "port-attribute",
            node("hys.u"),
        ),
        (
            "\"@id\": \"urn:oce:author:econ_enable.dTHys\",\n      \"@type\": \"S231:Parameter\",".to_owned(),
            "\"@id\": \"urn:oce:author:econ_enable.dTHys\",\n      \"@type\": \"S231:Parameter\", \"S231:isArray\": true,".to_owned(),
            "array-sequence-parameter",
            node("dTHys"),
        ),
    ];
    for (from, to, expected, subject) in cases {
        refused(&edited(&from, &to), expected, &subject);
    }
}

#[test]
fn a_dangling_reference_is_unresolved() {
    let connected = "\"S231:isConnectedTo\": [\n        {\n          \"@id\": \"urn:oce:author:econ_enable.hys.u\"";
    for target in ["urn:oce:author:econ_enable.hys.nope", "urn:other:hys.u"] {
        let error = import(&edited(
            connected,
            &connected.replace("urn:oce:author:econ_enable.hys.u", target),
        ))
        .map(|_| ())
        .expect_err("refused");
        assert_eq!(
            (error.subject.as_str(), error.kind),
            (
                "urn:oce:author:econ_enable.sub.y",
                ImportErrorKind::Unresolved {
                    reference: target.to_owned()
                }
            )
        );
    }
}

#[test]
fn every_truncation_and_byte_flip_is_refused_or_read_without_panic() {
    let bytes = document().into_bytes();
    // Every proper prefix short of the closing brace is refused (the last byte is the newline).
    for end in 0..bytes.len() - 1 {
        assert!(import(&bytes[..end]).is_err(), "truncated at {end}");
    }
    for at in 0..bytes.len() {
        for flip in [0x01u8, 0x20, 0x80] {
            let mut mutated = bytes.clone();
            mutated[at] ^= flip;
            let _ = import(&mutated);
        }
    }
}

#[test]
fn every_emitted_golden_reimports_to_the_same_bytes() {
    let mut read = 0;
    for entry in std::fs::read_dir(fixture("emit")).expect("goldens") {
        let path = entry.expect("entry").path();
        let bytes = std::fs::read(&path).expect("read");
        let imported = import(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(imported.report.is_empty(), "{}", path.display());
        assert_eq!(
            emit(&imported.sequence, &EmitOptions::new()).expect("emit"),
            bytes,
            "{}",
            path.display()
        );
        read += 1;
    }
    // The every-element golden (enumeration parameters, bounds, constant members) is among them.
    assert!(read >= 2, "{read}");
}

#[test]
fn the_options_set_derived_from_and_nothing_else() {
    let bytes = document().into_bytes();
    let plain = import(&bytes).expect("import").sequence;
    assert_eq!(plain.derived_from, None);
    let id =
        oce_author::ProvenanceId::new("cxf:fnv1a128:00112233445566778899aabbccddeeff").expect("id");
    let options = ImportOptions::new().with_derived_from(id.clone());
    let mut with = import_cxf(&bytes, &options).expect("import").sequence;
    assert_eq!(with.derived_from, Some(id));
    with.derived_from = None;
    assert_eq!(with, plain);
}
