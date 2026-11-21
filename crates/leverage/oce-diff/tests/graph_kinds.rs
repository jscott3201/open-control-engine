//! One minimal pair per kind of the graph section (diff note, "Categories and sections" and
//! "Test plan", per category), each asserting the kind, the subject and the values: a class
//! change under one name, `-0.0` against `0.0`, a NaN payload change, a literal against a
//! reference, an assignment dropped back to its literal default and one against an expression
//! default, an input left undriven, and each of the three reorderings.

mod support;

use oce_author::{Literal, Sequence};
use serde_json::{Value, json};

use support::{block, boundary, compare, link, parse, sequence};

/// A sequence named `s` from the statements after the header.
fn model(body: &str) -> Sequence {
    parse(&format!("sequence s\nderived_from none\n{body}"))
}

/// The graph section of the diff of two statement lists.
fn graph(base: &str, head: &str) -> Value {
    support::json(&compare(&model(base), &model(head)))["graph"].take()
}

/// A sequence of one `CDL.Reals.Sources.Constant` named `c` with `k` set to `value`.
fn constant(value: f64) -> Sequence {
    let mut model = sequence("s");
    model.blocks.push(block(
        "c",
        "CDL.Reals.Sources.Constant",
        &[("k", Literal::Real(value))],
    ));
    model
}

#[test]
fn a_block_only_in_head_is_added_with_its_class_assignments_and_drivers() {
    let base = "in u : Real\n";
    let head = "in u : Real\nblock g = CDL.Reals.MultiplyByParameter(k = 2.0)\nu -> g.u\n";
    assert_eq!(
        graph(base, head),
        json!([{ "kind": "added", "subject": "g", "class": "CDL.Reals.MultiplyByParameter",
                 "params": [{ "name": "k", "value": { "real": "2.0", "bits": "0x4000000000000000" } }],
                 "inputs": [{ "port": "u", "source": "u" }] }])
    );
}

#[test]
fn a_block_only_in_base_is_removed_with_the_same() {
    let base = "in u : Real\nblock g = CDL.Reals.MultiplyByParameter(k = 2.0)\nu -> g.u\n";
    let head = "in u : Real\n";
    assert_eq!(
        graph(base, head),
        json!([{ "kind": "removed", "subject": "g", "class": "CDL.Reals.MultiplyByParameter",
                 "params": [{ "name": "k", "value": { "real": "2.0", "bits": "0x4000000000000000" } }],
                 "inputs": [{ "port": "u", "source": "u" }] }])
    );
}

#[test]
fn a_class_change_under_one_name_is_removed_plus_added_never_reparameterized() {
    let base = "block b = CDL.Reals.Abs\n";
    let head = "block b = CDL.Reals.Sqrt\n";
    assert_eq!(
        graph(base, head),
        json!([
          { "kind": "added", "subject": "b", "class": "CDL.Reals.Sqrt", "params": [], "inputs": [] },
          { "kind": "removed", "subject": "b", "class": "CDL.Reals.Abs", "params": [], "inputs": [] }
        ])
    );
    let diff = compare(&model(base), &model(head));
    assert_eq!(diff.summary.blocks.by_name, 0);
    assert_eq!(diff.summary.blocks.base_only, 1);
    assert_eq!(diff.summary.blocks.head_only, 1);
}

#[test]
fn a_consumer_of_a_block_whose_class_changed_keeps_its_driver_spelling_and_is_not_rewired() {
    // `b.y` is spelled the same on both sides. The block behind it is another one, which the
    // removed and added entries say; the consumer's own statement did not change.
    let base = "out y : Real\nblock b = CDL.Reals.Abs\nb.y -> y\n";
    let head = "out y : Real\nblock b = CDL.Reals.Sqrt\nb.y -> y\n";
    assert_eq!(
        support::kinds(&graph(base, head)),
        [
            ("added".to_owned(), "b".to_owned()),
            ("removed".to_owned(), "b".to_owned())
        ]
    );
}

