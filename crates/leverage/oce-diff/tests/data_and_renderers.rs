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

/// `value` with its summary flags set to what its sections give, so a hand-built diff is one a
/// reader accepts: `equal` when every section is empty, a section flag when its section holds
/// no entry other than `reordered`. A flag that is `null` stays `null`.
fn follow(mut value: Value) -> Value {
    let section = |value: &Value, name: &str| -> (bool, bool) {
        let entries = value[name].as_array().expect("a section");
        (
            entries.is_empty(),
            entries.iter().all(|entry| entry["kind"] == "reordered"),
        )
    };
    let mut empty = true;
    for (name, flag) in [
        ("parameters", "parameters_equal"),
        ("interface", "interface_equal"),
        ("graph", "graph_equal"),
        ("metadata", "metadata_equal"),
    ] {
        let (is_empty, same_but_for_order) = section(&value, name);
        empty &= is_empty;
        if !value["summary"][flag].is_null() {
            value["summary"][flag] = json!(same_but_for_order);
        }
    }
    value["summary"]["equal"] = json!(empty);
    value
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
    // Each edit is followed by the flags, so the one thing wrong is the thing named.
    let edited = |edit: &dyn Fn(&mut Value)| {
        let mut value = good.clone();
        edit(&mut value);
        serde_json::from_value::<Diff>(follow(value))
    };
    let rewired = json!([{ "kind": "rewired", "subject": "a", "old": null, "new": null }]);
    assert!(edited(&|v| v["graph"] = rewired.clone()).is_ok());
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
        edited(&|v| v["graph"] = json!([{ "kind": "moved", "subject": "a" }])).is_err(),
        "a kind the section does not have"
    );
    assert!(
        edited(&|v| {
            v["graph"] = rewired.clone();
            v["graph"][0]["extra"] = json!(1);
        })
        .is_err(),
        "a key in an entry"
    );
    // A kind of one section is not a kind of another.
    assert!(edited(&|v| v["metadata"] = rewired.clone()).is_err());
}

#[test]
fn every_key_is_required_and_a_value_that_may_be_absent_is_written_null() {
    // Nothing is read as "absent means none": a missing key is refused wherever it stands.
    let (base, head) = example("every_kind");
    let good = support::json(&compare(&base, &head));
    assert!(serde_json::from_value::<Diff>(good.clone()).is_ok());
    let without = |path: &[&str]| {
        let mut value = good.clone();
        let (last, parents) = path.split_last().expect("a path");
        let mut at = &mut value;
        for key in parents {
            at = match key.parse::<usize>() {
                Ok(index) => &mut at[index],
                Err(_) => &mut at[*key],
            };
        }
        let removed = at.as_object_mut().expect("an object").remove(*last);
        assert!(removed.is_some(), "{path:?} is in the golden");
        serde_json::from_value::<Diff>(value)
    };
    for key in [
        "schema",
        "source",
        "catalog",
        "base",
        "head",
        "summary",
        "notes",
        "parameters",
        "interface",
        "graph",
        "metadata",
    ] {
        assert!(without(&[key]).is_err(), "top-level `{key}`");
    }
    for key in [
        "equal",
        "graph_equal",
        "parameters_equal",
        "interface_equal",
        "metadata_equal",
        "blocks",
    ] {
        assert!(without(&["summary", key]).is_err(), "summary `{key}`");
    }
    for key in ["by_name", "renamed", "base_only", "head_only"] {
        assert!(
            without(&["summary", "blocks", key]).is_err(),
            "blocks `{key}`"
        );
    }
    assert!(
        without(&["base", "derived_from"]).is_err(),
        "a null `derived_from` is written"
    );
    assert!(without(&["head", "sequence"]).is_err());
    // Every key of every entry of every section.
    let mut keys = 0;
    for section in ["parameters", "interface", "graph", "metadata"] {
        let entries = good[section].as_array().expect("a section");
        for (index, entry) in entries.iter().enumerate() {
            for key in entry.as_object().expect("an entry").keys() {
                let index = index.to_string();
                assert!(
                    without(&[section, &index, key]).is_err(),
                    "{section}[{index}] without `{key}`: {entry}"
                );
                keys += 1;
            }
        }
    }
    assert!(keys > 100, "{keys} keys were each taken out once");
    // The nullable keys in particular: an entry with one side empty writes that side `null`.
    let diff: Diff = serde_json::from_value(follow({
        let mut value = empty();
        value["graph"] = json!([
          { "kind": "reparameterized", "subject": "a.k", "old": null, "new": { "integer": 1 } },
          { "kind": "rewired", "subject": "a.u", "old": "b.y", "new": null }
        ]);
        value
    }))
    .expect("null is a value");
    assert_eq!(diff.graph.len(), 2);
    for (entry, key) in [(0, "old"), (1, "new")] {
        let mut value = follow(support::json(&diff));
        value["graph"][entry]
            .as_object_mut()
            .expect("an entry")
            .remove(key);
        assert!(
            serde_json::from_value::<Diff>(value).is_err(),
            "graph[{entry}] without `{key}`"
        );
    }
}

