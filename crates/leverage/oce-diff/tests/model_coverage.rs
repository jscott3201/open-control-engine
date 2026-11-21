//! The oracle for "nothing goes unreported": an empty diff means the two models are equal up to
//! blank lines, and every single-field edit of a model is reported in the section it belongs to.
//!
//! The reference is the model's own structural equality (`Sequence: PartialEq`, which compares a
//! Real by its bits and every text-only field), with the one thing the diff does not compare,
//! `Annotations::blank_line_before`, cleared on both sides. The edits are enumerated by hand, one
//! per field of every model type, so a field added to the authoring model that the diff does not
//! look at shows up here as an edit that changes the model and leaves the diff empty.

mod support;

use oce_author::{Bound, Literal, ParamValue, ParameterType, ProvenanceId, Sequence, SignalType};

use support::{block, boundary, compare, id, link, parse, real_parameter};

/// A model that holds every element the authoring model has, each at least twice.
const EVERY_ELEMENT: &str = "\
# A header comment.
sequence every_element
derived_from cxf:fnv1a128:0123abcd

# The first parameter.
param gain : Real = 2.0
param hold : Real = 1 h
param mode : SimpleController = SimpleController.PI

# The supply temperature.
in TSup : Real @unit K @quantity ThermodynamicTemperature @displayUnit degC @min 250 @max 350.5 @brick Supply_Air_Temperature_Sensor @s223 Sensor @qudt Temperature @haystack air @haystack temp @name \"Supply\" @label \"supply air\" @provisional
in uEna : Boolean
# The command.
out yCmd : Real @unit 1
out yOn : Boolean

# The gain block.
block scale = CDL.Reals.MultiplyByParameter(k = gain)
block delay = CDL.Logical.TrueDelay(
  # How long.
  delayTime = hold,
  delayOnInit = true
)
block lim = CDL.Reals.Limiter(uMax = 1.0, uMin = 0)

# Into the gain.
TSup -> scale.u
scale.y -> lim.u
lim.y -> yCmd
uEna -> delay.u
delay.y -> yOn

# A trailing comment.
";

fn every_element() -> Sequence {
    parse(EVERY_ELEMENT)
}

/// `model` with every blank-line flag cleared: what the diff compares.
fn without_layout(model: &Sequence) -> Sequence {
    let mut model = model.clone();
    model.annotations.blank_line_before = false;
    for parameter in &mut model.params {
        parameter.annotations.blank_line_before = false;
    }
    for connector in model.inputs.iter_mut().chain(&mut model.outputs) {
        connector.annotations.blank_line_before = false;
    }
    for block in &mut model.blocks {
        block.annotations.blank_line_before = false;
        for assignment in &mut block.assignments {
            assignment.annotations.blank_line_before = false;
        }
    }
    for connection in &mut model.connections {
        connection.annotations.blank_line_before = false;
    }
    model
}

type Edit = fn(&mut Sequence);