#[test]
fn a_renamed_block_carries_its_old_name_class_assignments_and_confidence() {
    let base = "in u : Real\nblock old = CDL.Reals.MultiplyByParameter(k = 2.0)\nu -> old.u\n";
    let head = "in u : Real\nblock new = CDL.Reals.MultiplyByParameter(k = 2.0)\nu -> new.u\n";
    assert_eq!(
        graph(base, head),
        json!([{ "kind": "renamed", "subject": "new", "from": "old",
                 "class": "CDL.Reals.MultiplyByParameter",
                 "params": [{ "name": "k", "value": { "real": "2.0", "bits": "0x4000000000000000" } }],
                 "confidence": "exact" }])
    );
}

#[test]
fn a_changed_value_is_reparameterized_at_block_dot_param() {
    let base = "block g = CDL.Reals.MultiplyByParameter(k = 2.0)\n";
    let head = "block g = CDL.Reals.MultiplyByParameter(k = 3.0)\n";
    assert_eq!(
        graph(base, head),
        json!([{ "kind": "reparameterized", "subject": "g.k",
                 "old": { "real": "2.0", "bits": "0x4000000000000000" },
                 "new": { "real": "3.0", "bits": "0x4008000000000000" } }])
    );
}

#[test]
fn negative_zero_against_zero_is_reparameterized() {
    let diff = compare(&constant(-0.0), &constant(0.0));
    assert_eq!(
        support::json(&diff)["graph"],
        json!([{ "kind": "reparameterized", "subject": "c.k",
                 "old": { "real": "-0.0", "bits": "0x8000000000000000" },
                 "new": { "real": "0.0", "bits": "0x0000000000000000" } }])
    );
    assert!(compare(&constant(-0.0), &constant(-0.0)).summary.equal);
}

#[test]
fn a_nan_equals_only_its_own_payload() {
    let quiet = f64::from_bits(0x7ff8_0000_0000_0000);
    let payload = f64::from_bits(0x7ff8_0000_0000_0001);
    assert!(
        compare(&constant(payload), &constant(payload))
            .summary
            .equal
    );
    let diff = compare(&constant(quiet), &constant(payload));
    assert_eq!(
        support::json(&diff)["graph"],
        json!([{ "kind": "reparameterized", "subject": "c.k",
                 "old": { "real": "NaN", "bits": "0x7ff8000000000000" },
                 "new": { "real": "NaN", "bits": "0x7ff8000000000001" } }])
    );
    // Two NaNs spell alike, so the text carries the bits.
    assert!(
        oce_diff::render_text(&diff)
            .contains("~ c.k = NaN(0x7ff8000000000001)   # was NaN(0x7ff8000000000000)\n")
    );
}

#[test]
fn the_infinities_and_the_largest_integers_are_carried_exactly() {
    let diff = compare(&constant(f64::INFINITY), &constant(f64::NEG_INFINITY));
    assert_eq!(
        support::json(&diff)["graph"],
        json!([{ "kind": "reparameterized", "subject": "c.k",
                 "old": { "real": "inf", "bits": "0x7ff0000000000000" },
                 "new": { "real": "-inf", "bits": "0xfff0000000000000" } }])
    );
    let integer = |value: i64| {
        let mut model = sequence("s");
        model.blocks.push(block(
            "c",
            "CDL.Integers.Sources.Constant",
            &[("k", Literal::Integer(value))],
        ));
        model
    };
    let diff = compare(&integer(i64::MIN), &integer(i64::MAX));
    assert_eq!(
        support::json(&diff)["graph"],
        json!([{ "kind": "reparameterized", "subject": "c.k",
                 "old": { "integer": i64::MIN }, "new": { "integer": i64::MAX } }])
    );
    // 2^53 and 2^53 + 1 are one f64 and two Integers.
    let diff = compare(&integer(1 << 53), &integer((1 << 53) + 1));
    assert_eq!(diff.graph.len(), 1);
}

