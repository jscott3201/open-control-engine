//! Enumerations are named by their canonical class path, end to end through the facade (issue
//! #762, the owner's ruling of 2026-11-22; typing IRIs, issue #637).
//!
//! On `development` before this change the five spellings of `controllerType` the issue lists
//! (`Buildings.Controls.OBC.CDL.Types.SimpleController.PD`, `SimpleController.PD`,
//! `Foo.SimpleController.PD`, `…ASHRAE.G36.Types.SimpleController.PD`,
//! `Modelica.Blocks.Types.SimpleController.PD`) all loaded, as CDL `PD`, with one content id. Only
//! the first loads now; every other is `unknown-enum-type` naming the canonical path. The same
//! holds for a root parameter's value and declared type, and a compiled-schedule blob records the
//! durable class path the engine writes and nothing else.

// Shared helpers, declared once for this binary in its root `facade.rs`.
use crate::blob_image;
use crate::parameter_documents::{Param, Shape, document, load, loaded, param};

use oce_api::{BlobError, Value};

const CDL: &str = "Buildings.Controls.OBC.CDL.Types";
const SIMPLE_CONTROLLER: &str = "Buildings.Controls.OBC.CDL.Types.SimpleController";

const PID: Shape<'static> = Shape {
    class: "CDL.Reals.PID",
    output: "Real",
    inputs: &[("u_s", "Real"), ("u_m", "Real")],
};

/// The issue's spellings of the class part of `SimpleController.PD`, and the ruling's others.
const OTHER_PATHS: [&str; 8] = [
    "SimpleController",
    "Foo.SimpleController",
    "Buildings.Controls.OBC.ASHRAE.G36.Types.SimpleController",
    "Modelica.Blocks.Types.SimpleController",
    "CDL.Types.SimpleController",
    "Controls.OBC.CDL.Types.SimpleController",
    "buildings.controls.obc.cdl.types.SimpleController",
    "Foo.Buildings.Controls.OBC.CDL.Types.SimpleController",
];

fn quoted(text: &str) -> String {
    format!("\"{text}\"")
}

fn controller(literal: &str) -> Param {
    crate::parameter_documents::value("controllerType", literal)
}

/// A root parameter `p` with members `members`, and `controllerType = "p"`.
fn through_root(members: &str) -> String {
    document(
        &PID,
        &[controller("\"p\"")],
        &[Param {
            name: "p".to_owned(),
            members: members.to_owned(),
        }],
    )
}

/// The diagnostics of the refusal of `doc`: `(code, message)`.
fn refusal(doc: &str) -> Vec<(String, String)> {
    match load(doc) {
        Ok(_) => panic!("expected a refusal\n{doc}"),
        Err(error) => error
            .all_diagnostics()
            .map(|d| (d.code.as_str().to_owned(), d.message.clone()))
            .collect(),
    }
}

/// The controller type the loaded block holds: its one-based ordinal (`PD` is 3).
fn first_output(doc: &str) -> u32 {
    match param(&loaded(doc), "controllerType") {
        Value::Enum { ordinal, .. } => ordinal,
        other => panic!("not an enumeration value: {other:?}"),
    }
}

#[test]
fn only_the_canonical_spelling_of_simple_controller_pd_loads() {
    let canonical = format!("{SIMPLE_CONTROLLER}.PD");
    let pd = first_output(&document(&PID, &[controller(&quoted(&canonical))], &[]));
    let pi = first_output(&document(
        &PID,
        &[controller(&quoted(&format!("{SIMPLE_CONTROLLER}.PI")))],
        &[],
    ));
    assert_eq!((pd, pi), (3, 2), "the member written is the one held");
    for path in OTHER_PATHS {
        let doc = document(&PID, &[controller(&quoted(&format!("{path}.PD")))], &[]);
        let diags = refusal(&doc);
        let named: Vec<_> = diags
            .iter()
            .filter(|(code, _)| code == "unknown-enum-type")
            .collect();
        assert_eq!(named.len(), 1, "{path}: {diags:?}");
        assert!(
            named[0].1.contains(&format!("{SIMPLE_CONTROLLER}.PD")),
            "{path}: {}",
            named[0].1
        );
    }
}