#[test]
fn only_this_schema_is_read() {
    let with = |schema: Value| {
        let mut value = empty();
        value["schema"] = schema;
        serde_json::from_value::<Diff>(value)
    };
    assert!(with(json!("oce-diff/1")).is_ok());
    for schema in [
        json!("oce-diff/2"),
        json!("oce-diff/1 "),
        json!("OCE-DIFF/1"),
        json!("oce-report/1"),
        json!(""),
        json!(1),
        Value::Null,
    ] {
        let error = with(schema.clone()).expect_err("another schema");
        if schema.is_string() {
            assert!(
                error.to_string().contains("is not `oce-diff/1`"),
                "{schema}: {error}"
            );
        }
    }
}

#[test]
fn a_real_whose_bits_are_malformed_or_are_not_its_spelling_is_refused() {
    let read = |real: &str, bits: &str| {
        serde_json::from_value::<Literal>(json!({ "real": real, "bits": bits }))
    };
    assert!(read("0.6", "0x3fe3333333333333").is_ok());
    assert!(read("-0.0", "0x8000000000000000").is_ok());
    assert!(
        read("NaN", "0x7ff8000000000001").is_ok(),
        "a NaN payload is its bits"
    );
    assert!(read("inf", "0x7ff0000000000000").is_ok());
    for bits in [
        "zzz",
        "",
        "0x",
        "3fe3333333333333",
        "0x3fe333333333333",
        "0x3fe33333333333333",
        "0x3FE3333333333333",
        "0X3fe3333333333333",
        "0x3fe333333333333g",
        "0x+fe3333333333333",
        " 0x3fe3333333333333",
    ] {
        let error = read("0.6", bits).expect_err("malformed bits");
        assert!(
            error.to_string().contains("16 lowercase hex digits"),
            "`{bits}`: {error}"
        );
    }
    // Well-formed bits of another value, and another spelling of the same value.
    for (real, bits) in [
        ("0.6", "0x3fe6666666666666"),
        ("0.0", "0x8000000000000000"),
        ("0.60", "0x3fe3333333333333"),
        ("6e-1", "0x3fe3333333333333"),
        ("nan", "0x7ff8000000000000"),
        ("", "0x3fe3333333333333"),
    ] {
        let error = read(real, bits).expect_err("the two halves disagree");
        assert!(
            error
                .to_string()
                .contains("is not the spelling of its bits"),
            "`{real}` with {bits}: {error}"
        );
    }
    // Inside a diff the same refusal stops the whole read.
    let mut value = follow({
        let mut value = empty();
        value["graph"] = json!([{ "kind": "reparameterized", "subject": "a.k", "old": null,
                                  "new": { "real": "0.6", "bits": "zzz" } }]);
        value
    });
    assert!(serde_json::from_value::<Diff>(value.clone()).is_err());
    value["graph"][0]["new"]["bits"] = json!("0x3fe3333333333333");
    assert!(serde_json::from_value::<Diff>(value).is_ok());
}

