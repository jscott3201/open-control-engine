//! Enumeration-typed sequence parameters and member literals (importer note, decisions 1 to 3
//! and 10; issue #492), generated from the facade's one table (`oce_api::enumerations()`), never
//! hand-copied: every enumeration type and every constant package survives wire → model → wire and
//! text → model → text with every member, validates clean, emits as the corpus spells it and
//! loads warning-free; a reference type-checks by enumeration class; and each refusal has its
//! exact code.

mod support;

use oce_api::{EnumerationClass, EnumerationKind, MemberValue, Value, ValueType, enumerations};
use oce_author::{
    Assignment, Block, Boundary, CLASS_BASE, Connection, EmitOptions, Endpoint, Literal, MemberRef,
    ParamValue, ParameterType, Sequence, SequenceParameter, SignalType, emit, from_wire_str,
    parse_text, print_text, to_wire_string, validate, validate_author_side,
};

use support::cxf::load_clean;
use support::{codes, id, member};

/// A one-block clean sequence (a Real constant driving an output) to carry parameters.
fn carrier() -> Sequence {
    let mut s = Sequence::new(id("named"));
    s.outputs.push(Boundary::new(id("y"), SignalType::Real));
    let mut source = Block::new(id("src"), "CDL.Reals.Sources.Constant");
    source
        .assignments
        .push(Assignment::new(id("k"), Literal::Real(1.0)));
    s.blocks.push(source);
    s.connections.push(Connection::new(
        Endpoint::parse("src.y"),
        Endpoint::parse("y"),
    ));
    s
}

/// An Integer constant block `name` with `k = value`, driving a new Integer output `y_<name>`.
fn integer_constant(s: &mut Sequence, name: &str, value: ParamValue) {
    let mut block = Block::new(id(name), "CDL.Integers.Sources.Constant");
    block.assignments.push(Assignment::new(id("k"), value));
    s.blocks.push(block);
    let output = format!("y_{name}");
    s.outputs
        .push(Boundary::new(id(&output), SignalType::Integer));
    s.connections.push(Connection::new(
        Endpoint::parse(&format!("{name}.y")),
        Endpoint::parse(&output),
    ));
}

fn rows(kind: EnumerationKind) -> Vec<&'static EnumerationClass> {
    enumerations().iter().filter(|r| r.kind == kind).collect()
}

/// Both forms carry `s` exactly.
fn round_trips(s: &Sequence) {
    let wire = to_wire_string(s);
    let read = from_wire_str(&wire).unwrap_or_else(|e| panic!("{wire}\n{e}"));
    assert_eq!(&read, s);
    assert_eq!(to_wire_string(&read), wire);
    let text = print_text(s).expect("spellable");
    let parsed = parse_text(&text).unwrap_or_else(|e| panic!("{text}\n{e}"));
    assert_eq!(&parsed, s, "{text}");
    assert_eq!(print_text(&parsed).expect("spellable"), text);
}

/// The `@graph` node of `iri` in an emitted document.
fn node(bytes: &[u8], iri: &str) -> serde_json::Value {
    let document: serde_json::Value = serde_json::from_slice(bytes).expect("JSON");
    document["@graph"]
        .as_array()
        .expect("graph")
        .iter()
        .find(|n| n["@id"] == iri)
        .unwrap_or_else(|| panic!("{iri} emitted"))
        .clone()
}

#[test]
fn the_table_has_both_kinds_and_the_schema_lists_only_enumerations() {
    let enums = rows(EnumerationKind::Enumeration);
    let packages = rows(EnumerationKind::ConstantPackage);
    assert!(enums.len() >= 4 && !packages.is_empty());
    let names = oce_author::parameter_type_names();
    assert_eq!(&names[..3], ["Real", "Integer", "Boolean"]);
    let expected: Vec<&str> = enums.iter().map(|r| r.type_name()).collect();
    assert_eq!(&names[3..], expected.as_slice());
    for package in &packages {
        assert!(
            !names.contains(&package.type_name()),
            "{}",
            package.class_path
        );
    }
}

