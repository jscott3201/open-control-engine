//! One minimal pair per kind of the parameters, interface and metadata sections (diff note,
//! "Categories and sections" and "Site parameters and the graph"), each asserting the kind, the
//! subject and the values: a sequence-parameter change with what it affects, an Integer literal
//! against the equal Real, every boundary attribute, every semantic field and a comment on every
//! kind of statement.

mod support;

use oce_author::Sequence;
use serde_json::{Value, json};

use support::{compare, parse};

/// A sequence named `s` from the statements after the header.
fn model(body: &str) -> Sequence {
    parse(&format!("sequence s\nderived_from none\n{body}"))
}

/// The diff of two statement lists, as a JSON value.
fn diffed(base: &str, head: &str) -> Value {
    support::json(&compare(&model(base), &model(head)))
}

const GAIN: &str = "block g = CDL.Reals.MultiplyByParameter(k = gain)\n\
                    block h = CDL.Reals.MultiplyByParameter(k = gain)\n";

#[test]
fn a_changed_sequence_parameter_lists_what_it_affects_and_leaves_the_graph_equal() {
    let diff = diffed(
        &format!("param gain : Real = 2.0\n{GAIN}"),
        &format!("param gain : Real = 3.0\n{GAIN}"),
    );
    assert_eq!(
        diff["parameters"],
        json!([{ "kind": "changed", "subject": "gain", "type": "Real",
                 "old": { "real": "2.0", "bits": "0x4000000000000000" },
                 "new": { "real": "3.0", "bits": "0x4008000000000000" },
                 "affects": ["g.k", "h.k"] }])
    );
    // A site retune: same structure, different tuning. The referencing assignments are not
    // repeated as graph entries.
    assert_eq!(diff["graph"], json!([]));
    assert_eq!(diff["summary"]["graph_equal"], json!(true));
    assert_eq!(diff["summary"]["parameters_equal"], json!(false));
    assert_eq!(diff["summary"]["equal"], json!(false));
}

#[test]
fn an_added_parameter_lists_heads_references_and_a_removed_one_bases() {
    let with = format!("param gain : Real = 2.0\n{GAIN}");
    let without = "block g = CDL.Reals.MultiplyByParameter(k = 2.0)\n\
                   block h = CDL.Reals.MultiplyByParameter(k = 2.0)\n";
    let added = diffed(without, &with);
    assert_eq!(
        added["parameters"],
        json!([{ "kind": "added", "subject": "gain", "type": "Real",
                 "value": { "real": "2.0", "bits": "0x4000000000000000" },
                 "affects": ["g.k", "h.k"] }])
    );
    let removed = diffed(&with, without);
    assert_eq!(
        removed["parameters"],
        json!([{ "kind": "removed", "subject": "gain", "type": "Real",
                 "value": { "real": "2.0", "bits": "0x4000000000000000" },
                 "affects": ["g.k", "h.k"] }])
    );
    // The assignments themselves changed from a literal to a reference: that is a graph change.
    assert_eq!(
        support::kinds(&added["graph"]),
        [
            ("reparameterized".to_owned(), "g.k".to_owned()),
            ("reparameterized".to_owned(), "h.k".to_owned())
        ]
    );
}

#[test]
fn a_parameter_nothing_references_affects_nothing() {
    let diff = diffed("param spare : Integer = 1\n", "param spare : Integer = 2\n");
    assert_eq!(
        diff["parameters"],
        json!([{ "kind": "changed", "subject": "spare", "type": "Integer",
                 "old": { "integer": 1 }, "new": { "integer": 2 }, "affects": [] }])
    );
}

