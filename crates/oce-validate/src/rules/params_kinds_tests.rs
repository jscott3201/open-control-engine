//! The load gate refuses every parameter whose value its catalog type does not admit (issue
//! #734), through [`crate::validate_block_parameters`], the load gate's parameter checks on one
//! block.

use std::sync::Arc;

use oce_blocks::{ParamNameTemplate, catalog, class_parameters};
use oce_diag::{DiagCode, Diagnostic, Severity};
use oce_model::{EnumClassId, ParamTable, Value, ValueType};

use crate::validate_block_parameters;

fn table(entries: &[(&str, Value)]) -> ParamTable {
    ParamTable {
        values: entries
            .iter()
            .map(|(name, value)| (Arc::from(*name), value.clone()))
            .collect(),
    }
}

/// The error diagnostics the gate refuses `entries` on `class_path` with (empty when it admits
/// them).
fn refusal(class_path: &str, entries: &[(&str, Value)]) -> Vec<Diagnostic> {
    match validate_block_parameters(class_path, &table(entries)) {
        Ok(_) => Vec::new(),
        Err(error) => error.diagnostics,
    }
}

fn kind_mismatch(name: &str, class_path: &str, declared: &str, got: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::ParameterKindMismatch,
        format!("parameter `{name}` on block `{class_path}` must have kind {declared}; got {got}"),
    )
    .with_subject("block#0".to_owned())
}

const PULSE: &str = "CDL.Integers.Sources.Pulse";

#[test]
fn a_real_on_the_pulse_amplitude_is_refused_where_it_ran_on_the_default() {
    // The issue's document: `period = 4.0`, `amplitude = period`. Before this rule the table
    // loaded clean and the block output the default amplitude 1.
    for amplitude in [Value::Real(4.0), Value::Real(-0.0), Value::Real(2.5)] {
        assert_eq!(
            refusal(
                PULSE,
                &[("period", Value::Real(4.0)), ("amplitude", amplitude)]
            ),
            vec![kind_mismatch("amplitude", PULSE, "Integer", "Real")]
        );
    }
    for (name, value, got) in [
        ("offset", Value::Boolean(true), "Boolean"),
        ("offset", Value::String(Arc::from("2")), "String"),
        ("amplitude", Value::Real(9.223_372_036_854_776e18), "Real"),
    ] {
        assert_eq!(
            refusal(PULSE, &[("period", Value::Real(4.0)), (name, value)]),
            vec![kind_mismatch(name, PULSE, "Integer", got)]
        );
    }
    // Its neighbours load: Integers on the Integer parameters, an Integer widened on `period`.
    assert_eq!(
        refusal(
            PULSE,
            &[
                ("period", Value::Integer(4)),
                ("amplitude", Value::Integer(4)),
                ("offset", Value::Integer(-2)),
                ("width", Value::Real(0.5)),
            ]
        ),
        Vec::new()
    );
}

/// A value of `value_type` for a required parameter of a baseline table.
fn baseline_value(value_type: ValueType) -> Value {
    match value_type {
        // Above the 2x2 table's last time stamp (a required period) and below pi/2 (a latitude).
        ValueType::Real => Value::Real(1.5),
        ValueType::Integer => Value::Integer(1),
        ValueType::Boolean => Value::Boolean(false),
        ValueType::String => Value::String(Arc::from("x")),
        ValueType::Enum(class) => Value::Enum { class, ordinal: 1 },
    }
}

