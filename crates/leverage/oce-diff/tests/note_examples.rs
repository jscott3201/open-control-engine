//! The oracle for the worked examples: the expected entries are the diff note's own JSON, text
//! and table (`docs/design/diff.md`, "Worked examples"), typed into this file by hand from the
//! note and never produced by the code. A change to the note's examples is copied here by hand,
//! and a difference between the two is resolved in the note or in the code, never by editing
//! this file to follow the output.
//!
//! The JSON is compared as parsed values, so the note's hand layout and key order do not
//! matter; `example_goldens.rs` pins the bytes. The note writes the catalog id as
//! `catalog:1:fnv1a128:<32 hex>`, which is checked here by shape and against the facade.
//!
//! Example (a) is in the note whole; (b) shows the summary, the notes and the graph section and
//! (c) the graph section with its note. Each example's other sections are checked against what
//! the note says of them in prose (only the comment differs).

mod support;

use oce_diff::{render_markdown, render_text};
use serde_json::{Value, json};

use support::{compare, example};

/// The diff of a worked example, as a JSON value.
fn diffed(name: &str) -> Value {
    let (base, head) = example(name);
    support::json(&compare(&base, &head))
}

fn rendered(name: &str) -> String {
    let (base, head) = example(name);
    render_text(&compare(&base, &head))
}