#[test]
fn every_enumeration_member_round_trips_validates_and_loads_as_the_corpus_spells_it() {
    let mut checked = 0;
    for row in rows(EnumerationKind::Enumeration) {
        for name in row.members {
            let mut s = carrier();
            s.params.push(SequenceParameter::new(
                id("p"),
                ParameterType::parse(row.type_name()),
                member(&format!("{}.{name}", row.type_name())),
            ));
            round_trips(&s);
            assert_eq!(validate(&s), vec![], "{}.{name}", row.type_name());
            let bytes = emit(&s, &EmitOptions::new()).expect("emit");
            let param = node(&bytes, "urn:oce:author:named.p");
            assert_eq!(
                param["S231:isOfDataType"]["@id"],
                format!("{CLASS_BASE}{}", row.class_path)
            );
            assert_eq!(
                param["S231:value"],
                format!("Buildings.Controls.OBC.{}.{name}", row.class_path)
            );
            load_clean(&bytes);
            checked += 1;
        }
    }
    assert!(checked > 40, "{checked}");
}

#[test]
fn every_constant_member_round_trips_validates_and_grounds_to_its_integer() {
    let mut checked = 0;
    for row in rows(EnumerationKind::ConstantPackage) {
        for (name, value) in row.members.iter().zip(row.values) {
            let spelling = format!("{}.{name}", row.type_name());
            let mut s = carrier();
            s.params.push(SequenceParameter::new(
                id("p"),
                SignalType::Integer,
                member(&spelling),
            ));
            integer_constant(&mut s, "lit", ParamValue::Literal(member(&spelling)));
            integer_constant(&mut s, "via", ParamValue::Ref(id("p")));
            round_trips(&s);
            assert_eq!(validate(&s), vec![], "{spelling}");
            let bytes = emit(&s, &EmitOptions::new()).expect("emit");
            // Ruling 9: the package IRI is the `isOfDataType`, the full member path the value.
            for iri in ["urn:oce:author:named.p", "urn:oce:author:named.lit.k"] {
                let param = node(&bytes, iri);
                assert_eq!(
                    param["S231:isOfDataType"]["@id"],
                    format!("{CLASS_BASE}{}", row.class_path),
                    "{iri}"
                );
                assert_eq!(
                    param["S231:value"],
                    format!("Buildings.Controls.OBC.{}.{name}", row.class_path)
                );
            }
            let engine = load_clean(&bytes);
            for block in ["lit", "via"] {
                let path = format!("urn:oce:author:named.{block}");
                let loaded = engine
                    .topology()
                    .blocks
                    .into_iter()
                    .find(|b| b.instance_path == path)
                    .expect("block loads");
                assert!(
                    matches!(
                        loaded.params.as_slice(),
                        [(k, Value::Integer(v))] if k == "k" && v == value
                    ),
                    "{spelling} via {block}: {:?}",
                    loaded.params
                );
            }
            checked += 1;
        }
    }
    assert!(checked > 10, "{checked}");
}

/// Every (class, parameter) of the catalog whose type is enumeration `id`, settable on an
/// authorable class.
fn enum_slots(id: oce_api::EnumClassId) -> Vec<(&'static str, &'static str)> {
    let authorable = oce_author::authorable_class_paths();
    oce_api::class_metadata()
        .iter()
        .filter(|class| authorable.contains(&class.class_path))
        .flat_map(|class| {
            class
                .parameters
                .iter()
                .filter(move |p| p.settable && p.value_type == ValueType::Enum(id))
                .map(move |p| (class.class_path, p.name))
        })
        .collect()
}

/// The findings of `s` at `subject`.
fn at(s: &Sequence, subject: &str) -> Vec<&'static str> {
    validate_author_side(s)
        .into_iter()
        .filter(|f| f.subject == subject)
        .map(|f| f.code)
        .collect()
}