/// A table the gate admits for `class_path`: each required parameter bound to a value of its
/// kind (a required table to a 2x2 table with increasing times). Asserted, so a class this
/// cannot satisfy fails here rather than weakening the sweep.
fn baseline(class_path: &str) -> Vec<(String, Value)> {
    let mut entries: Vec<(String, Value)> = Vec::new();
    for parameter in class_parameters(class_path).expect("registered") {
        if parameter.default != oce_blocks::DefaultSource::Required {
            continue;
        }
        let template = ParamNameTemplate::parse(parameter.name);
        match template.rank() {
            0 => entries.push((
                parameter.name.to_owned(),
                baseline_value(parameter.value_type),
            )),
            2 => {
                for (row, col, value) in [(1, 1, 0.0), (1, 2, 0.0), (2, 1, 1.0), (2, 2, 1.0)] {
                    let name = template.element_name(&[row, col]).expect("indices");
                    entries.push((name, Value::Real(value)));
                }
            }
            _ => {}
        }
    }
    let borrowed: Vec<(&str, Value)> = entries
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect();
    assert_eq!(
        refusal(class_path, &borrowed),
        Vec::new(),
        "{class_path} baseline"
    );
    entries
}

#[test]
fn every_parameter_of_every_class_is_refused_for_every_wrong_kind() {
    let mut cells = 0usize;
    for entry in catalog() {
        let base = baseline(entry.class_path);
        for parameter in class_parameters(entry.class_path).expect("registered") {
            let template = ParamNameTemplate::parse(parameter.name);
            let name = match template.rank() {
                0 => parameter.name.to_owned(),
                1 => template.element_name(&[1]).expect("index"),
                _ => template.element_name(&[1, 1]).expect("indices"),
            };
            let own_enum = match parameter.value_type {
                ValueType::Enum(class) => class,
                _ => EnumClassId::SMOOTHNESS,
            };
            let wrong: Vec<Value> = [
                Value::Real(2.0),
                Value::Integer(2),
                Value::Boolean(true),
                Value::String(Arc::from("x")),
                Value::Enum {
                    class: if own_enum == EnumClassId::SMOOTHNESS {
                        EnumClassId::EXTRAPOLATION
                    } else {
                        EnumClassId::SMOOTHNESS
                    },
                    ordinal: 1,
                },
            ]
            .into_iter()
            .filter(|value| {
                oce_blocks::parameter_kind_fault(entry.class_path, &name, value).is_some_and(
                    |fault| matches!(fault, oce_blocks::ParameterKindFault::WrongKind { .. }),
                )
            })
            .collect();
            assert!(!wrong.is_empty(), "{} {name}", entry.class_path);
            for value in wrong {
                // The baseline with this one parameter replaced (or added).
                let mut entries: Vec<(&str, Value)> = base
                    .iter()
                    .filter(|(bound, _)| *bound != name)
                    .map(|(bound, bound_value)| (bound.as_str(), bound_value.clone()))
                    .collect();
                entries.push((&name, value.clone()));
                let diags = refusal(entry.class_path, &entries);
                // Refused naming the parameter: by this rule, or by the class rule that judges
                // its kind (which keeps its own code and wording).
                let named = format!("`{name}`");
                assert!(
                    diags.iter().any(|diag| matches!(
                        diag.code,
                        DiagCode::ParameterKindMismatch | DiagCode::ParameterOutOfRange
                    ) && diag.message.contains(&named)),
                    "{} {name} = {value:?} was not refused for its kind: {diags:?}",
                    entry.class_path
                );
                cells += 1;
            }
        }
    }
    // 136 classes; on the base of this change 243 of these cells loaded on a default.
    assert!(cells > 600, "{cells}");
}

#[test]
fn an_integer_past_two_to_the_fifty_third_on_a_real_parameter_is_out_of_range() {
    let class = "CDL.Reals.Sources.Constant";
    let diags = refusal(class, &[("k", Value::Integer((1 << 53) + 1))]);
    assert_eq!(
        diags,
        vec![
            Diagnostic::error(
                DiagCode::ParameterOutOfRange,
                "parameter `k` on block `CDL.Reals.Sources.Constant` binds the Integer \
                 9007199254740993 to a Real parameter; an Integer above 2^53 in magnitude is not \
                 exact as a Real"
            )
            .with_subject("block#0".to_owned())
        ]
    );
    assert_eq!(diags[0].severity, Severity::Error);
    assert_eq!(
        refusal(class, &[("k", Value::Integer(1 << 53))]),
        Vec::new()
    );
}

