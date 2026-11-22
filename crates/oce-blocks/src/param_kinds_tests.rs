//! What a parameter's catalog type admits, cell by cell and over the whole catalog (issues #734,
//! #735). `oce-validate`'s `params_kinds` tests hold the load gate to the same table, and
//! `oce-api`'s `parameter_kinds` tests run the admitted values end to end.

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use oce_model::{EnumClassId, Value, ValueType, enum_descriptor};

use crate::registry::all_entries;
use crate::{
    MAX_EXACT_INTEGER_AS_REAL, ParamNameTemplate, ParameterKindFault, admits_parameter_value,
    class_parameters, exact_real, parameter_kind_fault,
};

const TWO_POW_53: i64 = 1 << 53;

fn string(text: &str) -> Value {
    Value::String(Arc::from(text))
}

/// One value of every kind, the enumeration one of `class`.
fn one_of_each_kind(class: EnumClassId) -> Vec<Value> {
    vec![
        Value::Real(2.0),
        Value::Integer(2),
        Value::Boolean(true),
        string("x"),
        Value::Enum { class, ordinal: 1 },
    ]
}

#[test]
fn a_real_parameter_admits_a_real_and_an_integer_exact_as_a_real_only() {
    for value in [
        Value::Real(2.0),
        Value::Real(2.5),
        Value::Real(-0.0),
        Value::Real(f64::MAX),
        Value::Integer(0),
        Value::Integer(-7),
        Value::Integer(TWO_POW_53),
        Value::Integer(-TWO_POW_53),
    ] {
        assert_eq!(
            admits_parameter_value(ValueType::Real, &value),
            Ok(()),
            "{value:?}"
        );
    }
    for integer in [TWO_POW_53 + 1, -(TWO_POW_53 + 1), i64::MAX, i64::MIN] {
        assert_eq!(
            admits_parameter_value(ValueType::Real, &Value::Integer(integer)),
            Err(ParameterKindFault::InexactInteger),
            "{integer}"
        );
    }
    for value in [
        Value::Boolean(false),
        string("2"),
        Value::Enum {
            class: EnumClassId::SMOOTHNESS,
            ordinal: 1,
        },
    ] {
        assert_eq!(
            admits_parameter_value(ValueType::Real, &value),
            Err(ParameterKindFault::WrongKind {
                declared: ValueType::Real
            }),
            "{value:?}"
        );
    }
}

#[test]
fn an_integer_parameter_admits_an_integer_and_never_narrows_a_real() {
    for integer in [0, 7, -7, TWO_POW_53 + 1, i64::MAX, i64::MIN] {
        assert_eq!(
            admits_parameter_value(ValueType::Integer, &Value::Integer(integer)),
            Ok(())
        );
    }
    // Integral Reals included: `2.0`, `-0.0`, 2^53 as a Real are not Integers.
    for value in [
        Value::Real(2.0),
        Value::Real(-0.0),
        Value::Real(0.0),
        Value::Real(TWO_POW_53 as f64),
        Value::Real(2.5),
        Value::Boolean(true),
        string("2"),
        Value::Enum {
            class: EnumClassId::SIMPLE_CONTROLLER,
            ordinal: 2,
        },
    ] {
        assert_eq!(
            admits_parameter_value(ValueType::Integer, &value),
            Err(ParameterKindFault::WrongKind {
                declared: ValueType::Integer
            }),
            "{value:?}"
        );
    }
}

#[test]
fn a_boolean_or_string_parameter_admits_its_own_kind_only() {
    for (declared, admitted) in [
        (ValueType::Boolean, Value::Boolean(false)),
        (ValueType::String, string("")),
    ] {
        assert_eq!(admits_parameter_value(declared, &admitted), Ok(()));
        for value in one_of_each_kind(EnumClassId::SMOOTHNESS) {
            if value.value_type() == declared {
                continue;
            }
            assert_eq!(
                admits_parameter_value(declared, &value),
                Err(ParameterKindFault::WrongKind { declared }),
                "{declared:?} {value:?}"
            );
        }
    }
}