/// One edit per field of every model type, with the one section it must be reported in.
fn edits() -> Vec<(&'static str, &'static str, Edit)> {
    vec![
        ("sequence name", "metadata", |m| m.name = id("another")),
        ("derived_from changed", "metadata", |m| {
            m.derived_from = Some(ProvenanceId::unchecked("cxf:fnv1a128:ffff"));
        }),
        ("derived_from dropped", "metadata", |m| {
            m.derived_from = None
        }),
        ("derived_from the word none", "metadata", |m| {
            m.derived_from = Some(ProvenanceId::unchecked("none"));
        }),
        ("header comment", "metadata", |m| {
            m.annotations.comments.push("more".into())
        }),
        ("header comment dropped", "metadata", |m| {
            m.annotations.comments.clear()
        }),
        ("trailing comment", "metadata", |m| {
            m.trailing_comments[0].push('!')
        }),
        // Sequence parameters.
        ("parameter added", "parameters", |m| {
            m.params.push(real_parameter("extra", 0.0))
        }),
        ("parameter removed", "parameters", |m| {
            m.params.remove(0);
        }),
        ("parameter renamed", "parameters", |m| {
            m.params[0].name = id("gain2")
        }),
        ("parameter type", "parameters", |m| {
            m.params[0].value_type = ParameterType::Integer
        }),
        ("parameter enum type", "parameters", |m| {
            m.params[2].value_type = ParameterType::Enum("Other".into());
        }),
        ("parameter value", "parameters", |m| {
            m.params[0].value = Literal::Real(2.5)
        }),
        ("parameter value kind", "metadata", |m| {
            m.params[0].value = Literal::Integer(2)
        }),
        ("parameter member", "parameters", |m| {
            m.params[2].value = Literal::Member(oce_author::MemberRef::parse("SimpleController.P"));
        }),
        ("parameter suffix", "metadata", |m| {
            m.params[1].suffix = Some("min".into())
        }),
        ("parameter scaled_from", "metadata", |m| {
            m.params[1].scaled_from = Some(Literal::Real(1.0));
        }),
        ("parameter comment", "metadata", |m| {
            m.params[0].annotations.comments.clear()
        }),
        ("parameters reordered", "parameters", |m| {
            m.params.swap(0, 1)
        }),
        // Boundaries.
        ("input added", "interface", |m| {
            m.inputs.push(boundary("uNew", SignalType::Real))
        }),
        ("input removed", "interface", |m| {
            m.inputs.pop();
        }),
        ("output removed", "interface", |m| {
            m.outputs.pop();
        }),
        ("input renamed", "interface", |m| {
            m.inputs[1].name = id("uOther")
        }),
        ("input type", "interface", |m| {
            m.inputs[0].value_type = SignalType::Integer
        }),
        ("input unit", "interface", |m| {
            m.inputs[0].semantic.unit = None
        }),
        ("output unit", "interface", |m| {
            m.outputs[0].semantic.unit = Some("K".into())
        }),
        ("input quantity", "interface", |m| {
            m.inputs[0].quantity = Some("Other".into())
        }),
        ("input display unit", "interface", |m| {
            m.inputs[0].display_unit = None
        }),
        ("input min", "interface", |m| {
            m.inputs[0].min = Some(Bound::Integer(251))
        }),
        ("input min dropped", "interface", |m| m.inputs[0].min = None),
        ("input min respelled", "metadata", |m| {
            m.inputs[0].min = Some(Bound::Real(250.0))
        }),
        ("input max", "interface", |m| {
            m.inputs[0].max = Some(Bound::Real(350.25))
        }),
        ("input max added", "interface", |m| {
            m.inputs[1].max = Some(Bound::Integer(1))
        }),
        ("input name field", "metadata", |m| {
            m.inputs[0].semantic.name = None
        }),
        ("input label", "metadata", |m| {
            m.inputs[0].semantic.label = Some("x".into())
        }),
        ("input brick", "metadata", |m| {
            m.inputs[0].semantic.brick = Some("x".into())
        }),
        ("input s223", "metadata", |m| {
            m.inputs[0].semantic.s223 = None
        }),
        ("input qudt", "metadata", |m| {
            m.inputs[0].semantic.qudt = Some("x".into())
        }),
        ("input haystack order", "metadata", |m| {
            m.inputs[0].semantic.haystack.swap(0, 1)
        }),
        ("input haystack tag", "metadata", |m| {
            m.inputs[0].semantic.haystack.push("x".into())
        }),
        ("input provisional", "metadata", |m| {
            m.inputs[0].semantic.provisional = false
        }),
        ("input comment", "metadata", |m| {
            m.inputs[0].annotations.comments[0].push('!')
        }),
        ("output comment", "metadata", |m| {
            m.outputs[0].annotations.comments.clear()
        }),
        ("inputs reordered", "interface", |m| m.inputs.swap(0, 1)),
        ("outputs reordered", "interface", |m| m.outputs.swap(0, 1)),
        // Blocks and assignments.
        ("block added", "graph", |m| {
            m.blocks.push(block("extra", "CDL.Reals.Abs", &[]))
        }),
        ("block removed", "graph", |m| {
            m.blocks.remove(2);
        }),
        ("block renamed", "graph", |m| m.blocks[2].name = id("limit")),
        ("block class", "graph", |m| {
            m.blocks[2].class_path = "CDL.Reals.Abs".into()
        }),
        ("block comment", "metadata", |m| {
            m.blocks[0].annotations.comments.clear()
        }),
        ("blocks reordered", "graph", |m| m.blocks.swap(0, 2)),
        ("assignment value", "graph", |m| {
            m.blocks[2].assignments[0].value = ParamValue::Literal(Literal::Real(1.5));
        }),
        ("assignment negative zero", "graph", |m| {
            m.blocks[2].assignments[1].value = ParamValue::Literal(Literal::Real(-0.0));
        }),
        ("assignment respelled", "metadata", |m| {
            m.blocks[2].assignments[1].value = ParamValue::Literal(Literal::Real(0.0));
        }),
        ("assignment reference", "graph", |m| {
            m.blocks[0].assignments[0].value = ParamValue::Ref(id("hold"));
        }),
        ("assignment literal for a reference", "graph", |m| {
            m.blocks[0].assignments[0].value = ParamValue::Literal(Literal::Real(2.0));
        }),
        ("assignment renamed", "graph", |m| {
            m.blocks[2].assignments[0].name = id("uTop")
        }),
        ("assignment added", "graph", |m| {
            let extra = block("x", "x", &[("extra", Literal::Boolean(true))]);
            m.blocks[0].assignments.extend(extra.assignments);
        }),
        ("assignment removed", "graph", |m| {
            m.blocks[1].assignments.pop();
        }),
        ("assignment suffix", "metadata", |m| {
            m.blocks[2].assignments[0].suffix = Some("K".into());
        }),
        ("assignment scaled_from", "metadata", |m| {
            m.blocks[2].assignments[0].scaled_from = Some(Literal::Integer(1));
        }),
        ("assignment comment", "metadata", |m| {
            m.blocks[1].assignments[0].annotations.comments.clear();
        }),
        ("assignments reordered", "graph", |m| {
            m.blocks[2].assignments.swap(0, 1)
        }),
        // Connections.
        ("connection added", "graph", |m| {
            m.connections.push(link("TSup", "lim.extra"))
        }),
        ("connection removed", "graph", |m| {
            m.connections.remove(1);
        }),
        ("connection repeated", "graph", |m| {
            let again = m.connections[1].clone();
            m.connections.push(again);
        }),
        ("connection source", "graph", |m| {
            m.connections[1] = link("TSup", "lim.u")
        }),
        ("connection target", "graph", |m| {
            m.connections[1] = link("scale.y", "lim.v")
        }),
        ("connection comment", "metadata", |m| {
            m.connections[0].annotations.comments.clear();
        }),
        ("connections reordered", "graph", |m| {
            m.connections.swap(0, 4)
        }),
    ]
}

