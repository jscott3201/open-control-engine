//! What the CXF adapter reports for a ground value or a boundary that changed (issue #666): one
//! minimal pair per entry kind it can make, each value kind it compares, both sides of every
//! one-sided entry, and what a loaded document does not hold.

mod support;

use serde_json::json;

use support::loaded::{compare_loaded, corpus_bytes, load, loaded};
use support::parse;

/// A sequence named `s` from its statements.
fn model(statements: &str) -> oce_author::Sequence {
    parse(&format!("sequence s\nderived_from none\n\n{statements}"))
}

/// The JSON of the CXF diff of two models, each emitted and loaded.
fn cxf(base: &str, head: &str) -> serde_json::Value {
    let diff = compare_loaded(&loaded(&model(base)), &loaded(&model(head)));
    support::json(&diff)
}

/// A PID controller, an Integer source and a Boolean source, with assignments to vary.
fn plant(pid: &str, k: &str, flag: &str) -> String {
    format!(
        "in uSet : Real\nin uMea : Real\nout y : Real\nout n : Integer\nout b : Boolean\n\n\
         block pid = CDL.Reals.PID({pid})\n\
         block count = CDL.Integers.Sources.Constant(k = {k})\n\
         block flag = CDL.Logical.Sources.Constant(k = {flag})\n\n\
         uSet -> pid.u_s\nuMea -> pid.u_m\npid.y -> y\ncount.y -> n\nflag.y -> b\n"
    )
}

#[test]
fn a_ground_value_of_each_kind_that_changed_is_reparameterized_on_its_leaf() {
    let base = plant(
        "controllerType = SimpleController.PI, k = 0.5, reverseActing = true",
        "3",
        "true",
    );
    let head = plant(
        "controllerType = SimpleController.P, k = 0.75, reverseActing = false",
        "-4",
        "false",
    );
    let diff = cxf(&base, &head);
    assert_eq!(
        diff["graph"],
        json!([
            { "kind": "reparameterized", "subject": "count.k",
              "old": { "integer": 3 }, "new": { "integer": -4 } },
            { "kind": "reparameterized", "subject": "flag.k",
              "old": { "boolean": true }, "new": { "boolean": false } },
            { "kind": "reparameterized", "subject": "pid.controllerType",
              "old": { "member": "SimpleController.PI" },
              "new": { "member": "SimpleController.P" } },
            { "kind": "reparameterized", "subject": "pid.k",
              "old": { "real": "0.5", "bits": "0x3fe0000000000000" },
              "new": { "real": "0.75", "bits": "0x3fe8000000000000" } },
            { "kind": "reparameterized", "subject": "pid.reverseActing",
              "old": { "boolean": true }, "new": { "boolean": false } }
        ])
    );
    assert_eq!(diff["summary"]["graph_equal"], false);
    assert_eq!(diff["summary"]["parameters_equal"], json!(null));
    assert_eq!(diff["summary"]["metadata_equal"], json!(null));
    assert_eq!(diff["parameters"], json!([]));
    assert_eq!(diff["metadata"], json!([]));
    // Nothing changed is nothing reported, value kind by value kind.
    assert_eq!(cxf(&base, &base)["graph"], json!([]));
}