#[test]
fn a_literal_against_a_reference_is_reparameterized_and_never_grounded() {
    // The parameter holds the very value the literal has; the diff reports what was authored.
    let base = "param gain : Real = 2.0\nblock g = CDL.Reals.MultiplyByParameter(k = 2.0)\n";
    let head = "param gain : Real = 2.0\nblock g = CDL.Reals.MultiplyByParameter(k = gain)\n";
    assert_eq!(
        graph(base, head),
        json!([{ "kind": "reparameterized", "subject": "g.k",
                 "old": { "real": "2.0", "bits": "0x4000000000000000" },
                 "new": { "ref": "gain" } }])
    );
}

#[test]
fn an_enumeration_member_and_a_string_are_compared_as_spelled() {
    let base = "block c = CDL.Reals.PID(controllerType = SimpleController.PI)\n";
    let head = "block c = CDL.Reals.PID(controllerType = SimpleController.PID)\n";
    assert_eq!(
        graph(base, head),
        json!([{ "kind": "reparameterized", "subject": "c.controllerType",
                 "old": { "member": "SimpleController.PI" },
                 "new": { "member": "SimpleController.PID" } }])
    );
    let text = |value: &str| {
        let mut model = sequence("s");
        model.blocks.push(block(
            "a",
            "CDL.Utilities.Assert",
            &[("message", Literal::Text(value.to_owned()))],
        ));
        model
    };
    let diff = compare(&text("low"), &text("high"));
    assert_eq!(
        support::json(&diff)["graph"],
        json!([{ "kind": "reparameterized", "subject": "a.message",
                 "old": { "text": "low" }, "new": { "text": "high" } }])
    );
}

#[test]
fn an_assignment_dropped_back_to_its_literal_default_is_parameter_defaulted() {
    // `delayOnInit` defaults to `false` in the catalog.
    let with = "block d = CDL.Logical.TrueDelay(delayTime = 60.0, delayOnInit = false)\n";
    let without = "block d = CDL.Logical.TrueDelay(delayTime = 60.0)\n";
    assert_eq!(
        graph(with, without),
        json!([{ "kind": "parameter-defaulted", "subject": "d.delayOnInit",
                 "value": { "boolean": false }, "side": "base" }])
    );
    assert_eq!(
        graph(without, with),
        json!([{ "kind": "parameter-defaulted", "subject": "d.delayOnInit",
                 "value": { "boolean": false }, "side": "head" }])
    );
}

#[test]
fn an_assignment_on_one_side_that_is_not_the_default_is_reparameterized_with_a_null_side() {
    let with = "block d = CDL.Logical.TrueDelay(delayTime = 60.0, delayOnInit = true)\n";
    let without = "block d = CDL.Logical.TrueDelay(delayTime = 60.0)\n";
    assert_eq!(
        graph(with, without),
        json!([{ "kind": "reparameterized", "subject": "d.delayOnInit",
                 "old": { "boolean": true }, "new": null }])
    );
    assert_eq!(
        graph(without, with),
        json!([{ "kind": "reparameterized", "subject": "d.delayOnInit",
                 "old": null, "new": { "boolean": true } }])
    );
}

#[test]
fn an_assignment_against_an_expression_default_is_never_parameter_defaulted() {
    // `fallingSlewRate` defaults to `-raisingSlewRate`: the assigned -2.0 is what the expression
    // gives, and the diff does not evaluate it.
    let with = "block r = CDL.Reals.LimitSlewRate(raisingSlewRate = 2.0, fallingSlewRate = -2.0)\n";
    let without = "block r = CDL.Reals.LimitSlewRate(raisingSlewRate = 2.0)\n";
    assert_eq!(
        graph(with, without),
        json!([{ "kind": "reparameterized", "subject": "r.fallingSlewRate",
                 "old": { "real": "-2.0", "bits": "0xc000000000000000" }, "new": null }])
    );
}