#[test]
fn a_canonical_path_with_a_wrong_or_miscased_member_keeps_its_refusal() {
    for member in ["XX", "pd", "Pd", "PD.", ""] {
        let doc = document(
            &PID,
            &[controller(&quoted(&format!(
                "{SIMPLE_CONTROLLER}.{member}"
            )))],
            &[],
        );
        let codes: Vec<String> = refusal(&doc).into_iter().map(|(code, _)| code).collect();
        assert!(
            codes.iter().all(|code| code != "unknown-enum-type"),
            "{member:?}: the class is known, the member is not: {codes:?}"
        );
        assert!(!codes.is_empty(), "{member:?}");
    }
    // Another enumeration's canonical member keeps its kind refusal (issue #734, ruling R2).
    let doc = document(
        &PID,
        &[controller(&quoted(&format!(
            "{CDL}.Smoothness.ConstantSegments"
        )))],
        &[],
    );
    assert!(
        refusal(&doc)
            .iter()
            .any(|(code, _)| code == "parameter-kind-mismatch")
    );
}

#[test]
fn whitespace_around_an_expression_is_the_expressions_and_inside_a_path_is_refused() {
    // A value is an expression, and whitespace around one is not part of its tokens: the path
    // read is the canonical one (the authoring layer refuses any expression on a block
    // parameter, this one included).
    let canonical = format!("{SIMPLE_CONTROLLER}.PD");
    let padded = document(
        &PID,
        &[controller(&quoted(&format!("  {canonical}  ")))],
        &[],
    );
    let plain = document(&PID, &[controller(&quoted(&canonical))], &[]);
    assert_eq!(first_output(&padded), first_output(&plain));
    let split = document(
        &PID,
        &[controller(&quoted(&format!("{CDL} .SimpleController.PD")))],
        &[],
    );
    assert!(!refusal(&split).is_empty());
}

#[test]
fn a_root_parameter_reads_its_value_and_its_declared_type_by_the_canonical_path() {
    let typed = |datatype: &str, literal: &str| {
        format!(
            r#""@type": "S231:Parameter", "S231:isOfDataType": {{ "@id": "{datatype}" }}, "S231:value": {literal}"#
        )
    };
    let canonical_value = quoted(&format!("{SIMPLE_CONTROLLER}.PD"));
    let pd = first_output(&document(&PID, &[controller(&canonical_value)], &[]));
    // Declared in the authoring class base and in the S231 namespace: one meaning.
    for datatype in [
        format!("http://example.org#{SIMPLE_CONTROLLER}"),
        format!("http://data.ashrae.org/S231#{SIMPLE_CONTROLLER}"),
    ] {
        let out = first_output(&through_root(&typed(&datatype, &canonical_value)));
        assert_eq!(out, pd, "{datatype}");
    }
    // Untyped: the value as written, canonical.
    let untyped = format!(r#""@type": "S231:Parameter", "S231:value": {canonical_value}"#);
    assert_eq!(first_output(&through_root(&untyped)), pd);
    // A non-canonical value on a typed root.
    for path in OTHER_PATHS {
        let literal = quoted(&format!("{path}.PD"));
        let diags = refusal(&through_root(&typed(
            &format!("http://example.org#{SIMPLE_CONTROLLER}"),
            &literal,
        )));
        assert!(
            diags
                .iter()
                .any(|(code, message)| code == "unknown-enum-type"
                    && message.contains(&format!("{SIMPLE_CONTROLLER}.PD"))),
            "{path}: {diags:?}"
        );
    }
    // A non-canonical declared type (issue #637's mechanism).
    for path in OTHER_PATHS {
        let diags = refusal(&through_root(&typed(
            &format!("http://example.org#{path}"),
            &canonical_value,
        )));
        assert!(
            diags
                .iter()
                .any(|(code, message)| code == "unknown-enum-type"
                    && message.contains(SIMPLE_CONTROLLER)),
            "{path}: {diags:?}"
        );
    }
    // A typed root holding no member of its type: an ordinal, another enumeration's member, a
    // G36 constant. Refused on the declaration, before any block reads it.
    for literal in [
        "3".to_owned(),
        quoted(&format!("{CDL}.Smoothness.ConstantSegments")),
        quoted("Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes.occupied"),
    ] {
        let doc = through_root(&typed(
            &format!("http://example.org#{SIMPLE_CONTROLLER}"),
            &literal,
        ));
        assert!(
            refusal(&doc)
                .iter()
                .any(|(code, _)| code == "parameter-kind-mismatch"),
            "{literal}"
        );
    }
}

#[test]
fn a_blob_names_an_enumeration_by_its_durable_class_path_only() {
    let doc = document(
        &PID,
        &[controller(&quoted(&format!("{SIMPLE_CONTROLLER}.PD")))],
        &[],
    );
    let engine = loaded(&doc);
    let blob = engine.compile_blob().expect("compiles").bytes;
    oce_graph::blob::decode(&blob).expect("the engine's own blob decodes");
    let durable = b"CDL.Types.SimpleController";
    let at = blob
        .windows(durable.len())
        .position(|window| window == durable)
        .expect("the durable class path is recorded");
    // Same-length respellings: another package, another case.
    for respelt in [b"XDL.Types.SimpleController", b"CDL.Types.simpleController"] {
        let mut hostile = blob.clone();
        hostile[at..at + durable.len()].copy_from_slice(respelt);
        blob_image::rechecksum(&mut hostile);
        assert!(
            matches!(
                oce_graph::blob::decode(&hostile).err(),
                Some(BlobError::UnknownEnumClass { .. })
            ),
            "{}",
            String::from_utf8_lossy(respelt)
        );
    }
}

const TIME_TABLE: Shape<'static> = Shape {
    class: "CDL.Reals.Sources.TimeTable",
    output: "Real",
    inputs: &[],
};