#[test]
fn signed_zeros_are_two_values_by_their_bits() {
    let gain = |k: &str| {
        format!(
            "in u : Real\nout y : Real\n\nblock g = CDL.Reals.MultiplyByParameter(k = {k})\n\nu -> g.u\ng.y -> y\n"
        )
    };
    let (positive, negative) = (loaded(&model(&gain("0.0"))), loaded(&model(&gain("-0.0"))));
    assert_ne!(positive.id, negative.id);
    let diff = support::json(&compare_loaded(&positive, &negative));
    assert_eq!(
        diff["graph"],
        json!([{ "kind": "reparameterized", "subject": "g.k",
                 "old": { "real": "0.0", "bits": "0x0000000000000000" },
                 "new": { "real": "-0.0", "bits": "0x8000000000000000" } }])
    );
    // A bound of each sign is another effective domain, another id and an interface entry.
    let bounded = |min: &str| {
        format!(
            "in u : Real @min {min}\nout y : Real\n\nblock g = CDL.Reals.MultiplyByParameter(k = 1.0)\n\nu -> g.u\ng.y -> y\n"
        )
    };
    let (positive, negative) = (
        loaded(&model(&bounded("0.0"))),
        loaded(&model(&bounded("-0.0"))),
    );
    assert_ne!(positive.id, negative.id);
    let diff = support::json(&compare_loaded(&positive, &negative));
    assert_eq!(
        diff["interface"],
        json!([{ "kind": "min-changed", "subject": "u", "direction": "in",
                 "old": { "real": "0.0", "bits": "0x0000000000000000" },
                 "new": { "real": "-0.0", "bits": "0x8000000000000000" } }])
    );
}

#[test]
fn an_assignment_on_one_side_is_defaulted_against_a_literal_default_and_reparameterized_otherwise()
{
    let bare = plant("", "3", "true");
    // The catalog's literal defaults: `k = 1.0`, `reverseActing = true`, `controllerType = PI`.
    let defaults = plant(
        "controllerType = SimpleController.PI, k = 1.0, reverseActing = true",
        "3",
        "true",
    );
    let others = plant(
        "controllerType = SimpleController.PD, k = 2.0, reverseActing = false",
        "3",
        "true",
    );
    for (side, base, head) in [("head", &bare, &defaults), ("base", &defaults, &bare)] {
        let diff = cxf(base, head);
        assert_eq!(
            diff["graph"],
            json!([
                { "kind": "parameter-defaulted", "subject": "pid.controllerType",
                  "value": { "member": "SimpleController.PI" }, "side": side },
                { "kind": "parameter-defaulted", "subject": "pid.k",
                  "value": { "real": "1.0", "bits": "0x3ff0000000000000" }, "side": side },
                { "kind": "parameter-defaulted", "subject": "pid.reverseActing",
                  "value": { "boolean": true }, "side": side }
            ]),
            "assigned on {side}"
        );
        // An explicit default is still another document.
        assert_ne!(loaded(&model(base)).id, loaded(&model(head)).id);
    }
    let added = cxf(&bare, &others);
    let removed = cxf(&others, &bare);
    let k = json!({ "real": "2.0", "bits": "0x4000000000000000" });
    assert_eq!(
        added["graph"][1],
        json!({ "kind": "reparameterized", "subject": "pid.k", "old": null, "new": k })
    );
    assert_eq!(
        removed["graph"][1],
        json!({ "kind": "reparameterized", "subject": "pid.k", "old": k, "new": null })
    );
    assert_eq!(added["graph"].as_array().map(Vec::len), Some(3));
    assert_eq!(removed["graph"].as_array().map(Vec::len), Some(3));
}