#[test]
fn a_reference_type_checks_by_enumeration_class_on_every_catalog_slot() {
    let enums = rows(EnumerationKind::Enumeration);
    let mut slots = 0;
    for row in &enums {
        let Some(class_id) = row.id else { continue };
        let other = enums
            .iter()
            .find(|r| r.id != row.id)
            .expect("a second enumeration");
        for (class_path, parameter) in enum_slots(class_id) {
            for (declared, expected) in [
                (*row, Vec::<&str>::new()),
                (*other, vec!["parameter-kind-mismatch"]),
            ] {
                let mut s = carrier();
                s.params.push(SequenceParameter::new(
                    id("p"),
                    ParameterType::parse(declared.type_name()),
                    member(&format!("{}.{}", declared.type_name(), declared.members[0])),
                ));
                let mut block = Block::new(id("b"), class_path);
                block
                    .assignments
                    .push(Assignment::new(id(parameter), ParamValue::Ref(id("p"))));
                s.blocks.push(block);
                assert_eq!(
                    at(&s, &format!("b.{parameter}")),
                    expected,
                    "{class_path}.{parameter} from a {} parameter",
                    declared.type_name()
                );
                assert_eq!(at(&s, "p"), Vec::<&str>::new());
            }
            slots += 1;
        }
    }
    assert!(slots >= 4, "{slots} catalog slots");
}

#[test]
fn an_enumeration_reference_grounds_to_the_members_ordinal() {
    let mut s = carrier();
    s.params.push(SequenceParameter::new(
        id("conTyp"),
        ParameterType::parse("SimpleController"),
        member("SimpleController.PID"),
    ));
    s.inputs.push(Boundary::new(id("uSet"), SignalType::Real));
    s.inputs.push(Boundary::new(id("uMea"), SignalType::Real));
    s.outputs.push(Boundary::new(id("yCon"), SignalType::Real));
    let mut pid = Block::new(id("con"), "CDL.Reals.PID");
    pid.assignments.push(Assignment::new(
        id("controllerType"),
        ParamValue::Ref(id("conTyp")),
    ));
    s.blocks.push(pid);
    for (from, to) in [("uSet", "con.u_s"), ("uMea", "con.u_m"), ("con.y", "yCon")] {
        s.connections
            .push(Connection::new(Endpoint::parse(from), Endpoint::parse(to)));
    }
    assert_eq!(validate(&s), vec![]);
    let engine = load_clean(&emit(&s, &EmitOptions::new()).expect("emit"));
    let pid = engine
        .topology()
        .blocks
        .into_iter()
        .find(|b| b.instance_path == "urn:oce:author:named.con")
        .expect("pid");
    let (_, value) = pid
        .params
        .iter()
        .find(|(n, _)| n == "controllerType")
        .expect("grounded");
    let simple = oce_api::enumeration("CDL.Types.SimpleController").expect("row");
    let Some(MemberValue::Ordinal(ordinal)) = simple.member_value("PID") else {
        panic!("PID is an ordinal")
    };
    assert_eq!(ordinal, 4, "the upstream order P, PI, PD, PID");
    let class = simple.id.expect("enumeration id");
    assert!(
        matches!(value, Value::Enum { class: c, ordinal: o } if *c == class && *o == ordinal),
        "{value:?}"
    );
}

fn with_param(value_type: ParameterType, value: Literal) -> Sequence {
    let mut s = carrier();
    s.params
        .push(SequenceParameter::new(id("p"), value_type, value));
    s
}

fn with_assignment(class: &str, parameter: &str, value: ParamValue) -> Sequence {
    let mut s = carrier();
    s.params.push(SequenceParameter::new(
        id("conTyp"),
        ParameterType::parse("SimpleController"),
        member("SimpleController.PI"),
    ));
    s.params.push(SequenceParameter::new(
        id("mode"),
        SignalType::Integer,
        member("OperationModes.occupied"),
    ));
    s.params.push(SequenceParameter::new(
        id("gain"),
        SignalType::Real,
        Literal::Real(0.5),
    ));
    let mut block = Block::new(id("b"), class);
    block
        .assignments
        .push(Assignment::new(id(parameter), value));
    s.blocks.push(block);
    s
}