#[test]
fn every_single_field_edit_is_reported_in_its_section() {
    let base = every_element();
    for (name, section, edit) in edits() {
        let mut head = base.clone();
        edit(&mut head);
        assert_ne!(
            without_layout(&base),
            without_layout(&head),
            "{name} edits the model"
        );
        for (old, new) in [(&base, &head), (&head, &base)] {
            let diff = compare(old, new);
            assert!(!diff.summary.equal, "{name}: the diff is empty");
            let sizes = [
                ("parameters", diff.parameters.len()),
                ("interface", diff.interface.len()),
                ("graph", diff.graph.len()),
                ("metadata", diff.metadata.len()),
            ];
            for (other, size) in sizes {
                assert_eq!(
                    size > 0,
                    other == section,
                    "{name}: expected only `{section}`, found {size} in `{other}`\n{}",
                    diff.to_json()
                );
            }
            assert_eq!(diff.summary.graph_equal, section != "graph", "{name}");
            assert_eq!(
                diff.summary.interface_equal,
                section != "interface",
                "{name}"
            );
            assert_eq!(
                diff.summary.parameters_equal,
                Some(section != "parameters"),
                "{name}"
            );
            assert_eq!(
                diff.summary.metadata_equal,
                Some(section != "metadata"),
                "{name}"
            );
        }
    }
}

#[test]
fn a_diff_is_empty_exactly_when_the_models_are_equal_up_to_blank_lines() {
    // Every edited model against every other, two edits apart: the diff must agree with
    // structural equality on each pair, in both directions.
    let base = every_element();
    let mutants: Vec<(&str, Sequence)> = edits()
        .into_iter()
        .map(|(name, _, edit)| {
            let mut head = base.clone();
            edit(&mut head);
            (name, head)
        })
        .collect();
    let (mut equal_pairs, mut unequal_pairs) = (0, 0);
    for (first_name, first) in &mutants {
        for (second_name, second) in &mutants {
            let equal = without_layout(first) == without_layout(second);
            if equal {
                equal_pairs += 1;
            } else {
                unequal_pairs += 1;
            }
            assert_eq!(
                compare(first, second).summary.equal,
                equal,
                "{first_name} against {second_name}"
            );
        }
    }
    assert_eq!(
        equal_pairs,
        mutants.len(),
        "each edit gives a model of its own"
    );
    assert_eq!(unequal_pairs, mutants.len() * (mutants.len() - 1));
}

#[test]
fn a_blank_line_flag_is_the_one_field_that_is_not_compared() {
    let base = every_element();
    let mut head = base.clone();
    head.annotations.blank_line_before = !head.annotations.blank_line_before;
    head.params[0].annotations.blank_line_before ^= true;
    head.inputs[0].annotations.blank_line_before ^= true;
    head.outputs[0].annotations.blank_line_before ^= true;
    head.blocks[0].annotations.blank_line_before ^= true;
    head.blocks[1].assignments[0].annotations.blank_line_before ^= true;
    head.connections[0].annotations.blank_line_before ^= true;
    assert_ne!(base, head);
    assert_eq!(without_layout(&base), without_layout(&head));
    assert!(compare(&base, &head).summary.equal);
}
