//! Values the CXF importer reads (importer note, "The CXF importer" and "Test plan", hostile
//! corpus): boundary bounds converted as the loader converts them and refused with their exact
//! kind when they are no number literal of the boundary's type; enumeration-typed sequence
//! parameters and library constants resolved through the facade table, and every mismatch
//! refused; a package-typed Integer boundary normalised and reported, a package-typed Real or
//! Boolean one refused. Edits go through `support::edit` on the every-element golden, which
//! carries every element the model has.

use crate::support;

use oce_author::{
    Bound, EmitOptions, ImportError, ImportOptions, Imported, Literal, ParamValue, SignalType,
    emit, import_cxf, validate_author_side,
};
use serde_json::{Value, json};

use support::edit::{Document, reference, typed};
use support::fixture;

const ROOT: &str = "urn:oce:author:every_element";

fn id(local: &str) -> String {
    format!("{ROOT}.{local}")
}

fn golden() -> Document {
    Document::read(&fixture("emit/every_element.cxf.json"))
}

fn import(bytes: &[u8]) -> Result<Imported, ImportError> {
    import_cxf(bytes, &ImportOptions::new())
}

/// The golden with `key` on `local` set to `value`.
fn with(local: &str, key: &str, value: Value) -> Vec<u8> {
    golden().put(&id(local), key, value).bytes()
}

/// Assert `bytes` is refused with `code` at the node `local`.
fn refused(bytes: &[u8], code: &str, local: &str) {
    match import(bytes) {
        Err(error) => assert_eq!(
            (error.code(), error.subject.as_str()),
            (code, id(local).as_str()),
            "{error}"
        ),
        Ok(_) => panic!("expected `{code}` at `{local}`, but it imported"),
    }
}

fn boundary_bounds(bytes: &[u8], name: &str) -> (Option<Bound>, Option<Bound>) {
    let sequence = import(bytes).expect("imports").sequence;
    let boundary = sequence
        .inputs
        .iter()
        .chain(&sequence.outputs)
        .find(|b| b.name.as_str() == name)
        .expect("the boundary");
    (boundary.min, boundary.max)
}

fn stage_one_codes(bytes: &[u8]) -> Vec<&'static str> {
    let sequence = import(bytes).expect("imports").sequence;
    validate_author_side(&sequence)
        .iter()
        .map(|f| f.code)
        .collect()
}

#[test]
fn a_bound_that_is_no_number_literal_is_refused_with_its_kind() {
    for (value, code) in [
        (json!("1"), "expression-value"),
        (json!("TSupMax"), "expression-value"),
        (
            json!("Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes.occupied"),
            "expression-value",
        ),
        (typed("1e999", "double"), "malformed-literal"),
        (typed("NaN", "double"), "malformed-literal"),
        (typed("inf", "double"), "malformed-literal"),
        (typed("353.15 ", "double"), "malformed-literal"),
        (typed("353", "date"), "malformed-literal"),
        (typed("9223372036854775808", "integer"), "malformed-literal"),
        (json!([353.15]), "wrong-shape"),
        (json!({"@value": "1.0"}), "wrong-shape"),
        (Value::Null, "wrong-shape"),
        (json!(true), "datatype-mismatch"),
        (typed("true", "boolean"), "datatype-mismatch"),
    ] {
        refused(&with("TSup", "S231:max", value), code, "TSup");
    }
    // A bare `1e999` overflows binary64: the strict reader refuses it before any node is read.
    let bytes = golden()
        .put(&id("TSup"), "S231:max", json!("__BARE__"))
        .bytes_with_raw("__BARE__", "1e999");
    assert_eq!(
        import(&bytes).map(|_| ()).expect_err("refused").code(),
        "not-strict-json"
    );
}

#[test]
fn a_bound_disagreeing_with_its_boundary_type_is_a_datatype_mismatch() {
    // A bound on a Boolean boundary; a Real literal on an Integer boundary.
    refused(
        &with("uEna", "S231:min", json!(0)),
        "datatype-mismatch",
        "uEna",
    );
    refused(
        &with("yStage", "S231:max", json!(3.5)),
        "datatype-mismatch",
        "yStage",
    );
    refused(
        &with("yStage", "S231:max", typed("3.0", "double")),
        "datatype-mismatch",
        "yStage",
    );
    // An Integer literal, bare or typed, on an Integer boundary is the Integer.
    for value in [json!(-7), typed("-7", "integer"), typed("-7", "long")] {
        assert_eq!(
            boundary_bounds(&with("yStage", "S231:min", value), "yStage").0,
            Some(Bound::Integer(-7))
        );
    }
}

