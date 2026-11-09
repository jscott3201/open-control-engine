//! The load gate refuses non-canonical array and matrix element names (GitHub #470): an index with
//! a sign, a leading zero or zero. Without it, `TimeTable` would read `table_01_2` and `table_1_2`
//! as one cell and keep the later in table order, two documents differing only in that order would
//! execute differently, and the canonical content id, which sorts parameters by name, could not
//! tell them apart. The canonical spellings still load. GitHub #467 (names outside the declared
//! extent) is the related facade gap; this file covers only the spelling half.

use oce_api::{DiagCode, Engine, OcError};

const BASE: &str = "http://example.org#spell";

fn real(value: f64) -> String {
    format!(
        "{{ \"@value\": \"{value:?}\", \"@type\": \"http://www.w3.org/2001/XMLSchema#double\" }}"
    )
}

/// One block of `class` with parameters `params` (Integer for the widths `nin` and `nout`, Real
/// otherwise), Real inputs `inputs` each driven by a `Reals.Sources.Constant`, and Real outputs
/// `outputs`.
fn document(class: &str, params: &[(&str, f64)], inputs: &[&str], outputs: &[&str]) -> String {
    let mut nodes = Vec::new();
    let mut members = vec![format!("{{ \"@id\": \"{BASE}.blk\" }}")];
    let id = |name: &str| format!("{{ \"@id\": \"{BASE}.blk.{name}\" }}");
    for (name, value) in params {
        let (kind, literal) = if matches!(*name, "nin" | "nout") {
            ("Integer", format!("{}", *value as i64))
        } else {
            ("Real", real(*value))
        };
        nodes.push(format!(
            "{{ \"@id\": \"{BASE}.blk.{name}\", \"@type\": \"S231:Parameter\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:{kind}\" }}, \"S231:value\": {literal} }}"
        ));
    }
    for name in inputs {
        nodes.push(format!(
            "{{ \"@id\": \"{BASE}.blk.{name}\", \"@type\": \"S231:RealInput\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }} }}"
        ));
        let driver = format!("{BASE}.drv_{name}");
        members.push(format!("{{ \"@id\": \"{driver}\" }}"));
        nodes.push(format!(
            "{{ \"@id\": \"{driver}\", \"@type\": \
             \"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant\", \
             \"S231:hasParameter\": [{{ \"@id\": \"{driver}.k\" }}], \
             \"S231:hasOutput\": [{{ \"@id\": \"{driver}.y\" }}] }}, \
             {{ \"@id\": \"{driver}.k\", \"@type\": \"S231:Parameter\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }}, \"S231:value\": {} }}, \
             {{ \"@id\": \"{driver}.y\", \"@type\": \"S231:RealOutput\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }}, \
             \"S231:isConnectedTo\": {{ \"@id\": \"{BASE}.blk.{name}\" }} }}",
            real(1.0)
        ));
    }
    for name in outputs {
        nodes.push(format!(
            "{{ \"@id\": \"{BASE}.blk.{name}\", \"@type\": \"S231:RealOutput\", \
             \"S231:isOfDataType\": {{ \"@id\": \"S231:Real\" }} }}"
        ));
    }
    let list = |names: Vec<String>| names.join(", ");
    format!(
        "{{ \"@context\": {{ \"S231\": \"http://data.ashrae.org/S231P#\" }}, \"@graph\": [\
         {{ \"@id\": \"{BASE}\", \"@type\": \"S231:Block\", \"S231:containsBlock\": [{}] }}, \
         {{ \"@id\": \"{BASE}.blk\", \
            \"@type\": \"http://example.org#Buildings.Controls.OBC.{class}\", \
            \"S231:hasParameter\": [{}], \"S231:hasInput\": [{}], \"S231:hasOutput\": [{}] }}, \
         {} ] }}",
        members.join(", "),
        list(params.iter().map(|(name, _)| id(name)).collect()),
        list(inputs.iter().map(|name| id(name)).collect()),
        list(outputs.iter().map(|name| id(name)).collect()),
        nodes.join(", ")
    )
}

