//! The data's own contract and the renderers as pure functions of it (diff note, "Output";
//! decisions 7 and 13): the JSON spelling, the closed objects, a Real as spelling plus bits, and
//! what both renderers write for data the authoring diff never produces: a diff from another
//! level (`source: cxf`, the two flags `null`), every note, and the one-sided templates.
//!
//! The diffs here are read from JSON typed by hand, because `Diff` is `#[non_exhaustive]` and is
//! built only by this crate.

mod support;

use oce_diff::{
    DIFF_SCHEMA, Diff, DiffOptions, Literal, MAX_RENAME_CANDIDATES, MAX_RENAME_PAIRS,
    MAX_RENAME_ROUNDS, Note, Source, render_markdown, render_text,
};
use serde_json::{Value, json};

use support::{compare, example, sequence};

fn read(value: Value) -> Diff {
    serde_json::from_value(value).expect("the JSON is a Diff")
}

/// An empty diff of `s` against `s`, as JSON to edit.
fn empty() -> Value {
    let model = sequence("s");
    support::json(&compare(&model, &model))
}

#[test]
fn the_constants_are_the_notes() {
    assert_eq!(DIFF_SCHEMA, "oce-diff/1");
    assert_eq!(MAX_RENAME_CANDIDATES, 2_048);
    assert_eq!(MAX_RENAME_PAIRS, 65_536);
    assert_eq!(MAX_RENAME_ROUNDS, 8);
    let options = DiffOptions::default();
    assert!(options.renames);
    assert!(!options.canonical_ids_equal);
    assert_eq!(DiffOptions::new(), options);
}

#[test]
fn the_json_has_sorted_keys_two_space_indentation_and_one_final_line_feed() {
    let (base, head) = example("pressure_request");
    let text = compare(&base, &head).to_json();
    assert!(text.starts_with("{\n  \"base\": {\n    \"derived_from\": null,\n"));
    assert!(text.ends_with("}\n") && !text.ends_with("\n\n"));
    fn sorted(value: &Value) -> bool {
        match value {
            Value::Object(map) => {
                let keys: Vec<&String> = map.keys().collect();
                keys.windows(2).all(|pair| pair[0] < pair[1]) && map.values().all(sorted)
            }
            Value::Array(values) => values.iter().all(sorted),
            _ => true,
        }
    }
    // Read the keys in document order, whatever map type the parser was built with.
    let mut keys_in_order = Vec::new();
    for line in text.lines() {
        if let Some((key, _)) = line
            .trim_start()
            .strip_prefix('"')
            .and_then(|rest| rest.split_once("\":"))
        {
            keys_in_order.push((line.len() - line.trim_start().len(), key.to_owned()));
        }
    }
    let top: Vec<&str> = keys_in_order
        .iter()
        .filter(|(indent, _)| *indent == 2)
        .map(|(_, key)| key.as_str())
        .collect();
    assert_eq!(
        top,
        [
            "base",
            "catalog",
            "graph",
            "head",
            "interface",
            "metadata",
            "notes",
            "parameters",
            "schema",
            "source",
            "summary"
        ]
    );
    assert!(sorted(&serde_json::from_str(&text).expect("JSON")));
    assert!(!text.contains('\t'));
}

#[test]
fn a_real_is_a_spelling_and_its_bits_and_never_a_json_number() {
    for (value, spelling, bits) in [
        (0.6, "0.6", "0x3fe3333333333333"),
        (-0.0, "-0.0", "0x8000000000000000"),
        (0.0, "0.0", "0x0000000000000000"),
        (60.0, "60.0", "0x404e000000000000"),
        (1e21, "1e21", "0x444b1ae4d6e2ef50"),
        (5e-324, "5e-324", "0x0000000000000001"),
        (f64::MAX, "1.7976931348623157e308", "0x7fefffffffffffff"),
        (f64::INFINITY, "inf", "0x7ff0000000000000"),
        (f64::NEG_INFINITY, "-inf", "0xfff0000000000000"),
        (f64::NAN, "NaN", "0x7ff8000000000000"),
    ] {
        let literal = Literal::real(value);
        assert_eq!(
            serde_json::to_value(&literal).expect("serializes"),
            json!({ "real": spelling, "bits": bits }),
            "{value:?}"
        );
        // A finite spelling reads back to the same bits: it is the shortest that does.
        if value.is_finite() {
            assert_eq!(
                spelling.parse::<f64>().expect("a number").to_bits(),
                value.to_bits()
            );
        }
        let Literal::Real { bits: hex, .. } = &literal else {
            panic!("a Real");
        };
        let parsed = u64::from_str_radix(hex.strip_prefix("0x").expect("0x"), 16).expect("hex");
        assert_eq!(parsed, value.to_bits());
    }
}