#[test]
fn a_changed_type_is_type_changed_with_both_types_and_values() {
    let diff = diffed("param n : Integer = 3\n", "param n : Real = 3.5\n");
    assert_eq!(
        diff["parameters"],
        json!([{ "kind": "type-changed", "subject": "n",
                 "old_type": "Integer", "new_type": "Real",
                 "old": { "integer": 3 },
                 "new": { "real": "3.5", "bits": "0x400c000000000000" },
                 "affects": [] }])
    );
    // An enumeration type is a type like any other.
    let diff = diffed(
        "param mode : SimpleController = SimpleController.PI\n",
        "param mode : Integer = 1\n",
    );
    assert_eq!(
        support::kinds(&diff["parameters"]),
        [("type-changed".to_owned(), "mode".to_owned())]
    );
    assert_eq!(diff["parameters"][0]["old_type"], json!("SimpleController"));
    assert_eq!(
        diff["parameters"][0]["old"],
        json!({ "member": "SimpleController.PI" })
    );
}

#[test]
fn parameters_in_another_order_are_reordered_once() {
    let diff = diffed(
        "param a : Real = 1.0\nparam b : Real = 2.0\nparam c : Real = 3.0\n",
        "param c : Real = 3.0\nparam a : Real = 1.0\nparam b : Real = 2.0\n",
    );
    assert_eq!(
        diff["parameters"],
        json!([{ "kind": "reordered", "subject": "params" }])
    );
    assert_eq!(diff["summary"]["parameters_equal"], json!(false));
}

#[test]
fn an_integer_literal_against_the_equal_real_is_respelled_only_where_a_real_is_expected() {
    // On a Real sequence parameter: one value, two spellings. Metadata, not a parameter change.
    let diff = diffed("param t : Real = 60\n", "param t : Real = 60.0\n");
    assert_eq!(diff["parameters"], json!([]));
    assert_eq!(
        diff["metadata"],
        json!([{ "kind": "literal-respelled", "subject": "t", "old": "60", "new": "60.0" }])
    );
    // On a Real block parameter (`delayTime` of TrueDelay), the same.
    let diff = diffed(
        "block d = CDL.Logical.TrueDelay(delayTime = 60)\n",
        "block d = CDL.Logical.TrueDelay(delayTime = 60.0)\n",
    );
    assert_eq!(diff["graph"], json!([]));
    assert_eq!(diff["summary"]["graph_equal"], json!(true));
    assert_eq!(
        diff["metadata"],
        json!([{ "kind": "literal-respelled", "subject": "d.delayTime",
                 "old": "60", "new": "60.0" }])
    );
    // On an Integer block parameter (`k` of Integers.Sources.Constant) the Real is another value.
    let diff = diffed(
        "block c = CDL.Integers.Sources.Constant(k = 1)\n",
        "block c = CDL.Integers.Sources.Constant(k = 1.0)\n",
    );
    assert_eq!(
        support::kinds(&diff["graph"]),
        [("reparameterized".to_owned(), "c.k".to_owned())]
    );
    assert_eq!(diff["metadata"], json!([]));
    // And on a parameter the catalog does not know, the diff does not guess.
    let diff = diffed(
        "block c = CDL.Nowhere.Nothing(k = 1)\n",
        "block c = CDL.Nowhere.Nothing(k = 1.0)\n",
    );
    assert_eq!(
        support::kinds(&diff["graph"]),
        [("reparameterized".to_owned(), "c.k".to_owned())]
    );
}

#[test]
fn an_integer_and_a_real_that_differ_in_value_or_sign_of_zero_are_not_respelled() {
    let diff = diffed("param t : Real = 60\n", "param t : Real = 60.5\n");
    assert_eq!(
        support::kinds(&diff["parameters"]),
        [("changed".to_owned(), "t".to_owned())]
    );
    // The Integer 0 means +0.0 where a Real is expected, never -0.0.
    let diff = diffed("param z : Real = 0\n", "param z : Real = -0.0\n");
    assert_eq!(
        support::kinds(&diff["parameters"]),
        [("changed".to_owned(), "z".to_owned())]
    );
    let diff = diffed("param z : Real = 0\n", "param z : Real = 0.0\n");
    assert_eq!(diff["parameters"], json!([]));
    // An array element of a Real array is a Real too.
    let diff = diffed(
        "block m = CDL.Reals.MultiSum(nin = 2, k = [1, 2])\n",
        "block m = CDL.Reals.MultiSum(nin = 2, k = [1.0, 2.0])\n",
    );
    assert_eq!(diff["graph"], json!([]));
    assert_eq!(
        support::kinds(&diff["metadata"]),
        [
            ("literal-respelled".to_owned(), "m.k_1".to_owned()),
            ("literal-respelled".to_owned(), "m.k_2".to_owned())
        ]
    );
}