/// The relative paths modelica-json (pinned 85721b82) copies from a source as written: CXF-Core's
/// `TimeTable` defaults (`third_party/modelica-buildings-cdl/cxf-core/CXF-Core.jsonld` line 8979,
/// `"CDL.Types.Extrapolation.Periodic"`, and line 8999, `"CDL.Types.Smoothness.LinearSegments"`,
/// from `Reals/Sources/TimeTable.mo` lines 6 and 8). A translation of a sequence that writes
/// one is refused in every context a value is read, naming the whole canonical member (the
/// owner's ruling of 2026-11-22 on issue #762: the engine stays canonical-only).
#[test]
fn a_translators_relative_paths_are_refused_in_every_context_naming_the_canonical_member() {
    let table = crate::parameter_documents::table("table", "[0.0, 0.0, 2.0, 2.0]");
    let cases = [
        (
            "extrapolation",
            "CDL.Types.Extrapolation.Periodic",
            "Extrapolation",
        ),
        (
            "smoothness",
            "CDL.Types.Smoothness.LinearSegments",
            "Smoothness",
        ),
    ];
    for (name, relative, type_name) in cases {
        let canonical = format!("Buildings.Controls.OBC.{relative}");
        let named = |diags: &[(String, String)], what: &str| {
            assert!(
                diags
                    .iter()
                    .any(|(code, message)| code == "unknown-enum-type"
                        && message.contains(&format!("write `{canonical}`"))),
                "{what} {relative}: {diags:?}"
            );
        };
        let block = |literal: &str, root: &[Param]| {
            let table = crate::parameter_documents::table("table", "[0.0, 0.0, 2.0, 2.0]");
            document(
                &TIME_TABLE,
                &[table, crate::parameter_documents::value(name, literal)],
                root,
            )
        };
        // The canonical member loads.
        loaded(&block(&quoted(&canonical), &[]));
        // A block parameter.
        named(&refusal(&block(&quoted(relative), &[])), "block parameter");
        // A typed root parameter, read by the block.
        let typed = Param {
            name: "p".to_owned(),
            members: format!(
                r#""@type": "S231:Parameter", "S231:isOfDataType": {{ "@id": "http://example.org#Buildings.Controls.OBC.CDL.Types.{type_name}" }}, "S231:value": "{relative}""#
            ),
        };
        named(&refusal(&block("\"p\"", &[typed])), "typed root");
        // An untyped root parameter, read by the block.
        let untyped = Param {
            name: "p".to_owned(),
            members: format!(r#""@type": "S231:Parameter", "S231:value": "{relative}""#),
        };
        named(&refusal(&block("\"p\"", &[untyped])), "untyped root");
    }
    drop(table);
}

/// A root parameter declared a CDL enumeration holds a member of that enumeration and nothing
/// else, even when no block reads it: another enumeration's canonical member, an ordinal, a
/// Boolean, a G36 constant are each refused on the declaration (`parameter-kind-mismatch`).
#[test]
fn a_declared_cdl_enumeration_holds_only_its_own_member_even_unread() {
    let unread = |literal: &str| {
        document(
            &PID,
            &[controller(&quoted(&format!("{SIMPLE_CONTROLLER}.PI")))],
            &[Param {
                name: "q".to_owned(),
                members: format!(
                    r#""@type": "S231:Parameter", "S231:isOfDataType": {{ "@id": "http://example.org#{CDL}.Smoothness" }}, "S231:value": {literal}"#
                ),
            }],
        )
    };
    loaded(&unread(&quoted(&format!(
        "{CDL}.Smoothness.ConstantSegments"
    ))));
    for literal in [
        quoted(&format!("{SIMPLE_CONTROLLER}.PD")),
        quoted(&format!("{CDL}.Extrapolation.Periodic")),
        "2".to_owned(),
        "true".to_owned(),
        quoted("Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes.occupied"),
    ] {
        let diags = refusal(&unread(&literal));
        assert!(
            diags
                .iter()
                .any(|(code, message)| code == "parameter-kind-mismatch"
                    && message.contains(&format!("{CDL}.Smoothness"))),
            "{literal}: {diags:?}"
        );
    }
}

/// A typing IRI names its enumeration by the exact canonical path after its last `#` or `/`:
/// a longer or shorter path, a member path, whitespace, or an IRI with neither separator before
/// the path (`urn:x:…`, which loaded before) types nothing and is refused.
#[test]
fn a_typing_iri_matches_the_canonical_path_exactly() {
    let typed = |datatype: &str| {
        through_root(&format!(
            r#""@type": "S231:Parameter", "S231:isOfDataType": {{ "@id": "{datatype}" }}, "S231:value": "{SIMPLE_CONTROLLER}.PD""#
        ))
    };
    for datatype in [
        format!("http://example.org#{SIMPLE_CONTROLLER}"),
        format!("http://example.org/{SIMPLE_CONTROLLER}"),
        format!("https://example.org/a/b#{SIMPLE_CONTROLLER}"),
    ] {
        assert_eq!(first_output(&typed(&datatype)), 3, "{datatype}");
    }
    for datatype in [
        format!("http://example.org#{SIMPLE_CONTROLLER}X"),
        format!("http://example.org#X{SIMPLE_CONTROLLER}"),
        format!("http://example.org#{SIMPLE_CONTROLLER}.PD"),
        format!("http://example.org#{SIMPLE_CONTROLLER} "),
        format!("urn:x:{SIMPLE_CONTROLLER}"),
        format!("http://example.org#{}", &SIMPLE_CONTROLLER[1..]),
    ] {
        assert!(load(&typed(&datatype)).is_err(), "{datatype} loaded");
    }
}

/// Whitespace around a canonical path on a CDL-typed or untyped root parameter, or a block
/// parameter, is the expression's own and loads, as it did before issue #762 (measured at
/// a765b37d); a G36-typed root keeps its profile refusal (`oce-cxf`
/// `canonical_enumeration_paths.rs`).
#[test]
fn whitespace_around_a_canonical_path_on_a_cdl_or_untyped_value_loads_as_before() {
    let pd = format!("{SIMPLE_CONTROLLER}.PD");
    for padded in [format!(" {pd}"), format!("{pd} "), format!(" {pd} ")] {
        let typed = format!(
            r#""@type": "S231:Parameter", "S231:isOfDataType": {{ "@id": "http://example.org#{SIMPLE_CONTROLLER}" }}, "S231:value": "{padded}""#
        );
        assert_eq!(first_output(&through_root(&typed)), 3, "typed {padded:?}");
        let untyped = format!(r#""@type": "S231:Parameter", "S231:value": "{padded}""#);
        assert_eq!(
            first_output(&through_root(&untyped)),
            3,
            "untyped {padded:?}"
        );
        let block = document(&PID, &[controller(&quoted(&padded))], &[]);
        assert_eq!(first_output(&block), 3, "block {padded:?}");
    }
}