#[test]
fn every_literal_shape_reads_back_and_no_other_shape_is_read() {
    for shape in [
        json!({ "real": "0.5", "bits": "0x3fe0000000000000" }),
        json!({ "integer": -9_223_372_036_854_775_808_i64 }),
        json!({ "boolean": true }),
        json!({ "text": "a \"quoted\" line\nand another" }),
        json!({ "ref": "phiHigh" }),
        json!({ "member": "SimpleController.PI" }),
    ] {
        let literal: Literal = serde_json::from_value(shape.clone()).expect("a literal");
        assert_eq!(serde_json::to_value(&literal).expect("serializes"), shape);
    }
    for shape in [
        json!(0.5),
        json!("0.5"),
        json!({ "real": 0.5, "bits": "0x3fe0000000000000" }),
        json!({ "real": "0.5" }),
        json!({ "integer": 1.5 }),
        json!({ "integer": 1, "extra": 2 }),
        json!({ "ref": "a", "member": "b" }),
        json!({}),
    ] {
        assert!(
            serde_json::from_value::<Literal>(shape.clone()).is_err(),
            "{shape} is not a literal"
        );
    }
}

#[test]
fn every_object_is_closed_and_every_kind_is_known() {
    let good = empty();
    assert!(serde_json::from_value::<Diff>(good.clone()).is_ok());
    let edited = |edit: &dyn Fn(&mut Value)| {
        let mut value = good.clone();
        edit(&mut value);
        serde_json::from_value::<Diff>(value)
    };
    assert!(
        edited(&|v| v["extra"] = json!(1)).is_err(),
        "a key at the top"
    );
    assert!(
        edited(&|v| v["summary"]["extra"] = json!(1)).is_err(),
        "a key in the summary"
    );
    assert!(edited(&|v| v["summary"]["blocks"]["extra"] = json!(1)).is_err());
    assert!(
        edited(&|v| v["base"]["extra"] = json!(1)).is_err(),
        "a key in a side"
    );
    assert!(
        edited(&|v| v["notes"] = json!(["no-such-note"])).is_err(),
        "a note"
    );
    assert!(
        edited(&|v| v["source"] = json!("blob")).is_err(),
        "a source"
    );
    assert!(
        edited(&|v| {
            v.as_object_mut().expect("object").remove("graph");
        })
        .is_err()
    );
    assert!(
        edited(&|v| v["graph"] = json!([{ "kind": "moved", "subject": "a" }])).is_err(),
        "a kind the section does not have"
    );
    assert!(
        edited(
            &|v| v["graph"] = json!([{ "kind": "rewired", "subject": "a", "old": null,
                                          "new": null, "extra": 1 }])
        )
        .is_err(),
        "a key in an entry"
    );
    assert!(
        edited(
            &|v| v["graph"] = json!([{ "kind": "rewired", "subject": "a", "old": null,
                                          "new": null }])
        )
        .is_ok()
    );
    // A kind of one section is not a kind of another.
    assert!(
        edited(
            &|v| v["metadata"] = json!([{ "kind": "rewired", "subject": "a", "old": null,
                                             "new": null }])
        )
        .is_err()
    );
}

#[test]
fn an_empty_diff_renders_as_its_header_and_an_empty_table() {
    let diff = read(empty());
    assert_eq!(
        render_text(&diff),
        "# oce diff s -> s\n# graph equal; parameters equal; interface equal; metadata equal\n"
    );
    assert_eq!(
        render_markdown(&diff),
        "| Change | Subject | Was | Now |\n| --- | --- | --- | --- |\n"
    );
}

#[test]
fn a_diff_from_a_level_without_parameters_or_metadata_says_so() {
    // What the CXF adapter reports: the two flags are null, and a retune is a graph change.
    let mut value = empty();
    value["source"] = json!("cxf");
    value["summary"]["parameters_equal"] = Value::Null;
    value["summary"]["metadata_equal"] = Value::Null;
    value["summary"]["graph_equal"] = json!(false);
    value["summary"]["equal"] = json!(false);
    value["graph"] = json!([{ "kind": "reparameterized", "subject": "dehum.uHigh",
                              "old": { "real": "0.6", "bits": "0x3fe3333333333333" },
                              "new": { "real": "0.7", "bits": "0x3fe6666666666666" } }]);
    let diff = read(value);
    assert_eq!(diff.source, Source::Cxf);
    assert_eq!(diff.summary.parameters_equal, None);
    assert_eq!(
        render_text(&diff),
        "# oce diff s -> s\n\
         # graph differs; parameters not separable; interface equal; metadata not compared\n\
         # graph\n\
         ~ dehum.uHigh = 0.7   # was 0.6\n"
    );
}