#[test]
fn an_integer_literal_on_a_real_parameter_is_the_ground_value_of_its_real_spelling() {
    // Issue #683 (owner ruling): the engine holds an Integer bound to a parameter the catalog
    // types Real as that Real, at load. So the tracked document that writes `k = 0` and the same
    // document respelled `0.0` load alike: one content id, and nothing to report at this level.
    let path = "crates/oce-cxf/tests/fixtures/pass_through_miniature.jsonld";
    let integer = corpus_bytes(path);
    let text = String::from_utf8(integer.clone()).expect("UTF-8");
    assert_eq!(text.matches("\"S231:value\": 0\n").count(), 1);
    let real = text.replace("\"S231:value\": 0\n", "\"S231:value\": 0.0\n");
    let other = text.replace("\"S231:value\": 0\n", "\"S231:value\": 1\n");
    let (integer, real, other) = (
        load(&integer),
        load(real.as_bytes()),
        load(other.as_bytes()),
    );
    assert_eq!(integer.id, real.id);
    let diff = compare_loaded(&integer, &real);
    assert_eq!(support::json(&diff)["graph"], json!([]));
    assert!(diff.summary.graph_equal);
    assert!(diff.summary.equal);
    // Not vacuous: another Integer is another value, and it is reported as the Real it is held
    // as on both sides.
    assert_ne!(integer.id, other.id);
    assert_eq!(
        support::json(&compare_loaded(&integer, &other))["graph"],
        json!([{ "kind": "reparameterized", "subject": "keep.k",
                 "old": { "real": "0.0", "bits": "0x0000000000000000" },
                 "new": { "real": "1.0", "bits": "0x3ff0000000000000" } }])
    );
    // The authoring level writes the Real it means, so the same two spellings authored are one
    // document too: the emitter widens the Integer.
    let gain = |k: &str| {
        format!(
            "in u : Real\nout y : Real\n\nblock g = CDL.Reals.MultiplyByParameter(k = {k})\n\nu -> g.u\ng.y -> y\n"
        )
    };
    let (written, widened) = (loaded(&model(&gain("0"))), loaded(&model(&gain("0.0"))));
    assert_eq!(written.id, widened.id);
    assert!(compare_loaded(&written, &widened).summary.equal);
}

/// A gain between an input and an output, with the two boundaries' declarations to vary.
fn gain(input: &str, output: &str) -> String {
    format!(
        "in u : {input}\nout y : {output}\n\nblock g = CDL.Reals.MultiplyByParameter(k = 2.0)\n\nu -> g.u\ng.y -> y\n"
    )
}

#[test]
fn each_attribute_of_a_connector_is_its_own_entry() {
    let base = gain(
        "Real @unit K @quantity ThermodynamicTemperature @displayUnit degC @min 0.0 @max 10.0",
        "Real @unit K @quantity ThermodynamicTemperature @displayUnit degC",
    );
    let head = gain(
        "Real @unit degC @quantity Temperature @displayUnit K @min 1.0 @max 11.0",
        "Real @unit degC @quantity Temperature @displayUnit degF",
    );
    let diff = cxf(&base, &head);
    let one = |kind: &str, subject: &str, direction: &str, old: serde_json::Value, new| json!({ "kind": kind, "subject": subject, "direction": direction, "old": old, "new": new });
    let real = |text: &str, bits: &str| json!({ "real": text, "bits": bits });
    assert_eq!(
        diff["interface"],
        json!([
            one("unit-changed", "u", "in", json!("K"), json!("degC")),
            one(
                "quantity-changed",
                "u",
                "in",
                json!("ThermodynamicTemperature"),
                json!("Temperature")
            ),
            one("display-unit-changed", "u", "in", json!("degC"), json!("K")),
            one(
                "min-changed",
                "u",
                "in",
                real("0.0", "0x0000000000000000"),
                real("1.0", "0x3ff0000000000000")
            ),
            one(
                "max-changed",
                "u",
                "in",
                real("10.0", "0x4024000000000000"),
                real("11.0", "0x4026000000000000")
            ),
            one("unit-changed", "y", "out", json!("K"), json!("degC")),
            one(
                "quantity-changed",
                "y",
                "out",
                json!("ThermodynamicTemperature"),
                json!("Temperature")
            ),
            one(
                "display-unit-changed",
                "y",
                "out",
                json!("degC"),
                json!("degF")
            ),
        ])
    );
    assert_eq!(diff["summary"]["interface_equal"], false);
    assert_eq!(diff["summary"]["graph_equal"], true);
    assert_eq!(diff["graph"], json!([]));
    // Each the other way round, as its own entry.
    let back = cxf(&head, &base);
    assert_eq!(back["interface"].as_array().map(Vec::len), Some(8));
    assert_eq!(back["interface"][0]["old"], json!("degC"));
}