/// A finding as `(code, subject)`.
type Pair = (&'static str, String);

/// A refusal case: what it is, the sequence and the findings expected.
type Case = (&'static str, Sequence, Vec<Pair>);

fn pair(code: &'static str, subject: &str) -> Pair {
    (code, subject.to_owned())
}

#[test]
fn each_named_type_refusal_has_its_code() {
    let enum_type = ParameterType::parse("SimpleController");
    let cases: Vec<Case> = vec![
        (
            "an unknown type name",
            with_param(
                ParameterType::parse("SimpleControllers"),
                member("SimpleControllers.PI"),
            ),
            vec![pair("unknown-enum-type", "p")],
        ),
        (
            "a constant package as a type",
            with_param(
                ParameterType::parse("OperationModes"),
                member("OperationModes.occupied"),
            ),
            vec![pair("unknown-enum-type", "p")],
        ),
        (
            "a scalar spelled in lowercase is a type name",
            with_param(ParameterType::parse("real"), Literal::Real(1.0)),
            vec![pair("unknown-enum-type", "p")],
        ),
        (
            "an Integer on an enumeration parameter",
            with_param(enum_type.clone(), Literal::Integer(2)),
            vec![pair("parameter-kind-mismatch", "p")],
        ),
        (
            "a member of another enumeration",
            with_param(enum_type.clone(), member("Smoothness.LinearSegments")),
            vec![pair("unknown-enum-literal", "p")],
        ),
        (
            "an unknown member",
            with_param(enum_type.clone(), member("SimpleController.PIX")),
            vec![pair("unknown-enum-literal", "p")],
        ),
        (
            "a constant on an enumeration parameter",
            with_param(enum_type, member("OperationModes.occupied")),
            vec![pair("unknown-enum-literal", "p")],
        ),
        (
            "an enumeration member on an Integer parameter",
            with_param(ParameterType::Integer, member("SimpleController.PI")),
            vec![pair("parameter-kind-mismatch", "p")],
        ),
        (
            "an unknown constant",
            with_param(ParameterType::Integer, member("OperationModes.asleep")),
            vec![pair("unknown-enum-literal", "p")],
        ),
        (
            "an unknown package",
            with_param(ParameterType::Integer, member("OperationMode.occupied")),
            vec![pair("unknown-enum-literal", "p")],
        ),
        (
            "a constant on a Real parameter",
            with_param(ParameterType::Real, member("OperationModes.occupied")),
            vec![pair("parameter-kind-mismatch", "p")],
        ),
        (
            "an enumeration reference on a Real block parameter",
            with_assignment(
                "CDL.Reals.MultiplyByParameter",
                "k",
                ParamValue::Ref(id("conTyp")),
            ),
            vec![pair("parameter-kind-mismatch", "b.k")],
        ),
        (
            "a Real reference on an enumeration block parameter",
            with_assignment(
                "CDL.Reals.PID",
                "controllerType",
                ParamValue::Ref(id("gain")),
            ),
            vec![pair("parameter-kind-mismatch", "b.controllerType")],
        ),
        (
            "a constant reference on an enumeration block parameter",
            with_assignment(
                "CDL.Reals.PID",
                "controllerType",
                ParamValue::Ref(id("mode")),
            ),
            vec![pair("parameter-kind-mismatch", "b.controllerType")],
        ),
        (
            "an enumeration reference on an Integer block parameter",
            with_assignment(
                "CDL.Integers.Sources.Constant",
                "k",
                ParamValue::Ref(id("conTyp")),
            ),
            vec![pair("parameter-kind-mismatch", "b.k")],
        ),
        (
            "a constant literal on an enumeration block parameter",
            with_assignment(
                "CDL.Reals.PID",
                "controllerType",
                ParamValue::Literal(member("OperationModes.occupied")),
            ),
            vec![pair("unknown-enum-literal", "b.controllerType")],
        ),
        (
            "an enumeration literal on an Integer block parameter",
            with_assignment(
                "CDL.Integers.Sources.Constant",
                "k",
                ParamValue::Literal(member("SimpleController.PI")),
            ),
            vec![pair("parameter-kind-mismatch", "b.k")],
        ),
        (
            "a constant literal on a Real block parameter",
            with_assignment(
                "CDL.Reals.MultiplyByParameter",
                "k",
                ParamValue::Literal(member("OperationModes.occupied")),
            ),
            vec![pair("parameter-kind-mismatch", "b.k")],
        ),
        (
            "an unknown constant on an Integer block parameter",
            with_assignment(
                "CDL.Integers.Sources.Constant",
                "k",
                ParamValue::Literal(member("ZoneStates.idle")),
            ),
            vec![pair("unknown-enum-literal", "b.k")],
        ),
    ];
    for (case, s, expected) in cases {
        let found: Vec<Pair> = codes(&validate_author_side(&s))
            .into_iter()
            .filter(|(_, subject)| subject == "p" || subject.starts_with("b."))
            .collect();
        assert_eq!(found, expected, "{case}");
    }
}

#[test]
fn a_reference_to_a_broken_parameter_is_reported_once_at_the_parameter() {
    let mut s = with_assignment(
        "CDL.Reals.PID",
        "controllerType",
        ParamValue::Ref(id("conTyp")),
    );
    s.params[0].value = member("SimpleController.PIX");
    assert_eq!(
        codes(&validate_author_side(&s)),
        vec![pair("unknown-enum-literal", "conTyp")]
    );
}

#[test]
fn every_unknown_type_and_package_meets_the_engines_code_when_emitted_unchecked() {
    // The validator refuses an unknown enumeration type before anything is emitted; the parity
    // test (src/validate/engine_tests.rs) pins that the engine reports the same code for it. A
    // constant package as a type is refused by the author side alone: the engine reads the
    // package IRI as Integer and would load it.
    for package in rows(EnumerationKind::ConstantPackage) {
        let s = with_param(
            ParameterType::parse(package.type_name()),
            member(&format!("{}.{}", package.type_name(), package.members[0])),
        );
        let findings = validate(&s);
        assert_eq!(codes(&findings), vec![pair("unknown-enum-type", "p")]);
        assert!(
            findings[0].detail.contains("never a parameter type"),
            "{}",
            findings[0].detail
        );
    }
}

#[test]
fn hostile_type_names_and_members_are_typed_findings_or_refusals() {
    let long = "T".repeat(5_000);
    for spelling in [
        "",
        " ",
        "a.b",
        "Real ",
        "\u{202e}x",
        long.as_str(),
        "SimpleController.",
    ] {
        let s = with_param(
            ParameterType::Enum(spelling.to_owned()),
            member("SimpleController.PI"),
        );
        // The wire form carries any string; the validator names it; the printer refuses what it
        // cannot spell rather than printing another model.
        let findings = validate_author_side(&s);
        assert!(
            findings.iter().any(|f| f.code == "unknown-enum-type"),
            "{spelling:?}: {findings:?}"
        );
        if let Ok(text) = print_text(&s) {
            assert_eq!(parse_text(&text).as_ref(), Ok(&s), "{spelling:?}");
        }
    }
    // A member spelling the text form cannot read back is refused by the printer.
    let s = with_param(
        ParameterType::Integer,
        Literal::Member(MemberRef::parse("Operation Modes.occupied")),
    );
    assert!(print_text(&s).is_err());
}

#[test]
fn a_parameter_type_text_needs_a_word() {
    for (text, code) in [
        ("param p : 7 = 1\n", "stray-token"),
        (
            "param p : \"SimpleController\" = SimpleController.PI\n",
            "stray-token",
        ),
        (
            "param p : SimpleController.PI = SimpleController.PI\n",
            "stray-token",
        ),
    ] {
        let source = format!("sequence s\nextends none\n{text}");
        let error = parse_text(&source).expect_err(text);
        assert_eq!(error.code, code, "{text}: {error}");
    }
    let parsed = parse_text("sequence s\nextends none\nparam p : Foo = Foo.bar\n").expect("parses");
    assert_eq!(
        parsed.params[0].value_type,
        ParameterType::Enum("Foo".to_owned())
    );
}