#[test]
fn a_summary_its_sections_contradict_is_refused() {
    let rewired = json!([{ "kind": "rewired", "subject": "a.u", "old": null, "new": "b.y" }]);
    let reordered = json!([{ "kind": "reordered", "subject": "blocks", "list": "blocks" }]);
    let changed = |edit: &dyn Fn(&mut Value)| {
        let mut value = empty();
        edit(&mut value);
        serde_json::from_value::<Diff>(value).map_err(|error| error.to_string())
    };
    // `equal: true` beside an entry, and `equal: false` beside none.
    let error = changed(&|v| v["graph"] = rewired.clone()).expect_err("equal beside an entry");
    assert!(error.contains("summary.equal is true"), "{error}");
    let error = changed(&|v| v["summary"]["equal"] = json!(false)).expect_err("nothing differs");
    assert!(error.contains("summary.equal is false"), "{error}");
    // A section flag that says equal beside a change, or differs beside none.
    let error = changed(&|v| {
        v["graph"] = rewired.clone();
        v["summary"]["equal"] = json!(false);
    })
    .expect_err("graph_equal beside a rewiring");
    assert!(error.contains("summary.graph_equal is true"), "{error}");
    for flag in [
        "graph_equal",
        "parameters_equal",
        "interface_equal",
        "metadata_equal",
    ] {
        let error = changed(&|v| v["summary"][flag] = json!(false)).expect_err(flag);
        assert!(
            error.contains(&format!("summary.{flag} is false")),
            "{error}"
        );
    }
    // A reordering leaves its section's flag true and `equal` false: both are required.
    assert!(
        changed(&|v| {
            v["graph"] = reordered.clone();
            v["summary"]["equal"] = json!(false);
        })
        .is_ok()
    );
    let error = changed(&|v| {
        v["graph"] = reordered.clone();
        v["summary"]["equal"] = json!(false);
        v["summary"]["graph_equal"] = json!(false);
    })
    .expect_err("a reordering is not a graph difference");
    assert!(error.contains("summary.graph_equal is false"), "{error}");
    let error = changed(&|v| v["graph"] = reordered.clone()).expect_err("a reordering is an entry");
    assert!(error.contains("summary.equal is true"), "{error}");
    // `null` is "this level cannot tell", and only beside an empty section.
    assert!(changed(&|v| v["summary"]["parameters_equal"] = Value::Null).is_ok());
    let error = changed(&|v| {
        v["metadata"] = json!([{ "kind": "comment-changed", "subject": "u",
                                 "old": null, "new": " c" }]);
        v["summary"]["equal"] = json!(false);
        v["summary"]["metadata_equal"] = Value::Null;
    })
    .expect_err("null beside an entry");
    assert!(error.contains("summary.metadata_equal is null"), "{error}");
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
    value["graph"] = json!([{ "kind": "reparameterized", "subject": "dehum.uHigh",
                              "old": { "real": "0.6", "bits": "0x3fe3333333333333" },
                              "new": { "real": "0.7", "bits": "0x3fe6666666666666" } }]);
    let diff = read(follow(value));
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
         # names disjoint: no block kept its name and class; 3 of 5 base blocks matched by the rename pass, 2 removed, 4 added\n\
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
fn counts_read_from_json_cannot_overflow_a_renderer() {
    // A `Diff` can be read from any JSON; the renderers add its counts without panicking.
    let mut value = empty();
    value["notes"] = json!(["names-disjoint"]);
    value["summary"]["blocks"] = json!({ "by_name": usize::MAX, "renamed": usize::MAX,
                                         "base_only": usize::MAX, "head_only": usize::MAX });
    let text = render_text(&read(value.clone()));
    assert!(text.contains(&format!(
        "# names disjoint: no block kept its name and class; {max} of {max} base blocks matched by the rename pass, {max} removed, {max} added\n",
        max = usize::MAX
    )));
    value["notes"] = json!([]);
    assert!(render_text(&read(value)).contains(&format!(
        "# blocks: {max} by name, {max} renamed, {max} removed, {max} added\n",
        max = usize::MAX
    )));
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
    let diff = read(follow(value));
    assert_eq!(
        render_text(&diff),
        "# oce diff s -> s\n\
         # graph differs; parameters equal; interface differs; metadata differs\n\
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