#[test]
fn an_integer_input_is_bounded_by_its_effective_domain() {
    let count = |declaration: &str| {
        format!(
            "in n : Integer{declaration}\nout y : Integer\n\nblock a = CDL.Integers.Add\n\nn -> a.u1\nn -> a.u2\na.y -> y\n"
        )
    };
    let diff = cxf(&count(""), &count(" @min -3 @max 7"));
    let one = |kind: &str, old: i64, new: i64| {
        json!({ "kind": kind, "subject": "n", "direction": "in",
                "old": { "integer": old }, "new": { "integer": new } })
    };
    // An Integer input left open is the CDL `i32` range, not unbounded.
    assert_eq!(
        diff["interface"],
        json!([
            one("min-changed", i64::from(i32::MIN), -3),
            one("max-changed", i64::from(i32::MAX), 7)
        ])
    );
}

#[test]
fn a_connector_added_or_removed_carries_its_declaration_and_an_output_no_bounds() {
    let base = gain("Real", "Real");
    let head = format!(
        "{}in v : Real @unit K @quantity ThermodynamicTemperature @displayUnit degC @min -1.0 @max 1.0\n\
         out z : Real @unit K @min 0.0\n\nblock h = CDL.Reals.Abs\n\nv -> h.u\nh.y -> z\n",
        gain("Real", "Real")
    );
    let diff = cxf(&base, &head);
    let added = json!([
        { "kind": "added", "subject": "v", "direction": "in", "type": "Real", "unit": "K",
          "quantity": "ThermodynamicTemperature", "display_unit": "degC",
          "min": { "real": "-1.0", "bits": "0xbff0000000000000" },
          "max": { "real": "1.0", "bits": "0x3ff0000000000000" } },
        { "kind": "added", "subject": "z", "direction": "out", "type": "Real", "unit": "K",
          "quantity": null, "display_unit": null, "min": null, "max": null }
    ]);
    assert_eq!(diff["interface"], added);
    let removed = cxf(&head, &base);
    for at in 0..2 {
        let mut entry = added[at].clone();
        entry["kind"] = json!("removed");
        assert_eq!(removed["interface"][at], entry);
    }
}

#[test]
fn a_type_change_through_a_pass_through_is_an_entry_on_both_connectors() {
    let pass = |kind: &str| {
        format!(
            "in u : {kind}\nin w : Real\nout y : {kind}\nout z : Real\n\nblock g = CDL.Reals.Abs\n\nu -> y\nw -> g.u\ng.y -> z\n"
        )
    };
    // Boolean, which has no bounds: an Integer input would also gain its `i32` domain.
    let diff = cxf(&pass("Real"), &pass("Boolean"));
    let one = |subject: &str, direction: &str| {
        json!({ "kind": "type-changed", "subject": subject, "direction": direction,
                "old": "Real", "new": "Boolean" })
    };
    assert_eq!(diff["interface"], json!([one("u", "in"), one("y", "out")]));
    assert_eq!(diff["graph"], json!([]));
    // A pass-through is a link of two boundaries: rewiring it is a graph entry at the output.
    let both = |source: &str| {
        format!(
            "in u : Real\nin v : Real\nout y : Real\n\n{source} -> y\nblock g = CDL.Reals.Abs\n\nv -> g.u\n"
        )
    };
    let rewired = cxf(&both("u"), &both("v"));
    assert_eq!(
        rewired["graph"],
        json!([{ "kind": "rewired", "subject": "y", "old": "u", "new": "v" }])
    );
}

#[test]
fn what_a_loaded_document_does_not_hold_is_not_reported() {
    // An output's bounds, an input nothing reads, a comment and authored order are outside the
    // loaded graph and its content id: the two documents have one id and an empty diff.
    let base = "in u : Real\nout y : Real\n\nblock g = CDL.Reals.MultiplyByParameter(k = 2.0)\nblock h = CDL.Reals.Abs\n\nu -> g.u\ng.y -> h.u\nh.y -> y\n";
    let head = "# A comment.\nin u : Real\nin unread : Real\nout y : Real @min 0.0 @max 1.0\n\nblock h = CDL.Reals.Abs\nblock g = CDL.Reals.MultiplyByParameter(k = 2.0)\n\nh.y -> y\ng.y -> h.u\nu -> g.u\n";
    let (base, head) = (loaded(&model(base)), loaded(&model(head)));
    assert_eq!(base.id, head.id);
    let diff = compare_loaded(&base, &head);
    assert!(diff.summary.equal, "{}", diff.to_json());
}