#[test]
fn an_enumeration_parameter_admits_each_member_in_each_of_its_three_spellings() {
    for class in [
        EnumClassId::SIMPLE_CONTROLLER,
        EnumClassId::SMOOTHNESS,
        EnumClassId::EXTRAPOLATION,
        EnumClassId::ZERO_TIME,
    ] {
        let declared = ValueType::Enum(class);
        let descriptor = enum_descriptor(class).expect("a CDL enumeration");
        let count = descriptor.members.len();
        for (index, member) in descriptor.members.iter().enumerate() {
            let ordinal = u32::try_from(index + 1).expect("small");
            for value in [
                Value::Enum { class, ordinal },
                Value::Integer(i64::from(ordinal)),
                string(member),
                Value::String(Arc::from(alloc::format!(
                    "Buildings.Controls.OBC.{}.{member}",
                    descriptor.class_path
                ))),
            ] {
                assert_eq!(
                    admits_parameter_value(declared, &value),
                    Ok(()),
                    "{value:?}"
                );
            }
        }
        let unknown = Err(ParameterKindFault::UnknownMember { declared: class });
        let past = u32::try_from(count + 1).expect("small");
        for value in [
            Value::Enum { class, ordinal: 0 },
            Value::Enum {
                class,
                ordinal: past,
            },
            Value::Integer(0),
            Value::Integer(-1),
            Value::Integer(i64::from(past)),
            Value::Integer(i64::MIN),
            string(""),
            string("Nope"),
            string("Types.Nope"),
        ] {
            assert_eq!(
                admits_parameter_value(declared, &value),
                unknown,
                "{value:?}"
            );
        }
        // A member of another enumeration is the wrong kind, whatever its ordinal.
        let other = if class == EnumClassId::SMOOTHNESS {
            EnumClassId::EXTRAPOLATION
        } else {
            EnumClassId::SMOOTHNESS
        };
        for value in [
            Value::Enum {
                class: other,
                ordinal: 1,
            },
            Value::Real(1.0),
            Value::Boolean(true),
        ] {
            assert_eq!(
                admits_parameter_value(declared, &value),
                Err(ParameterKindFault::WrongKind { declared }),
                "{value:?}"
            );
        }
    }
}

#[test]
fn the_exact_bound_is_two_to_the_fifty_third_and_the_conversion_is_exact_below_it() {
    assert_eq!(MAX_EXACT_INTEGER_AS_REAL, 9_007_199_254_740_992);
    for integer in [0, 1, -1, TWO_POW_53 - 1, TWO_POW_53, -TWO_POW_53] {
        let real = exact_real(integer).expect("within the bound");
        assert_eq!(real as i64, integer);
    }
    assert_eq!(exact_real(0).map(f64::to_bits), Some(0.0f64.to_bits()));
    for integer in [TWO_POW_53 + 1, -(TWO_POW_53 + 1), i64::MAX, i64::MIN] {
        assert_eq!(exact_real(integer), None, "{integer}");
    }
}

/// Every name a resolved model binds for one catalog parameter: the scalar name, or array
/// element names at the first index and a multi-digit one.
fn bound_names(catalog_name: &str) -> Vec<alloc::string::String> {
    let template = ParamNameTemplate::parse(catalog_name);
    let indices: &[&[usize]] = match template.rank() {
        0 => &[&[]],
        1 => &[&[1], &[12]],
        _ => &[&[1, 1], &[3, 12]],
    };
    indices
        .iter()
        .map(|index| template.element_name(index).expect("valid indices"))
        .collect()
}

#[test]
fn every_parameter_of_every_class_refuses_every_wrong_kind_and_admits_its_own() {
    let mut refused = 0usize;
    let mut admitted = 0usize;
    for entry in all_entries() {
        for parameter in class_parameters(entry.class_path).expect("registered") {
            let declared = parameter.value_type;
            let own_enum = match declared {
                ValueType::Enum(class) => class,
                _ => EnumClassId::SMOOTHNESS,
            };
            for name in bound_names(parameter.name) {
                for value in one_of_each_kind(own_enum) {
                    let fault = parameter_kind_fault(entry.class_path, &name, &value);
                    let own_kind = value.value_type() == declared
                        || (declared == ValueType::Real
                            && value.value_type() == ValueType::Integer)
                        || (matches!(declared, ValueType::Enum(_))
                            && matches!(value, Value::Integer(_)));
                    if own_kind {
                        assert_eq!(fault, None, "{} {name} {value:?}", entry.class_path);
                        admitted += 1;
                    } else if let (ValueType::Enum(class), Value::String(_)) = (declared, &value) {
                        // A String is an enumeration stand-in; `x` names no member.
                        assert_eq!(
                            fault,
                            Some(ParameterKindFault::UnknownMember { declared: class }),
                            "{} {name}",
                            entry.class_path
                        );
                        refused += 1;
                    } else {
                        assert_eq!(
                            fault,
                            Some(ParameterKindFault::WrongKind { declared }),
                            "{} {name} {value:?}",
                            entry.class_path
                        );
                        refused += 1;
                    }
                }
            }
        }
    }
    assert!(refused > 500 && admitted > 300, "{refused} {admitted}");
}

#[test]
fn a_parameter_the_catalog_does_not_type_is_outside_the_rule() {
    for (class_path, name) in [
        ("CDL.Reals.NoSuchClass", "k"),
        ("", "k"),
        ("CDL.Integers.Sources.Pulse", "amplitud"),
        ("CDL.Reals.MultiSum", "k_0"),
    ] {
        for value in one_of_each_kind(EnumClassId::SMOOTHNESS) {
            assert_eq!(parameter_kind_fault(class_path, name, &value), None);
        }
    }
    // The issue's own cell: `Integers.Sources.Pulse.amplitude` given a Real.
    assert_eq!(
        parameter_kind_fault("CDL.Integers.Sources.Pulse", "amplitude", &Value::Real(4.0)),
        Some(ParameterKindFault::WrongKind {
            declared: ValueType::Integer
        })
    );
    assert_eq!(
        parameter_kind_fault("CDL.Integers.Sources.Pulse", "period", &Value::Integer(4)),
        None
    );
}
