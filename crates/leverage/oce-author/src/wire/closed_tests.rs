//! The closed-shape adapter on its own: the `serde_json` behaviour it narrows (each test first
//! shows the plain derive accepting the shape), the paths it records, and the one construct it
//! cannot see into (GitHub #544).

use serde::Deserialize;

use super::*;

#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename = "Pair")]
struct Pair {
    name: String,
    on: bool,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Outer {
    pairs: Vec<Pair>,
}

#[derive(Debug, PartialEq, Deserialize)]
enum Mode {
    On,
    Off,
}

/// Read `text` through the adapter; a refusal as its message and the path the track kept.
fn closed<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T, (String, String)> {
    let track = Track::default();
    let mut deserializer = serde_json::Deserializer::from_str(text);
    T::deserialize(Closed::new(&mut deserializer, &track))
        .and_then(|value| deserializer.end().map(|()| value))
        .map_err(|error| (error.to_string(), track.failed_path()))
}

#[test]
fn a_derived_struct_reads_a_list_positionally_unless_closed() {
    let pair = Pair {
        name: "a".to_owned(),
        on: true,
    };
    // The defect: the derive reads a list as the fields in declaration order.
    let bare: Pair = serde_json::from_str(r#"["a", true]"#).expect("serde reads a list");
    assert_eq!(bare, pair);
    let (detail, path) = closed::<Pair>(r#"["a", true]"#).expect_err("refused");
    assert_eq!(
        detail,
        "invalid type: sequence, expected a Pair object at line 1 column 0"
    );
    assert_eq!(path, "document");
    assert_eq!(closed::<Pair>(r#"{"name": "a", "on": true}"#), Ok(pair));
}

#[test]
fn a_struct_refuses_every_other_json_type_naming_the_object() {
    for (text, found) in [
        ("null", "null"),
        ("true", "boolean `true`"),
        ("7", "integer `7`"),
        ("-1.5", "floating point `-1.5`"),
        (r#""a""#, r#"string "a""#),
        ("[]", "sequence"),
    ] {
        let (detail, path) = closed::<Pair>(text).expect_err(text);
        assert!(
            detail.starts_with(&format!("invalid type: {found}, expected a Pair object")),
            "{text}: {detail}"
        );
        assert_eq!(path, "document", "{text}");
    }
}

#[test]
fn the_article_follows_the_object_name() {
    #[derive(Debug, Deserialize)]
    #[serde(rename = "Assignment")]
    struct Named {
        _name: String,
    }
    let (detail, _) = closed::<Named>("[]").expect_err("refused");
    assert!(
        detail.starts_with("invalid type: sequence, expected an Assignment object"),
        "{detail}"
    );
}

#[test]
fn members_are_closed_at_every_depth_and_named_by_path() {
    let (detail, path) = closed::<Outer>(r#"{"pairs": [{"name": "a", "on": true}, ["b", false]]}"#)
        .expect_err("the second pair is a list");
    assert!(detail.contains("expected a Pair object"), "{detail}");
    assert_eq!(path, "pairs[1]");
    let (_, path) = closed::<Outer>(r#"{"pairs": [{"name": "a", "on": "yes"}]}"#)
        .expect_err("a string where a boolean belongs");
    assert_eq!(path, "pairs[0].on");
    assert_eq!(
        closed::<Outer>(r#"{"pairs": [{"on": false, "name": "b"}]}"#),
        Ok(Outer {
            pairs: vec![Pair {
                name: "b".to_owned(),
                on: false
            }]
        }),
        "an object's keys are read in any order"
    );
}

#[test]
fn a_derived_enum_reads_a_string_only_when_closed() {
    // The defect's sibling: the derive also reads the map form of a unit variant.
    let bare: Mode = serde_json::from_str(r#"{"On": null}"#).expect("serde reads the map form");
    assert_eq!(bare, Mode::On);
    let (detail, _) = closed::<Mode>(r#"{"On": null}"#).expect_err("refused");
    assert!(
        detail.starts_with(r#"invalid type: map, expected one of the strings "On", "Off""#),
        "{detail}"
    );
    assert_eq!(closed::<Mode>(r#""Off""#), Ok(Mode::Off));
    let (detail, _) = closed::<Mode>(r#""on""#).expect_err("names are exact");
    assert!(detail.starts_with("unknown variant `on`"), "{detail}");
    for text in ["null", "1", "[\"On\"]", "true"] {
        let (detail, _) = closed::<Mode>(text).expect_err(text);
        assert!(detail.starts_with("invalid type:"), "{text}: {detail}");
    }
}

#[test]
fn an_option_never_reads_null_as_absent_when_closed() {
    #[derive(Debug, PartialEq, Deserialize)]
    struct Holder {
        note: Option<String>,
    }
    let bare: Holder = serde_json::from_str(r#"{"note": null}"#).expect("serde reads null");
    assert_eq!(bare, Holder { note: None });
    let (detail, path) = closed::<Holder>(r#"{"note": null}"#).expect_err("refused");
    assert!(
        detail.starts_with("invalid type: null, expected a string"),
        "{detail}"
    );
    assert_eq!(path, "note");
    assert_eq!(
        closed::<Holder>(r#"{"note": "n"}"#),
        Ok(Holder {
            note: Some("n".to_owned())
        })
    );
}

#[test]
fn an_unknown_key_is_still_refused_by_name_after_it_is_read_as_text() {
    let (detail, _) = closed::<Pair>(r#"{"name": "a", "off": true}"#).expect_err("refused");
    assert!(
        detail.starts_with("unknown field `off`, expected `name` or `on`"),
        "{detail}"
    );
    let (detail, _) = closed::<Pair>(r#"{"name": "a"}"#).expect_err("refused");
    assert!(detail.starts_with("missing field `on`"), "{detail}");
}

#[test]
fn buffered_content_reads_past_the_adapter() {
    // Why the wire DTOs may carry no `untagged`, `tag` or `content` attribute (the integration
    // test `the_wire_dtos_carry_no_serde_attribute_that_bypasses_the_closed_shapes`): serde reads
    // such an enum's variant from a buffered copy of the value, which the adapter never sees, so
    // a list is read positionally again.
    #[derive(Debug, PartialEq, Deserialize)]
    #[serde(untagged)]
    enum Either {
        Pair(Pair),
    }
    assert_eq!(
        closed::<Either>(r#"["a", true]"#),
        Ok(Either::Pair(Pair {
            name: "a".to_owned(),
            on: true
        }))
    );
}

#[test]
fn a_path_spells_keys_with_dots_and_indexes_in_brackets() {
    assert_eq!(spell(&[]), "document");
    let steps = [
        Step::Key("inputs".into()),
        Step::Index(0),
        Step::Key("semantic".into()),
    ];
    assert_eq!(spell(&steps), "inputs[0].semantic");
    let steps = [
        Step::Key("blocks".into()),
        Step::Index(12),
        Step::Key("params".into()),
        Step::Index(3),
        Step::Key("value".into()),
        Step::Key("ref".into()),
    ];
    assert_eq!(spell(&steps), "blocks[12].params[3].value.ref");
    assert_eq!(spell(&[Step::Index(4)]), "[4]");
}

#[test]
fn the_first_failure_keeps_the_innermost_path() {
    let track = Track::default();
    assert_eq!(track.failed_path(), "document");
    track.enter(Step::Key("inputs".into()));
    track.enter(Step::Index(1));
    let inner: Result<(), ()> = track.leave(Err(()));
    let outer = track.leave(inner);
    assert!(outer.is_err());
    assert_eq!(track.failed_path(), "inputs[1]");
    assert!(track.steps.borrow().is_empty(), "every step left");
}

#[test]
fn a_success_records_no_path() {
    let track = Track::default();
    track.enter(Step::Key("outputs".into()));
    assert_eq!(track.leave(Ok::<(), ()>(())), Ok(()));
    assert_eq!(track.failed_path(), "document");
}