#[test]
fn a_bound_on_a_real_boundary_is_read_as_the_loader_reads_it() {
    let exact = 2_i64.pow(53) + 1;
    for (value, expected) in [
        (json!(223), 223.0),
        (typed("223", "integer"), 223.0),
        (typed("223.0", "decimal"), 223.0),
        (typed("2.23e2", "float"), 223.0),
        (typed("-0.0", "double"), -0.0),
        (typed("5e-324", "double"), 5e-324),
        (typed("0.30000000000000004", "double"), 0.1 + 0.2),
        (json!(exact), exact as f64),
    ] {
        let (min, _) = boundary_bounds(&with("TSup", "S231:min", value.clone()), "TSup");
        assert!(
            matches!(min, Some(Bound::Real(v)) if v.to_bits() == expected.to_bits()),
            "{value}: {min:?}"
        );
    }
    // One-sided: either side alone.
    let bytes = golden().remove(&id("TSup"), "S231:min").bytes();
    assert_eq!(
        boundary_bounds(&bytes, "TSup"),
        (None, Some(Bound::Real(353.15)))
    );
}

#[test]
fn crossed_bounds_import_and_the_validator_refuses_them() {
    let bytes = golden()
        .put(&id("TSup"), "S231:min", typed("400.0", "double"))
        .put(&id("TSup"), "S231:max", typed("300.0", "double"))
        .bytes();
    assert_eq!(
        boundary_bounds(&bytes, "TSup"),
        (Some(Bound::Real(400.0)), Some(Bound::Real(300.0)))
    );
    assert!(stage_one_codes(&bytes).contains(&"crossed-bounds"));
}

#[test]
fn nominal_and_unbounded_on_a_boundary_are_refused_not_dropped() {
    for (key, value) in [
        ("S231:nominal", json!(300)),
        ("S231:unbounded", json!(true)),
    ] {
        let error = import(&with("TSup", key, value))
            .map(|_| ())
            .expect_err("refused");
        assert_eq!(
            error.kind,
            oce_author::ImportErrorKind::ConnectorAttribute {
                attribute: key.to_owned()
            }
        );
    }
}

#[test]
fn an_enumeration_parameter_takes_only_a_member_of_its_own_type() {
    for (value, code) in [
        (json!(1), "datatype-mismatch"),
        (typed("1", "integer"), "datatype-mismatch"),
        (json!(true), "datatype-mismatch"),
        (
            json!("Buildings.Controls.OBC.CDL.Types.Smoothness.ConstantSegments"),
            "datatype-mismatch",
        ),
        (
            json!("Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes.occupied"),
            "datatype-mismatch",
        ),
        (json!("\"PI\""), "datatype-mismatch"),
        (json!("PI"), "expression-value"),
        (
            json!("Buildings.Controls.OBC.CDL.Types.NoSuchType.PI"),
            "unknown-member-type",
        ),
        (
            json!("Buildings.Controls.OBC.CDL.Types.SimpleController."),
            "unknown-member-type",
        ),
        (
            json!("Buildings.Other.SimpleController.PI"),
            "unknown-member-type",
        ),
        (
            json!(["Buildings.Controls.OBC.CDL.Types.SimpleController.PI"]),
            "array-shape",
        ),
        (Value::Null, "wrong-shape"),
    ] {
        refused(&with("conTyp", "S231:value", value), code, "conTyp");
    }
    // An unknown member of a known type is carried for the validator.
    let bytes = with(
        "conTyp",
        "S231:value",
        json!("Buildings.Controls.OBC.CDL.Types.SimpleController.PIDX"),
    );
    assert!(stage_one_codes(&bytes).contains(&"unknown-enum-literal"));
}

#[test]
fn a_sequence_parameter_type_the_table_lacks_is_an_unknown_datatype() {
    for datatype in [
        "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.NoSuchType",
        "http://example.org#Buildings.Controls.OBC.CDL.Types",
        "S231:String",
        "S231:Bogus",
    ] {
        refused(
            &with("conTyp", "S231:isOfDataType", reference(datatype)),
            "unknown-datatype",
            "conTyp",
        );
    }
    let bytes = golden().remove(&id("kGain"), "S231:isOfDataType").bytes();
    refused(&bytes, "missing-key", "kGain");
    let bytes = golden().remove(&id("kGain"), "S231:value").bytes();
    refused(&bytes, "missing-key", "kGain");
}