#[test]
fn a_changed_unit_suffix_is_respelled_when_the_value_is_the_same() {
    // 1 h is 3600 s: one value, written two ways.
    let diff = diffed(
        "block d = CDL.Logical.TrueDelay(delayTime = 3600.0)\n",
        "block d = CDL.Logical.TrueDelay(delayTime = 1 h)\n",
    );
    assert_eq!(diff["graph"], json!([]));
    assert_eq!(
        diff["metadata"],
        json!([{ "kind": "literal-respelled", "subject": "d.delayTime",
                 "old": "3600.0", "new": "1 h" }])
    );
    // 2 h is another value: a graph change, with no respelling beside it.
    let diff = diffed(
        "block d = CDL.Logical.TrueDelay(delayTime = 1 h)\n",
        "block d = CDL.Logical.TrueDelay(delayTime = 2 h)\n",
    );
    assert_eq!(
        diff["graph"],
        json!([{ "kind": "reparameterized", "subject": "d.delayTime",
                 "old": { "real": "3600.0", "bits": "0x40ac200000000000" },
                 "new": { "real": "7200.0", "bits": "0x40bc200000000000" } }])
    );
    assert_eq!(diff["metadata"], json!([]));
}

#[test]
fn each_boundary_attribute_has_its_own_interface_kind() {
    let diff = diffed(
        "in u : Real @unit K @quantity ThermodynamicTemperature @displayUnit degC @min 0.5 @max 9.5\n",
        "in u : Integer @unit degC @quantity Temperature @displayUnit K @min 1.5 @max 8.5\n",
    );
    let real = |value: f64| json!({ "real": format!("{value:?}"), "bits": format!("0x{:016x}", value.to_bits()) });
    assert_eq!(
        diff["interface"],
        json!([
          { "kind": "type-changed", "subject": "u", "direction": "in",
            "old": "Real", "new": "Integer" },
          { "kind": "unit-changed", "subject": "u", "direction": "in", "old": "K", "new": "degC" },
          { "kind": "quantity-changed", "subject": "u", "direction": "in",
            "old": "ThermodynamicTemperature", "new": "Temperature" },
          { "kind": "display-unit-changed", "subject": "u", "direction": "in",
            "old": "degC", "new": "K" },
          { "kind": "min-changed", "subject": "u", "direction": "in",
            "old": real(0.5), "new": real(1.5) },
          { "kind": "max-changed", "subject": "u", "direction": "in",
            "old": real(9.5), "new": real(8.5) }
        ])
    );
    assert_eq!(diff["summary"]["interface_equal"], json!(false));
    assert_eq!(diff["summary"]["graph_equal"], json!(true));
}

#[test]
fn an_attribute_authored_on_one_side_only_is_null_on_the_other() {
    let diff = diffed("out y : Real @unit K @max 10\n", "out y : Real\n");
    assert_eq!(
        diff["interface"],
        json!([
          { "kind": "unit-changed", "subject": "y", "direction": "out", "old": "K", "new": null },
          { "kind": "max-changed", "subject": "y", "direction": "out",
            "old": { "integer": 10 }, "new": null }
        ])
    );
}

#[test]
fn an_added_or_removed_boundary_carries_its_declaration() {
    let with =
        "in u : Real @unit K @quantity ThermodynamicTemperature @displayUnit degC @min 0 @max 10\n";
    let entry = |kind: &str| {
        json!([{ "kind": kind, "subject": "u", "direction": "in", "type": "Real",
                 "unit": "K", "quantity": "ThermodynamicTemperature", "display_unit": "degC",
                 "min": { "integer": 0 }, "max": { "integer": 10 } }])
    };
    assert_eq!(diffed("", with)["interface"], entry("added"));
    assert_eq!(diffed(with, "")["interface"], entry("removed"));
}

