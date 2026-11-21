//! One value written two ways (`literal-respelled`, diff note decision 14) against a change of
//! value: an Integer literal against the equal Real, only where a Real is expected and only up to
//! 2^53; a unit suffix or a written number that changes under one value; a suffix that changes
//! together with its value; and a bound on a boundary whose type changes with it.

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
fn an_integer_past_two_to_the_fifty_third_is_never_the_real_it_rounds_to() {
    // 2^53 is the last Integer every one of whose neighbours is a Real. 2^53 + 1 rounds to 2^53
    // as an f64: a different value, so a change, not a respelling.
    let diff = diffed(
        "param big : Real = 9007199254740992\n",
        "param big : Real = 9007199254740992.0\n",
    );
    assert_eq!(diff["parameters"], json!([]));
    assert_eq!(
        diff["metadata"],
        json!([{ "kind": "literal-respelled", "subject": "big",
                 "old": "9007199254740992", "new": "9007199254740992.0" }])
    );
    for integer in [
        "9007199254740993",
        "-9007199254740993",
        "9223372036854775807",
    ] {
        let real = if integer.starts_with('-') {
            "-9007199254740992.0"
        } else if integer.len() == 16 {
            "9007199254740992.0"
        } else {
            "9223372036854775807.0"
        };
        let diff = diffed(
            &format!("param big : Real = {integer}\n"),
            &format!("param big : Real = {real}\n"),
        );
        assert_eq!(
            support::kinds(&diff["parameters"]),
            [("changed".to_owned(), "big".to_owned())],
            "{integer} against {real}"
        );
        assert_eq!(diff["metadata"], json!([]), "{integer} against {real}");
    }
    // The same limit on a block parameter and on its catalog default.
    let diff = diffed(
        "block d = CDL.Logical.TrueDelay(delayTime = 9007199254740993)\n",
        "block d = CDL.Logical.TrueDelay(delayTime = 9007199254740992.0)\n",
    );
    assert_eq!(
        support::kinds(&diff["graph"]),
        [("reparameterized".to_owned(), "d.delayTime".to_owned())]
    );
    let diff = diffed(
        "block d = CDL.Logical.TrueDelay(delayTime = -9007199254740992)\n",
        "block d = CDL.Logical.TrueDelay(delayTime = -9007199254740992.0)\n",
    );
    assert_eq!(diff["graph"], json!([]));
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
fn a_value_and_its_unit_suffix_that_both_change_are_both_reported() {
    let diff = diffed(
        "block d = CDL.Logical.TrueDelay(delayTime = 60.0)\n",
        "block d = CDL.Logical.TrueDelay(delayTime = 120.0 s)\n",
    );
    assert_eq!(
        support::kinds(&diff["graph"]),
        [("reparameterized".to_owned(), "d.delayTime".to_owned())]
    );
    assert_eq!(
        diff["metadata"],
        json!([{ "kind": "literal-respelled", "subject": "d.delayTime",
                 "old": "60.0", "new": "120.0 s" }])
    );
    // A scaled suffix kept across a change of value is not a respelling: `1 h` to `2 h`.
    let diff = diffed(
        "block d = CDL.Logical.TrueDelay(delayTime = 1 h)\n",
        "block d = CDL.Logical.TrueDelay(delayTime = 2 h)\n",
    );
    assert_eq!(diff["metadata"], json!([]));
    // But another scaled suffix is: `1 h` to `90 min`.
    let diff = diffed(
        "block d = CDL.Logical.TrueDelay(delayTime = 1 h)\n",
        "block d = CDL.Logical.TrueDelay(delayTime = 90 min)\n",
    );
    assert_eq!(
        diff["metadata"],
        json!([{ "kind": "literal-respelled", "subject": "d.delayTime",
                 "old": "1 h", "new": "90 min" }])
    );
    assert_eq!(diff["graph"].as_array().expect("a section").len(), 1);
    // The same on a sequence parameter.
    let diff = diffed("param t : Real = 60.0\n", "param t : Real = 120.0 s\n");
    assert_eq!(
        support::kinds(&diff["parameters"]),
        [("changed".to_owned(), "t".to_owned())]
    );
    assert_eq!(
        diff["metadata"],
        json!([{ "kind": "literal-respelled", "subject": "t", "old": "60.0", "new": "120.0 s" }])
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
fn a_bound_is_respelled_only_when_both_sides_type_the_connector_real() {
    // The connector's type changes with the bound's spelling. On the Integer side the bound is
    // an Integer, so this is a change of type and a change of bound, in both directions.
    let integer = "in x : Integer @min 1\n";
    let real = "in x : Real @min 1.0\n";
    for (base, head, old, new) in [
        (
            integer,
            real,
            json!({ "integer": 1 }),
            json!({ "real": "1.0", "bits": "0x3ff0000000000000" }),
        ),
        (
            real,
            integer,
            json!({ "real": "1.0", "bits": "0x3ff0000000000000" }),
            json!({ "integer": 1 }),
        ),
    ] {
        let diff = diffed(base, head);
        assert_eq!(
            support::kinds(&diff["interface"]),
            [
                ("type-changed".to_owned(), "x".to_owned()),
                ("min-changed".to_owned(), "x".to_owned())
            ]
        );
        assert_eq!(diff["interface"][1]["old"], old);
        assert_eq!(diff["interface"][1]["new"], new);
        assert_eq!(diff["metadata"], json!([]), "nothing is merely respelled");
    }
}