/// A 2-row, one-value-column time table, plus `extra`.
fn time_table(extra: &[(&'static str, f64)]) -> String {
    let mut params = vec![
        ("table_1_1", 0.0),
        ("table_1_2", 1.0),
        ("table_2_1", 10.0),
        ("table_2_2", 2.0),
    ];
    params.extend_from_slice(extra);
    document("CDL.Reals.Sources.TimeTable", &params, &[], &["y1"])
}

fn refusal(document: &str) -> Option<Vec<String>> {
    match Engine::new().load_cxf(document.as_bytes()) {
        Ok(_) => None,
        Err(error) => Some(
            error
                .all_diagnostics()
                .map(|diagnostic| format!("{}:{}", diagnostic.code.as_str(), diagnostic.message))
                .collect(),
        ),
    }
}

fn assert_spelling_refused(document: &str, name: &str) {
    let error = Engine::new()
        .load_cxf(document.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("{name}: a non-canonical element name must refuse the load"));
    assert!(
        matches!(error, OcError::Validate(_) | OcError::LoadContext(_)),
        "{name}: {error:?}"
    );
    let codes: Vec<DiagCode> = error.all_diagnostics().map(|d| d.code).collect();
    assert!(
        codes.contains(&DiagCode::NonCanonicalElementName),
        "{name}: {codes:?}"
    );
    assert!(
        error
            .all_diagnostics()
            .any(|d| d.code == DiagCode::NonCanonicalElementName && d.message.contains(name)),
        "{name}: the diagnostic names the parameter"
    );
}

#[test]
fn canonical_spellings_load_and_have_a_content_id() {
    for document in [
        time_table(&[]),
        time_table(&[("offset_1", 0.5)]),
        document(
            "CDL.Reals.MultiSum",
            &[("nin", 2.0), ("k_1", 1.0), ("k_2", 2.0)],
            &["u1", "u2"],
            &["y"],
        ),
        document(
            "CDL.Reals.MatrixGain",
            &[
                ("nin", 2.0),
                ("nout", 2.0),
                ("K_1_1", 1.0),
                ("K_1_2", 0.0),
                ("K_2_1", 0.0),
                ("K_2_2", 1.0),
            ],
            &["u1", "u2"],
            &["y1", "y2"],
        ),
    ] {
        assert_eq!(refusal(&document), None, "{document}");
        let mut engine = Engine::new();
        engine.load_cxf(document.as_bytes()).unwrap();
        engine.content_id().unwrap();
    }
}

#[test]
fn an_aliased_table_cell_is_refused_alone_or_beside_its_canonical_spelling() {
    for name in [
        "table_01_2",
        "table_+1_2",
        "table_1_02",
        "table_-1_2",
        "table_0_2",
    ] {
        assert_spelling_refused(&time_table(&[(name, 5.0)]), name);
    }
    // Alone: the aliased spelling replaces the canonical cell instead of sitting beside it.
    let alone = document(
        "CDL.Reals.Sources.TimeTable",
        &[
            ("table_1_1", 0.0),
            ("table_01_2", 1.0),
            ("table_2_1", 10.0),
            ("table_2_2", 2.0),
        ],
        &[],
        &["y1"],
    );
    assert_spelling_refused(&alone, "table_01_2");
}

#[test]
fn an_aliased_offset_is_refused() {
    for name in ["offset_01", "offset_+1"] {
        assert_spelling_refused(&time_table(&[("offset_1", 0.5), (name, 0.25)]), name);
        assert_spelling_refused(&time_table(&[(name, 0.25)]), name);
    }
}

#[test]
fn a_width_driven_and_a_matrix_element_alias_are_refused() {
    for name in ["k_01", "k_+1"] {
        let doc = document(
            "CDL.Reals.MultiSum",
            &[("nin", 2.0), ("k_1", 1.0), ("k_2", 2.0), (name, 3.0)],
            &["u1", "u2"],
            &["y"],
        );
        assert_spelling_refused(&doc, name);
    }
    let doc = document(
        "CDL.Reals.MatrixGain",
        &[
            ("nin", 2.0),
            ("nout", 2.0),
            ("K_1_1", 1.0),
            ("K_1_2", 0.0),
            ("K_2_1", 0.0),
            ("K_2_2", 1.0),
            ("K_01_1", 7.0),
        ],
        &["u1", "u2"],
        &["y1", "y2"],
    );
    assert_spelling_refused(&doc, "K_01_1");
}