#[test]
fn example_a_is_the_notes_whole_json() {
    let mut actual = diffed("site_retune");
    let catalog = actual["catalog"].take();
    let catalog = catalog.as_str().expect("a string");
    let hex = catalog
        .strip_prefix("catalog:1:fnv1a128:")
        .expect("the catalog id's tag");
    assert_eq!(hex.len(), 32);
    assert!(
        hex.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert_eq!(catalog, oce_api::catalog_content_id(oce_api::catalog()));

    let expected = json!({
      "schema": "oce-diff/1",
      "source": "authoring",
      "catalog": null,
      "base": { "sequence": "ahu_dehum_override", "derived_from": null },
      "head": { "sequence": "ahu_dehum_override", "derived_from": null },
      "summary": { "equal": false, "graph_equal": true, "parameters_equal": false,
                   "interface_equal": true, "metadata_equal": false,
                   "blocks": { "by_name": 1, "renamed": 0, "base_only": 0, "head_only": 0 } },
      "notes": [],
      "parameters": [
        { "kind": "changed", "subject": "phiHigh", "type": "Real", "affects": ["dehum.uHigh"],
          "old": { "real": "0.6", "bits": "0x3fe3333333333333" },
          "new": { "real": "0.7", "bits": "0x3fe6666666666666" } },
        { "kind": "changed", "subject": "phiLow", "type": "Real", "affects": ["dehum.uLow"],
          "old": { "real": "0.55", "bits": "0x3fe199999999999a" },
          "new": { "real": "0.65", "bits": "0x3fe4cccccccccccd" } }
      ],
      "interface": [],
      "graph": [],
      "metadata": [ { "kind": "comment-changed", "subject": "sequence",
                      "old": " Dehumidification override, sheet revision A.",
                      "new": " Dehumidification override, sheet revision B." } ]
    });
    assert_eq!(actual, expected);
}

#[test]
fn example_a_is_the_notes_text() {
    let expected = "\
# oce diff ahu_dehum_override -> ahu_dehum_override
# graph equal; parameters differ; interface equal; metadata differs
# parameters
~ param phiHigh : Real = 0.7   # was 0.6; affects dehum.uHigh
~ param phiLow : Real = 0.65   # was 0.55; affects dehum.uLow
# metadata
~ # comment on sequence changed
";
    assert_eq!(rendered("site_retune"), expected);
}

#[test]
fn the_literal_variant_of_example_a_is_two_reparameterized_entries() {
    // "Authored as literals (`uLow = 0.55, uHigh = 0.6`), the same retune is two graph
    // `reparameterized` entries, `dehum.uHigh` and `dehum.uLow`, and `graph_equal` is false."
    let actual = diffed("literal_retune");
    assert_eq!(actual["summary"]["graph_equal"], json!(false));
    assert_eq!(actual["summary"]["parameters_equal"], json!(true));
    assert_eq!(actual["parameters"], json!([]));
    assert_eq!(
        actual["graph"],
        json!([
          { "kind": "reparameterized", "subject": "dehum.uHigh",
            "old": { "real": "0.6", "bits": "0x3fe3333333333333" },
            "new": { "real": "0.7", "bits": "0x3fe6666666666666" } },
          { "kind": "reparameterized", "subject": "dehum.uLow",
            "old": { "real": "0.55", "bits": "0x3fe199999999999a" },
            "new": { "real": "0.65", "bits": "0x3fe4cccccccccccd" } }
        ])
    );
}

#[test]
fn example_b_is_the_notes_summary_notes_and_graph() {
    let actual = diffed("pressure_request");
    let expected = json!({
      "summary": { "equal": false, "graph_equal": false, "parameters_equal": true,
                   "interface_equal": true, "metadata_equal": false,
                   "blocks": { "by_name": 4, "renamed": 1, "base_only": 1, "head_only": 1 } },
      "notes": [],
      "graph": [
        { "kind": "reparameterized", "subject": "damOpen.t",
          "old": { "real": "0.95", "bits": "0x3fee666666666666" },
          "new": { "real": "0.85", "bits": "0x3feb333333333333" } },
        { "kind": "removed", "subject": "openLong", "class": "CDL.Logical.TrueDelay",
          "params": [ { "name": "delayTime", "value": { "integer": 60 } },
                      { "name": "delayOnInit", "value": { "boolean": true } } ],
          "inputs": [ { "port": "u", "source": "damOpen.y" } ] },
        { "kind": "renamed", "subject": "setFrac", "from": "seventySet",
          "class": "CDL.Reals.MultiplyByParameter",
          "params": [ { "name": "k",
                        "value": { "real": "0.7", "bits": "0x3fe6666666666666" } } ],
          "confidence": "exact" },
        { "kind": "added", "subject": "starvedLong", "class": "CDL.Logical.TrueDelay",
          "params": [ { "name": "delayTime", "value": { "integer": 120 } },
                      { "name": "delayOnInit", "value": { "boolean": true } } ],
          "inputs": [ { "port": "u", "source": "starved.y" } ] },
        { "kind": "rewired", "subject": "want.u1", "old": "openLong.y", "new": "damOpen.y" },
        { "kind": "rewired", "subject": "want.u2", "old": "starved.y", "new": "starvedLong.y" }
      ]
    });
    for section in ["summary", "notes", "graph"] {
        assert_eq!(actual[section], expected[section], "{section}");
    }
    // "the other fields are as in (a)": nothing but the comment differs outside the graph.
    assert_eq!(actual["parameters"], json!([]));
    assert_eq!(actual["interface"], json!([]));
    assert_eq!(
        support::kinds(&actual["metadata"]),
        [("comment-changed".to_owned(), "sequence".to_owned())]
    );
}

#[test]
fn example_b_is_the_notes_text() {
    let expected = "\
# oce diff zone_pressure_request -> zone_pressure_request
# graph differs; parameters equal; interface equal; metadata differs
# blocks: 4 by name, 1 renamed, 1 removed, 1 added
# graph
~ damOpen.t = 0.85   # was 0.95
- block openLong = CDL.Logical.TrueDelay(delayTime = 60, delayOnInit = true)
- damOpen.y -> openLong.u
> block setFrac = CDL.Reals.MultiplyByParameter(k = 0.7)   # was seventySet; renamed, exact
+ block starvedLong = CDL.Logical.TrueDelay(delayTime = 120, delayOnInit = true)
+ starved.y -> starvedLong.u
~ damOpen.y -> want.u1   # was openLong.y
~ starvedLong.y -> want.u2   # was starved.y
# metadata
~ # comment on sequence changed
";
    assert_eq!(rendered("pressure_request"), expected);
}

#[test]
fn example_b_holds_the_notes_table_rows_in_order() {
    // The note abridges the table to these four rows under the header.
    let rows = [
        "| Change | Subject | Was | Now |",
        "| --- | --- | --- | --- |",
        "| reparameterized | `damOpen.t` | 0.95 | 0.85 |",
        "| removed | `openLong` | `CDL.Logical.TrueDelay(delayTime = 60, delayOnInit = true)` | |",
        "| renamed (exact) | `setFrac` | `seventySet` | `setFrac` |",
        "| rewired | `want.u1` | `openLong.y` | `damOpen.y` |",
    ];
    let (base, head) = example("pressure_request");
    let table = render_markdown(&compare(&base, &head));
    let lines: Vec<&str> = table.lines().collect();
    let mut from = 0;
    for row in rows {
        let at = lines
            .iter()
            .skip(from)
            .position(|line| *line == row)
            .unwrap_or_else(|| panic!("row `{row}` is in the table after line {from}\n{table}"));
        from += at + 1;
    }
    assert_eq!(&lines[..2], &rows[..2], "the header leads the table");
}

#[test]
fn example_c_is_the_notes_graph_and_its_note() {
    let actual = diffed("plant_reset");
    let expected = json!({
      "notes": ["names-disjoint"],
      "graph": [
        { "kind": "renamed", "subject": "TMax", "from": "THigh",
          "class": "CDL.Reals.Sources.Constant",
          "params": [ { "name": "k",
                        "value": { "real": "285.15", "bits": "0x4071d26666666666" } } ],
          "confidence": "exact" },
        { "kind": "renamed", "subject": "TMin", "from": "TLow",
          "class": "CDL.Reals.Sources.Constant",
          "params": [ { "name": "k",
                        "value": { "real": "279.15", "bits": "0x4071726666666666" } } ],
          "confidence": "exact" },
        { "kind": "removed", "subject": "ena", "class": "CDL.Logical.And", "params": [],
          "inputs": [ { "port": "u1", "source": "uPlaEna" }, { "port": "u2", "source": "uReq" } ] },
        { "kind": "removed", "subject": "hold", "class": "CDL.Logical.TrueDelay",
          "params": [ { "name": "delayTime", "value": { "integer": 300 } },
                      { "name": "delayOnInit", "value": { "boolean": true } } ],
          "inputs": [ { "port": "u", "source": "ena.y" } ] },
        { "kind": "added", "subject": "onDelay", "class": "CDL.Logical.TrueDelay",
          "params": [ { "name": "delayTime", "value": { "integer": 600 } },
                      { "name": "delayOnInit", "value": { "boolean": true } } ],
          "inputs": [ { "port": "u", "source": "plaOn.y" } ] },
        { "kind": "added", "subject": "plaOn", "class": "CDL.Logical.And", "params": [],
          "inputs": [ { "port": "u1", "source": "uReq" }, { "port": "u2", "source": "uPlaEna" } ] },
        { "kind": "renamed", "subject": "sw", "from": "sel",
          "class": "CDL.Reals.Switch", "params": [], "confidence": "high" },
        { "kind": "rewired", "subject": "sw.u2", "old": "hold.y", "new": "onDelay.y" }
      ]
    });
    assert_eq!(actual["notes"], expected["notes"]);
    assert_eq!(actual["graph"], expected["graph"]);
    // Three of five matched by the rename pass, two not, on each side.
    assert_eq!(
        actual["summary"]["blocks"],
        json!({ "by_name": 0, "renamed": 3, "base_only": 2, "head_only": 2 })
    );
    assert_eq!(actual["parameters"], json!([]));
    assert_eq!(actual["interface"], json!([]));
}

#[test]
fn example_c_is_the_notes_text() {
    let expected = "\
# oce diff chw_plant_reset -> chw_plant_reset
# graph differs; parameters equal; interface equal; metadata differs
# names disjoint: no block kept its name and class; 3 of 5 base blocks matched by the rename pass, 2 removed, 2 added
# graph
> block TMax = CDL.Reals.Sources.Constant(k = 285.15)   # was THigh; renamed, exact
> block TMin = CDL.Reals.Sources.Constant(k = 279.15)   # was TLow; renamed, exact
- block ena = CDL.Logical.And
- uPlaEna -> ena.u1
- uReq -> ena.u2
- block hold = CDL.Logical.TrueDelay(delayTime = 300, delayOnInit = true)
- ena.y -> hold.u
+ block onDelay = CDL.Logical.TrueDelay(delayTime = 600, delayOnInit = true)
+ plaOn.y -> onDelay.u
+ block plaOn = CDL.Logical.And
+ uReq -> plaOn.u1
+ uPlaEna -> plaOn.u2
> block sw = CDL.Reals.Switch   # was sel; renamed, high
~ onDelay.y -> sw.u2   # was hold.y
# metadata
~ # comment on sequence changed
";
    assert_eq!(rendered("plant_reset"), expected);
}

#[test]
fn the_bits_the_note_writes_are_the_bits_of_the_values_it_writes() {
    // The note's hex was typed by hand; hold it to the IEEE-754 bits of each spelling, so the
    // oracle above is not two copies of one slip.
    for (spelling, bits) in [
        ("0.6", 0x3fe3_3333_3333_3333_u64),
        ("0.7", 0x3fe6_6666_6666_6666),
        ("0.55", 0x3fe1_9999_9999_999a),
        ("0.65", 0x3fe4_cccc_cccc_cccd),
        ("0.95", 0x3fee_6666_6666_6666),
        ("0.85", 0x3feb_3333_3333_3333),
    ] {
        let value: f64 = spelling.parse().expect("a number");
        assert_eq!(value.to_bits(), bits, "{spelling}");
    }
}
