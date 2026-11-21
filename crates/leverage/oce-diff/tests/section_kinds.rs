//! One minimal pair per kind of the parameters, interface and metadata sections (diff note,
//! "Categories and sections" and "Site parameters and the graph"), each asserting the kind, the
//! subject and the values: a sequence-parameter change with what it affects, every boundary
//! attribute, every semantic field and a comment on every kind of statement, and what a
//! reordering does to the summary. `literal_spellings.rs` holds the `literal-respelled` cases.

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
fn what_a_parameter_affects_is_sorted_and_lists_each_assignment_once() {
    // `z` is declared before `a`, and `a` assigns `k` twice (which a validator refuses): the
    // list is in byte order whatever the authored order, with `a.k` once.
    let side = |value: f64| {
        let mut model = model(&format!(
            "param gain : Real = {value:?}\n\
             block z = CDL.Reals.MultiplyByParameter(k = gain)\n\
             block a = CDL.Reals.MultiplyByParameter(k = gain)\n"
        ));
        let again = model.blocks[1].assignments[0].clone();
        model.blocks[1].assignments.push(again);
        model
    };
    let diff = support::json(&compare(&side(2.0), &side(3.0)));
    assert_eq!(diff["parameters"][0]["affects"], json!(["a.k", "z.k"]));
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
    // Order is not semantic: the parameters are the same parameters, and something did change.
    assert_eq!(diff["summary"]["parameters_equal"], json!(true));
    assert_eq!(diff["summary"]["equal"], json!(false));
}

#[test]
fn a_reordering_alone_leaves_every_section_flag_true_and_equal_false() {
    // Decision 16: a `reordered`-only diff is not `equal`, and a script that wants "same
    // structure" reads `graph_equal`. Every list of the model, each reordered alone.
    let base = "param a : Real = 1.0\nparam b : Real = 2.0\n\
                in u : Real\nin v : Real\nout y : Real\nout z : Real\n\
                block p = CDL.Reals.Limiter(uMax = 1.0, uMin = 0.0)\nblock q = CDL.Reals.Abs\n\
                u -> p.u\nv -> q.u\np.y -> y\nq.y -> z\n";
    let heads = [
        (
            "parameters",
            base.replace(
                "param a : Real = 1.0\nparam b : Real = 2.0\n",
                "param b : Real = 2.0\nparam a : Real = 1.0\n",
            ),
        ),
        (
            "interface",
            base.replace("in u : Real\nin v : Real\n", "in v : Real\nin u : Real\n"),
        ),
        (
            "interface",
            base.replace(
                "out y : Real\nout z : Real\n",
                "out z : Real\nout y : Real\n",
            ),
        ),
        (
            "graph",
            base.replace("uMax = 1.0, uMin = 0.0", "uMin = 0.0, uMax = 1.0"),
        ),
        (
            "graph",
            base.replace("u -> p.u\nv -> q.u\n", "v -> q.u\nu -> p.u\n"),
        ),
        (
            "graph",
            base.replace(
                "block p = CDL.Reals.Limiter(uMax = 1.0, uMin = 0.0)\nblock q = CDL.Reals.Abs\n",
                "block q = CDL.Reals.Abs\nblock p = CDL.Reals.Limiter(uMax = 1.0, uMin = 0.0)\n",
            ),
        ),
    ];
    for (section, head) in heads {
        assert_ne!(head, base, "{section}: the head is another text");
        let diff = diffed(base, &head);
        assert_eq!(
            support::kinds(&diff[section])
                .into_iter()
                .map(|(kind, _)| kind)
                .collect::<Vec<_>>(),
            ["reordered"],
            "{section}"
        );
        assert_eq!(
            diff["summary"],
            json!({ "equal": false, "graph_equal": true, "parameters_equal": true,
                    "interface_equal": true, "metadata_equal": true,
                    "blocks": { "by_name": 2, "renamed": 0, "base_only": 0, "head_only": 0 } }),
            "{section}"
        );
    }
    // The text says both things: the header calls the graph equal, the section lists the move.
    let moved = base.replace("u -> p.u\nv -> q.u\n", "v -> q.u\nu -> p.u\n");
    assert_eq!(
        oce_diff::render_text(&compare(&model(base), &model(&moved))),
        "# oce diff s -> s\n\
         # graph equal; parameters equal; interface equal; metadata equal\n\
         # graph\n\
         ~ # connections reordered\n"
    );
    // A reordering beside a real change does not hide the change.
    let both = moved.replace("uMax = 1.0", "uMax = 2.0");
    let diff = diffed(base, &both);
    assert_eq!(diff["summary"]["graph_equal"], json!(false));
}

#[test]
fn what_a_changed_parameter_affects_is_read_from_head() {
    // Head adds a second assignment that references `p`. The parameter changed between the two
    // sides, and what it affects now is what head says: both assignments.
    let base = "param p : Real = 1.0\n\
                block a = CDL.Reals.MultiplyByParameter(k = p)\n\
                block b = CDL.Reals.MultiplyByParameter(k = 5.0)\n";
    let head = "param p : Real = 2.0\n\
                block a = CDL.Reals.MultiplyByParameter(k = p)\n\
                block b = CDL.Reals.MultiplyByParameter(k = p)\n";
    assert_eq!(
        diffed(base, head)["parameters"][0]["affects"],
        json!(["a.k", "b.k"])
    );
    // The other way round head is the side with one reference.
    assert_eq!(
        diffed(head, base)["parameters"][0]["affects"],
        json!(["a.k"])
    );
    // The same for a changed type.
    let retyped = head.replace("param p : Real = 2.0", "param p : Integer = 2");
    let diff = diffed(base, &retyped);
    assert_eq!(diff["parameters"][0]["kind"], json!("type-changed"));
    assert_eq!(diff["parameters"][0]["affects"], json!(["a.k", "b.k"]));
    let diff = diffed(&retyped, base);
    assert_eq!(diff["parameters"][0]["affects"], json!(["a.k"]));
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