/// One enumeration parameter: its class, its name, its enumeration and the other parameters a
/// table the gate admits needs beside it.
type EnumCase<'a> = (&'a str, &'a str, EnumClassId, &'a [(&'a str, Value)]);

#[test]
fn an_enumeration_parameter_admits_a_qualified_member_only_and_reports_each_refusal_once() {
    // Ruling R2 (2026-11-21): no ordinal, no string, no other enumeration's member, on every
    // enumeration parameter, whatever rules the class has for it, each reported once.
    let time_table_rows = [
        ("table_1_1", Value::Real(0.0)),
        ("table_1_2", Value::Real(0.0)),
        ("table_2_1", Value::Real(1.0)),
        ("table_2_2", Value::Real(1.0)),
    ];
    let cases: [EnumCase<'_>; 4] = [
        // No member rule.
        (
            "CDL.Reals.PID",
            "controllerType",
            EnumClassId::SIMPLE_CONTROLLER,
            &[],
        ),
        // A member rule only.
        (
            "CDL.Reals.Sources.TimeTable",
            "smoothness",
            EnumClassId::SMOOTHNESS,
            &time_table_rows,
        ),
        (
            "CDL.Reals.Sources.TimeTable",
            "extrapolation",
            EnumClassId::EXTRAPOLATION,
            &time_table_rows,
        ),
        // A `Required` kind and a member rule.
        (
            "CDL.Reals.Sources.CalendarTime",
            "zerTim",
            EnumClassId::ZERO_TIME,
            &[],
        ),
    ];
    for (class, name, own, base) in cases {
        let with = |value: Value| {
            let mut entries = base.to_vec();
            entries.push((name, value));
            refusal(class, &entries)
        };
        assert_eq!(
            with(Value::Enum {
                class: own,
                ordinal: 2
            }),
            Vec::new(),
            "{class} {name}"
        );
        let other = if own == EnumClassId::SMOOTHNESS {
            EnumClassId::SIMPLE_CONTROLLER
        } else {
            EnumClassId::SMOOTHNESS
        };
        let declared = format!("{:?}", ValueType::Enum(own));
        for (value, got) in [
            (Value::Integer(2), "Integer"),
            (Value::Integer(99), "Integer"),
            (Value::String(Arc::from("PD")), "String"),
            (Value::String(Arc::from(".PD")), "String"),
            (Value::String(Arc::from("Foo.Bar.PD")), "String"),
            (
                Value::String(Arc::from("Buildings.Controls.OBC.CDL.Types.Smoothness.PD")),
                "String",
            ),
            (
                Value::String(Arc::from(
                    "Buildings.Controls.OBC.CDL.Types.ZeroTime.NY2017",
                )),
                "String",
            ),
            (
                Value::Enum {
                    class: other,
                    ordinal: 2,
                },
                "Enum",
            ),
            (
                Value::Enum {
                    class: other,
                    ordinal: 1,
                },
                "Enum",
            ),
            (Value::Real(2.0), "Real"),
            (Value::Boolean(true), "Boolean"),
        ] {
            assert_eq!(
                with(value.clone()),
                vec![kind_mismatch(name, class, &declared, got)],
                "{class} {name} = {value:?}"
            );
        }
    }
}

#[test]
fn an_enumeration_value_of_another_class_is_the_wrong_kind() {
    // Before, `EnumMembers` checked only the ordinal, and the reader then fell back to its
    // default: `LastTwoPoints` (ordinal 2) on `smoothness` ran as `LinearSegments`.
    let time_table = "CDL.Reals.Sources.TimeTable";
    let mut entries = vec![
        ("table_1_1", Value::Real(0.0)),
        ("table_1_2", Value::Real(0.0)),
        ("table_2_1", Value::Real(1.0)),
        ("table_2_2", Value::Real(1.0)),
    ];
    entries.push((
        "smoothness",
        Value::Enum {
            class: EnumClassId::EXTRAPOLATION,
            ordinal: 2,
        },
    ));
    assert_eq!(
        refusal(time_table, &entries),
        vec![kind_mismatch(
            "smoothness",
            time_table,
            &format!("{:?}", ValueType::Enum(EnumClassId::SMOOTHNESS)),
            "Enum"
        )]
    );
}

#[test]
fn a_parameter_a_class_rule_types_keeps_that_rules_finding() {
    // `Hysteresis.uLow` is required as a Real: the `Required` rule refuses a Boolean itself (and
    // the ordering rule over `uLow` and `uHigh` reports it too, as before). The kind rule adds
    // nothing, so the refusal is what it was.
    let class = "CDL.Reals.Hysteresis";
    let diags = refusal(
        class,
        &[("uLow", Value::Boolean(true)), ("uHigh", Value::Real(1.0))],
    );
    assert_eq!(diags.len(), 2, "{diags:?}");
    assert_eq!(diags[0], kind_mismatch("uLow", class, "Real", "Boolean"));
    assert!(
        diags[1].message.contains("must both be numeric"),
        "{diags:?}"
    );
    // A required Integer given a Real keeps the `Required` rule's finding.
    let constant = "CDL.Integers.Sources.Constant";
    assert_eq!(
        refusal(constant, &[("k", Value::Real(2.0))]),
        vec![kind_mismatch("k", constant, "Integer", "Real")]
    );
    // But an Integer past 2^53 on a ruled Real parameter is added: no class rule checks it.
    let diags = refusal(
        class,
        &[
            ("uLow", Value::Integer(i64::MIN)),
            ("uHigh", Value::Real(1.0)),
        ],
    );
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(
        diags[0].message.contains("not exact as a Real"),
        "{diags:?}"
    );
}

#[test]
fn a_parameter_typed_by_its_default_alone_is_judged_by_the_kind_rule() {
    // The comparators' threshold, the PID limits (named only by a rule over both of them, which
    // reads neither unless both are present and numeric) and the pulse offset have no rule that
    // judges their kind: before this rule a wrong kind loaded clean and ran on the default.
    for (class, name, value, declared, got) in [
        (
            "CDL.Integers.GreaterThreshold",
            "t",
            Value::Real(2.0),
            "Integer",
            "Real",
        ),
        (
            "CDL.Reals.GreaterThreshold",
            "t",
            Value::Boolean(true),
            "Real",
            "Boolean",
        ),
        (
            "CDL.Reals.PID",
            "yMax",
            Value::String(Arc::from("1")),
            "Real",
            "String",
        ),
        (
            "CDL.Reals.PID",
            "reverseActing",
            Value::Integer(1),
            "Boolean",
            "Integer",
        ),
        (
            "CDL.Integers.OnCounter",
            "y_start",
            Value::Real(1.0),
            "Integer",
            "Real",
        ),
        (
            "CDL.Logical.Pre",
            "pre_u_start",
            Value::Integer(1),
            "Boolean",
            "Integer",
        ),
    ] {
        assert_eq!(
            refusal(class, &[(name, value)]),
            vec![kind_mismatch(name, class, declared, got)],
            "{class}"
        );
    }
}

#[test]
fn a_parameter_the_catalog_does_not_type_is_not_judged() {
    // An unregistered class and an undeclared name are outside the rule (and outside this issue).
    assert_eq!(
        refusal("CDL.Reals.NoSuchClass", &[("k", Value::Boolean(true))]),
        Vec::new()
    );
    assert_eq!(
        refusal(
            PULSE,
            &[
                ("period", Value::Real(4.0)),
                ("unknown", Value::Boolean(true))
            ]
        ),
        Vec::new()
    );
}