#[test]
fn a_boundary_that_changes_direction_or_name_is_removed_plus_added() {
    // The rename pass never renames a boundary.
    let diff = diffed("in x : Real\n", "out x : Real\n");
    assert_eq!(
        support::kinds(&diff["interface"]),
        [
            ("added".to_owned(), "x".to_owned()),
            ("removed".to_owned(), "x".to_owned())
        ]
    );
    assert_eq!(diff["interface"][0]["direction"], json!("out"));
    assert_eq!(diff["interface"][1]["direction"], json!("in"));

    let base = "in TOut : Real\nblock a = CDL.Reals.Abs\nTOut -> a.u\n";
    let head = "in TOutdoor : Real\nblock a = CDL.Reals.Abs\nTOutdoor -> a.u\n";
    let diff = diffed(base, head);
    assert_eq!(
        support::kinds(&diff["interface"]),
        [
            ("removed".to_owned(), "TOut".to_owned()),
            ("added".to_owned(), "TOutdoor".to_owned())
        ]
    );
    assert_eq!(
        diff["graph"],
        json!([{ "kind": "rewired", "subject": "a.u", "old": "TOut", "new": "TOutdoor" }])
    );
}

#[test]
fn inputs_and_outputs_are_each_reordered_on_their_own() {
    let diff = diffed(
        "in a : Real\nin b : Real\nout y : Real\nout z : Real\n",
        "in b : Real\nin a : Real\nout y : Real\nout z : Real\n",
    );
    assert_eq!(
        diff["interface"],
        json!([{ "kind": "reordered", "subject": "inputs" }])
    );
    let diff = diffed(
        "in a : Real\nin b : Real\nout y : Real\nout z : Real\n",
        "in a : Real\nin b : Real\nout z : Real\nout y : Real\n",
    );
    assert_eq!(
        diff["interface"],
        json!([{ "kind": "reordered", "subject": "outputs" }])
    );
}

#[test]
fn a_bound_written_as_an_integer_and_as_the_equal_real_is_respelled_on_a_real_connector() {
    let diff = diffed(
        "in u : Real @min 0 @max 10\n",
        "in u : Real @min 0.0 @max 10.0\n",
    );
    assert_eq!(diff["interface"], json!([]));
    assert_eq!(
        diff["metadata"],
        json!([
          { "kind": "literal-respelled", "subject": "u.max", "old": "10", "new": "10.0" },
          { "kind": "literal-respelled", "subject": "u.min", "old": "0", "new": "0.0" }
        ])
    );
    // On an Integer connector a Real bound is not the Integer respelled.
    let diff = diffed("in n : Integer @min 0\n", "in n : Integer @min 0.0\n");
    assert_eq!(
        support::kinds(&diff["interface"]),
        [("min-changed".to_owned(), "n".to_owned())]
    );
}

#[test]
fn the_sequence_name_and_its_provenance_are_metadata() {
    let base = parse("sequence first\nderived_from none\n");
    let head = parse("sequence second\nderived_from cxf:fnv1a128:0123abcd\n");
    let diff = support::json(&compare(&base, &head));
    assert_eq!(
        diff["metadata"],
        json!([
          { "kind": "derived-from-changed", "subject": "derived_from",
            "old": null, "new": "cxf:fnv1a128:0123abcd" },
          { "kind": "sequence-renamed", "subject": "sequence", "old": "first", "new": "second" }
        ])
    );
    assert_eq!(
        diff["base"],
        json!({ "sequence": "first", "derived_from": null })
    );
    assert_eq!(
        diff["head"],
        json!({ "sequence": "second", "derived_from": "cxf:fnv1a128:0123abcd" })
    );
    assert_eq!(diff["summary"]["metadata_equal"], json!(false));
    assert_eq!(diff["summary"]["graph_equal"], json!(true));
}