#[test]
fn a_boundary_type_and_a_ground_value_are_spelled_as_the_authoring_form_spells_them() {
    // No tracked document has an enumeration or String boundary, so a loaded topology is edited
    // to carry each type the facade can report.
    use oce_api::{EnumClassId, Value, ValueType};
    let base = loaded(&model(&gain("Real", "Real"))).topology;
    let controller = oce_api::enumeration("CDL.Types.SimpleController")
        .and_then(|row| row.id)
        .expect("an enumeration");
    for (value_type, spelling) in [
        (ValueType::Integer, "Integer"),
        (ValueType::Boolean, "Boolean"),
        (ValueType::String, "String"),
        (ValueType::Enum(controller), "SimpleController"),
        (
            ValueType::Enum(EnumClassId(u32::MAX)),
            "Enum(EnumClassId(4294967295))",
        ),
    ] {
        let mut head = base.clone();
        head.boundary_inputs[0].value_type = value_type;
        let diff = support::json(&support::loaded::compare_topologies(&base, &head));
        assert_eq!(
            diff["interface"],
            json!([{ "kind": "type-changed", "subject": "u", "direction": "in",
                     "old": "Real", "new": spelling }]),
            "{spelling}"
        );
    }
    // A bound is read through no rule: an enumeration value the table does not hold is kept,
    // as its debug text.
    let mut head = base.clone();
    head.boundary_inputs[0].min = Some(Value::Enum {
        class: controller,
        ordinal: 9,
    });
    let diff = support::json(&support::loaded::compare_topologies(&base, &head));
    assert_eq!(
        diff["interface"][0]["new"],
        json!({ "text": "Enum { class: EnumClassId(1), ordinal: 9 }" })
    );
    // A ground value is read through the class's rules too, which name the ports: an
    // enumeration member by its ordinal, and a value the rules refuse is no block this level
    // can wire.
    let pid = loaded(&model(&plant(
        "controllerType = SimpleController.PI",
        "3",
        "true",
    )))
    .topology;
    let at = pid
        .blocks
        .iter()
        .position(|block| block.class_iri == "CDL.Reals.PID")
        .expect("the controller");
    for (ordinal, member) in [(1, "P"), (2, "PI"), (3, "PD"), (4, "PID")] {
        let mut head = pid.clone();
        head.blocks[at].params[0].1 = Value::Enum {
            class: controller,
            ordinal,
        };
        let diff = support::json(&support::loaded::compare_topologies(&pid, &head));
        let expected = if member == "PI" {
            json!([])
        } else {
            json!([{ "kind": "reparameterized", "subject": "pid.controllerType",
                     "old": { "member": "SimpleController.PI" },
                     "new": { "member": format!("SimpleController.{member}") } }])
        };
        assert_eq!(diff["graph"], expected, "{member}");
    }
    // Only a qualified member is admitted (issue #734, ruling R2): a string, even one naming a
    // member, an ordinal and an ordinal outside the enumeration are no block this level can wire
    // (before, `x` loaded and ran as the default controller and `PD` ran as PD).
    for refused in [
        Value::String("x".into()),
        Value::String("PD".into()),
        Value::String("Buildings.Controls.OBC.CDL.Types.SimpleController.PD".into()),
        Value::Integer(3),
        Value::Enum {
            class: controller,
            ordinal: 0,
        },
        Value::Enum {
            class: controller,
            ordinal: 5,
        },
    ] {
        let mut head = pid.clone();
        head.blocks[at].params[0].1 = refused;
        let error = support::loaded::refusal(&pid, &head);
        assert_eq!(error.code(), "unresolved-ports");
    }
}