#[test]
fn a_constant_member_fits_an_integer_and_nothing_else() {
    // On a package-typed Integer sequence parameter: a member of that package or an Integer.
    let read = import(&with("occMode", "S231:value", json!(3)))
        .expect("imports")
        .sequence;
    let occ = read
        .params
        .iter()
        .find(|p| p.name.as_str() == "occMode")
        .expect("occMode");
    assert_eq!(occ.value, Literal::Integer(3));
    refused(
        &with(
            "occMode",
            "S231:value",
            json!("Buildings.Controls.OBC.ASHRAE.G36.Types.ZoneStates.heating"),
        ),
        "datatype-mismatch",
        "occMode",
    );
    // On a Real sequence parameter, and on a Real block parameter that declares its type.
    let occupied = json!("Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes.occupied");
    refused(
        &with("kGain", "S231:value", occupied.clone()),
        "datatype-mismatch",
        "kGain",
    );
    refused(
        &with("conPID.k", "S231:value", occupied.clone()),
        "datatype-mismatch",
        "conPID.k",
    );
    // Without a declared type the block value is carried and the validator judges it.
    let bytes = golden()
        .put(&id("conPID.k"), "S231:value", occupied)
        .remove(&id("conPID.k"), "S231:isOfDataType")
        .bytes();
    assert!(stage_one_codes(&bytes).contains(&"parameter-kind-mismatch"));
}

#[test]
fn an_unknown_package_is_refused_and_an_unknown_member_is_carried() {
    refused(
        &with(
            "modCon.k",
            "S231:value",
            json!("Buildings.Controls.OBC.ASHRAE.G36.Types.NoSuchPackage.occupied"),
        ),
        "unknown-member-type",
        "modCon.k",
    );
    let bytes = with(
        "modCon.k",
        "S231:value",
        json!("Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes.nope"),
    );
    let sequence = import(&bytes).expect("imports").sequence;
    let block = sequence
        .blocks
        .iter()
        .find(|b| b.name.as_str() == "modCon")
        .expect("modCon");
    assert_eq!(
        block.assignments[0].value,
        ParamValue::Literal(support::member("OperationModes.nope"))
    );
    assert!(stage_one_codes(&bytes).contains(&"unknown-enum-literal"));
    // A block value typed by a package takes an Integer too; a Boolean is a mismatch.
    assert!(import(&with("modCon.k", "S231:value", json!(2))).is_ok());
    refused(
        &with("modCon.k", "S231:value", json!(true)),
        "datatype-mismatch",
        "modCon.k",
    );
}

#[test]
fn a_package_typed_integer_boundary_is_normalised_and_reported() {
    let package = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes";
    let read = import(&with("yStage", "S231:isOfDataType", reference(package))).expect("imports");
    let normalised: Vec<(&str, &str, SignalType)> = read
        .report
        .normalised
        .iter()
        .map(|n| (n.subject.as_str(), n.data_type.as_str(), n.to))
        .collect();
    assert_eq!(
        normalised,
        [(id("yStage").as_str(), package, SignalType::Integer)]
    );
    // Normalised, the document is the golden again, byte for byte.
    assert_eq!(
        emit(&read.sequence, &EmitOptions::new()).expect("emit"),
        std::fs::read(fixture("emit/every_element.cxf.json")).expect("golden")
    );
}

#[test]
fn a_package_or_enumeration_typed_real_or_boolean_boundary_is_a_datatype_mismatch() {
    let package = "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes";
    let enumeration = "http://example.org#Buildings.Controls.OBC.CDL.Types.SimpleController";
    for (local, datatype) in [
        ("TSup", package),
        ("uEna", package),
        ("yStage", enumeration),
        ("TSup", "S231:Integer"),
    ] {
        refused(
            &with(local, "S231:isOfDataType", reference(datatype)),
            "datatype-mismatch",
            local,
        );
    }
    refused(
        &with("TSup", "S231:isOfDataType", reference("S231:Bogus")),
        "unknown-datatype",
        "TSup",
    );
    refused(
        &with("TSup", "@type", json!("S231:StringInput")),
        "datatype-mismatch",
        "TSup",
    );
}