#[test]
fn a_required_parameter_a_reference_and_an_unknown_class_are_never_parameter_defaulted() {
    // Required: no default to fall back to.
    let kinds = |base: &str, head: &str| support::kinds(&graph(base, head));
    let reparameterized = |subject: &str| [("reparameterized".to_owned(), subject.to_owned())];
    assert_eq!(
        kinds(
            "block d = CDL.Logical.TrueDelay(delayTime = 60.0)\n",
            "block d = CDL.Logical.TrueDelay\n"
        ),
        reparameterized("d.delayTime")
    );
    // A reference is not a literal, whatever the parameter holds.
    assert_eq!(
        kinds(
            "param no : Boolean = false\nblock d = CDL.Logical.TrueDelay(delayOnInit = no)\n",
            "param no : Boolean = false\nblock d = CDL.Logical.TrueDelay\n"
        ),
        reparameterized("d.delayOnInit")
    );
    // A class and a parameter the catalog does not know have no default to read.
    assert_eq!(
        kinds(
            "block d = CDL.Nowhere.Nothing(delayOnInit = false)\n",
            "block d = CDL.Nowhere.Nothing\n"
        ),
        reparameterized("d.delayOnInit")
    );
    assert_eq!(
        kinds(
            "block d = CDL.Logical.TrueDelay(nothing = false)\n",
            "block d = CDL.Logical.TrueDelay\n"
        ),
        reparameterized("d.nothing")
    );
}

#[test]
fn an_integer_literal_equal_to_a_real_default_is_parameter_defaulted() {
    // `k` of `CDL.Reals.PID` defaults to the Real 1.0; the Integer 1 means the same there.
    assert_eq!(
        graph(
            "block c = CDL.Reals.PID(k = 1)\n",
            "block c = CDL.Reals.PID\n"
        ),
        json!([{ "kind": "parameter-defaulted", "subject": "c.k",
                 "value": { "integer": 1 }, "side": "base" }])
    );
    // `yMin` defaults to +0.0; -0.0 is another value.
    let mut with = sequence("s");
    with.blocks.push(block(
        "c",
        "CDL.Reals.PID",
        &[("yMin", Literal::Real(-0.0))],
    ));
    let mut without = sequence("s");
    without.blocks.push(block("c", "CDL.Reals.PID", &[]));
    assert_eq!(
        support::kinds(&support::json(&compare(&with, &without))["graph"]),
        [("reparameterized".to_owned(), "c.yMin".to_owned())]
    );
}

#[test]
fn a_changed_driver_is_rewired_once_at_its_target() {
    let base = "in a : Real\nin b : Real\nblock g = CDL.Reals.Abs\na -> g.u\n";
    let head = "in a : Real\nin b : Real\nblock g = CDL.Reals.Abs\nb -> g.u\n";
    assert_eq!(
        graph(base, head),
        json!([{ "kind": "rewired", "subject": "g.u", "old": "a", "new": "b" }])
    );
}

#[test]
fn an_input_left_undriven_is_rewired_to_null() {
    let driven = "in a : Real\nblock g = CDL.Reals.Abs\na -> g.u\n";
    let undriven = "in a : Real\nblock g = CDL.Reals.Abs\n";
    assert_eq!(
        graph(driven, undriven),
        json!([{ "kind": "rewired", "subject": "g.u", "old": "a", "new": null }])
    );
    assert_eq!(
        graph(undriven, driven),
        json!([{ "kind": "rewired", "subject": "g.u", "old": null, "new": "a" }])
    );
}

#[test]
fn a_boundary_output_is_a_target_like_any_other() {
    let base = "out y : Real\nblock a = CDL.Reals.Abs\nblock b = CDL.Reals.Abs\na.y -> y\n";
    let head = "out y : Real\nblock a = CDL.Reals.Abs\nblock b = CDL.Reals.Abs\nb.y -> y\n";
    assert_eq!(
        graph(base, head),
        json!([{ "kind": "rewired", "subject": "y", "old": "a.y", "new": "b.y" }])
    );
}