#[test]
fn each_note_has_its_own_header_line() {
    let mut value = empty();
    value["notes"] = json!(["names-disjoint", "rename-pass-skipped", "rename-pass-off"]);
    value["summary"]["blocks"] =
        json!({ "by_name": 0, "renamed": 3, "base_only": 2, "head_only": 4 });
    let diff = read(value);
    assert_eq!(
        diff.notes,
        [
            Note::NamesDisjoint,
            Note::RenamePassSkipped,
            Note::RenamePassOff
        ]
    );
    assert_eq!(
        render_text(&diff),
        "# oce diff s -> s\n\
         # graph equal; parameters equal; interface equal; metadata equal\n\
         # names disjoint: no block kept its name; 3 of 5 matched by the rename pass, 2 not matched\n\
         # rename pass skipped: more than 2048 unmatched blocks on a side or more than 65536 candidate pairs\n\
         # rename pass off\n"
    );
}

#[test]
fn the_block_counts_are_written_only_when_a_block_was_renamed_removed_or_added() {
    let counts = |by_name: usize, renamed: usize, base_only: usize, head_only: usize| {
        let mut value = empty();
        value["summary"]["blocks"] = json!({ "by_name": by_name, "renamed": renamed,
                                             "base_only": base_only, "head_only": head_only });
        render_text(&read(value))
    };
    assert_eq!(counts(7, 0, 0, 0).lines().count(), 2);
    assert!(counts(7, 1, 0, 0).contains("# blocks: 7 by name, 1 renamed, 0 removed, 0 added\n"));
    assert!(counts(7, 0, 2, 0).contains("# blocks: 7 by name, 0 renamed, 2 removed, 0 added\n"));
    assert!(counts(0, 0, 0, 3).contains("# blocks: 0 by name, 0 renamed, 0 removed, 3 added\n"));
}

#[test]
fn the_one_sided_templates_spell_the_empty_side() {
    let mut value = empty();
    value["graph"] = json!([
      { "kind": "reparameterized", "subject": "a.k", "old": null, "new": { "integer": 1 } },
      { "kind": "reparameterized", "subject": "a.m", "old": { "text": "a \"b\"\nc" }, "new": null },
      { "kind": "rewired", "subject": "a.u", "old": null, "new": "b.y" },
      { "kind": "rewired", "subject": "a.v", "old": "b.y", "new": null },
      { "kind": "parameter-defaulted", "subject": "a.x", "value": { "boolean": false }, "side": "base" },
      { "kind": "parameter-defaulted", "subject": "a.y", "value": { "boolean": false }, "side": "head" }
    ]);
    value["interface"] = json!([
      { "kind": "unit-changed", "subject": "u", "direction": "in", "old": null, "new": "K" },
      { "kind": "max-changed", "subject": "y", "direction": "out",
        "old": { "integer": 10 }, "new": null }
    ]);
    value["metadata"] = json!([
      { "kind": "derived-from-changed", "subject": "derived_from", "old": "cxf:a", "new": null },
      { "kind": "comment-changed", "subject": "u", "old": null, "new": " a | b\n <c> *d*" }
    ]);
    let diff = read(value);
    assert_eq!(
        render_text(&diff),
        "# oce diff s -> s\n\
         # graph equal; parameters equal; interface equal; metadata equal\n\
         # interface\n\
         ~ in u @unit K   # was none\n\
         ~ out y @max none   # was 10\n\
         # graph\n\
         ~ a.k = 1   # was not assigned\n\
         ~ a.m not assigned   # was \"a \\\"b\\\"\\nc\"\n\
         ~ b.y -> a.u   # was not driven\n\
         ~ a.v not driven   # was b.y\n\
         ~ a.x not assigned   # was false; the catalog default\n\
         ~ a.y = false   # was not assigned; the catalog default\n\
         # metadata\n\
         ~ derived_from none   # was cxf:a\n\
         ~ # comment on u changed\n"
    );
    assert_eq!(
        render_markdown(&diff),
        "| Change | Subject | Was | Now |\n\
         | --- | --- | --- | --- |\n\
         | in unit-changed | `u` | | `K` |\n\
         | out max-changed | `y` | 10 | |\n\
         | reparameterized | `a.k` | | 1 |\n\
         | reparameterized | `a.m` | `\"a \\\"b\\\"\\nc\"` | |\n\
         | rewired | `a.u` | | `b.y` |\n\
         | rewired | `a.v` | `b.y` | |\n\
         | parameter-defaulted | `a.x` | false | |\n\
         | parameter-defaulted | `a.y` | | false |\n\
         | derived-from-changed | `derived_from` | `cxf:a` | |\n\
         | comment-changed | `u` | |  a \\| b<br> &lt;c&gt; \\*d\\* |\n"
    );
}

#[test]
fn the_renderers_write_what_the_data_holds_and_nothing_about_how_it_was_made() {
    // Two different pairs of models with the same diff data render alike: a renderer sees the
    // data only.
    let (base, head) = example("site_retune");
    let diff = compare(&base, &head);
    let copy = read(support::json(&diff));
    assert_eq!(render_text(&copy), render_text(&diff));
    assert_eq!(render_markdown(&copy), render_markdown(&diff));
}