#[test]
fn every_semantic_field_but_the_unit_is_metadata_under_boundary_dot_field() {
    let diff = diffed(
        "in T : Real @unit K @brick Sensor @s223 Old @qudt Temp @haystack air @haystack temp @name \"Zone\" @label \"zone\"\n",
        "in T : Real @unit K @brick Setpoint @s223 New @qudt Temperature @haystack air @haystack sp @name \"Room\" @label \"room\" @provisional\n",
    );
    assert_eq!(diff["interface"], json!([]));
    assert_eq!(
        diff["metadata"],
        json!([
          { "kind": "semantic-changed", "subject": "T.brick", "old": "Sensor", "new": "Setpoint" },
          { "kind": "semantic-changed", "subject": "T.haystack", "old": "air temp", "new": "air sp" },
          { "kind": "semantic-changed", "subject": "T.label", "old": "zone", "new": "room" },
          { "kind": "semantic-changed", "subject": "T.name", "old": "Zone", "new": "Room" },
          { "kind": "semantic-changed", "subject": "T.provisional", "old": "false", "new": "true" },
          { "kind": "semantic-changed", "subject": "T.qudt", "old": "Temp", "new": "Temperature" },
          { "kind": "semantic-changed", "subject": "T.s223", "old": "Old", "new": "New" }
        ])
    );
    let diff = diffed("in T : Real @haystack air\n", "in T : Real\n");
    assert_eq!(
        diff["metadata"],
        json!([{ "kind": "semantic-changed", "subject": "T.haystack", "old": "air", "new": null }])
    );
}

#[test]
fn a_comment_on_any_statement_is_metadata_under_the_statements_subject() {
    let base = parse(
        "# header\nsequence s\nderived_from none\n# p\nparam p : Real = 1.0\n# u\nin u : Real\n\
         # y\nout y : Real\n# b\nblock b = CDL.Reals.MultiplyByParameter(\n  # k\n  k = 2.0\n)\n\
         # in\nu -> b.u\nb.y -> y\n\n# tail\n",
    );
    let head = parse(
        "# header 2\nsequence s\nderived_from none\n# p 2\nparam p : Real = 1.0\n# u 2\nin u : Real\n\
         # y 2\nout y : Real\n# b 2\nblock b = CDL.Reals.MultiplyByParameter(\n  # k 2\n  k = 2.0\n)\n\
         # in 2\nu -> b.u\n# out\nb.y -> y\n\n# tail 2\n# more\n",
    );
    let diff = support::json(&compare(&base, &head));
    assert_eq!(
        diff["metadata"],
        json!([
          { "kind": "comment-changed", "subject": "b", "old": " b", "new": " b 2" },
          { "kind": "comment-changed", "subject": "b.k", "old": " k", "new": " k 2" },
          { "kind": "comment-changed", "subject": "b.y -> y", "old": null, "new": " out" },
          { "kind": "comment-changed", "subject": "p", "old": " p", "new": " p 2" },
          { "kind": "comment-changed", "subject": "sequence", "old": " header", "new": " header 2" },
          { "kind": "comment-changed", "subject": "sequence.trailing",
            "old": " tail", "new": " tail 2\n more" },
          { "kind": "comment-changed", "subject": "u", "old": " u", "new": " u 2" },
          { "kind": "comment-changed", "subject": "u -> b.u", "old": " in", "new": " in 2" },
          { "kind": "comment-changed", "subject": "y", "old": " y", "new": " y 2" }
        ])
    );
    // Comments are the whole difference: every other section is empty.
    for section in ["parameters", "interface", "graph"] {
        assert_eq!(diff[section], json!([]), "{section}");
    }
    assert_eq!(diff["summary"]["equal"], json!(false));
}

#[test]
fn a_blank_line_is_layout_and_is_not_compared() {
    let base = parse("sequence s\nderived_from none\nin u : Real\nout y : Real\n");
    let head = parse("sequence s\nderived_from none\n\nin u : Real\n\nout y : Real\n");
    assert_ne!(base, head, "the models differ in their blank-line flags");
    assert!(compare(&base, &head).summary.equal);
}