#[test]
fn a_renamed_source_alone_does_not_rewire_its_targets() {
    let base = "in u : Real\nout y : Real\nblock old = CDL.Reals.Abs\nblock t = CDL.Reals.Sqrt\n\
                u -> old.u\nold.y -> t.u\nt.y -> y\n";
    let head = "in u : Real\nout y : Real\nblock new = CDL.Reals.Abs\nblock t = CDL.Reals.Sqrt\n\
                u -> new.u\nnew.y -> t.u\nt.y -> y\n";
    assert_eq!(
        support::kinds(&graph(base, head)),
        [("renamed".to_owned(), "new".to_owned())]
    );
}

#[test]
fn the_three_lists_are_each_reported_reordered_once() {
    let base = "in u : Real\nblock a = CDL.Reals.Limiter(uMax = 1.0, uMin = 0.0)\n\
                block b = CDL.Reals.Abs\nblock c = CDL.Reals.Abs\nu -> a.u\nu -> b.u\nu -> c.u\n";
    let head = "in u : Real\nblock c = CDL.Reals.Abs\nblock b = CDL.Reals.Abs\n\
                block a = CDL.Reals.Limiter(uMin = 0.0, uMax = 1.0)\nu -> c.u\nu -> b.u\nu -> a.u\n";
    assert_eq!(
        graph(base, head),
        json!([
          { "kind": "reordered", "subject": "a", "list": "assignments" },
          { "kind": "reordered", "subject": "blocks", "list": "blocks" },
          { "kind": "reordered", "subject": "connections", "list": "connections" }
        ])
    );
}

#[test]
fn adding_or_removing_an_element_is_not_a_reordering() {
    let base = "block a = CDL.Reals.Abs\nblock b = CDL.Reals.Abs\nblock c = CDL.Reals.Abs\n";
    let head = "block a = CDL.Reals.Abs\nblock x = CDL.Reals.Sqrt\nblock c = CDL.Reals.Abs\n";
    assert_eq!(
        support::kinds(&graph(base, head)),
        [
            ("removed".to_owned(), "b".to_owned()),
            ("added".to_owned(), "x".to_owned())
        ]
    );
}

#[test]
fn a_block_named_like_a_list_is_told_from_the_list() {
    // `blocks` is a legal block name. Its assignments and the block list both move here.
    let base =
        "block blocks = CDL.Reals.Limiter(uMax = 1.0, uMin = 0.0)\nblock z = CDL.Reals.Abs\n";
    let head =
        "block z = CDL.Reals.Abs\nblock blocks = CDL.Reals.Limiter(uMin = 0.0, uMax = 1.0)\n";
    assert_eq!(
        graph(base, head),
        json!([
          { "kind": "reordered", "subject": "blocks", "list": "blocks" },
          { "kind": "reordered", "subject": "blocks", "list": "assignments" }
        ])
    );
}

#[test]
fn empty_models_and_models_without_blocks_have_an_empty_graph() {
    let empty = sequence("s");
    let diff = compare(&empty, &empty);
    assert!(diff.summary.equal);
    assert_eq!(diff.summary.blocks, oce_diff::BlockCounts::default());
    assert!(diff.notes.is_empty());

    // One side empty: everything is added, and nothing is "disjoint" about no blocks.
    let mut full = sequence("s");
    full.inputs
        .push(boundary("u", oce_author::SignalType::Real));
    full.blocks.push(block("a", "CDL.Reals.Abs", &[]));
    full.connections.push(link("u", "a.u"));
    let diff = compare(&empty, &full);
    assert!(diff.notes.is_empty());
    assert_eq!(diff.summary.blocks.head_only, 1);
    assert_eq!(
        support::kinds(&support::json(&diff)["graph"]),
        [("added".to_owned(), "a".to_owned())]
    );
    let diff = compare(&full, &empty);
    assert!(diff.notes.is_empty());
    assert_eq!(diff.summary.blocks.base_only, 1);
}
