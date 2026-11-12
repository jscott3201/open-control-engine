//! The joined view's range rendering over the catalog's own rules (design note test plan,
//! "Serving"): one pinned rendering for every rule kind the catalog uses, every line plain text
//! an agent can read, and every kind of line held against what the engine actually accepts,
//! refuses and warns about. The rendering must never say `>=` where the engine checks `>`.

use std::collections::{BTreeMap, BTreeSet};

use oce_api::{
    CatalogRule, ClassPortsError, DiagCode, Engine, Value, catalog, class_ports, enumeration_types,
};
use oce_vocab::{class_vocab, parameter_base_name, parameter_range, vocabulary};

use crate::engine_agreement::one_block;

/// The rule's variant name, as `Debug` spells it.
fn kind(rule: &CatalogRule) -> String {
    let debug = format!("{rule:?}");
    debug
        .split(|c: char| !c.is_ascii_alphanumeric())
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// The range of `parameter` of `class_path` in the served view.
fn range(class_path: &str, parameter: &str) -> Vec<String> {
    let class = class_vocab(class_path).unwrap_or_else(|| panic!("{class_path} is authorable"));
    class
        .parameters
        .iter()
        .find(|found| found.name == parameter)
        .unwrap_or_else(|| panic!("{class_path} has no parameter {parameter}"))
        .range
        .clone()
}

/// One example from the catalog per rule kind that renders a line: the class, the parameter
/// and its whole range.
const RENDERED: &[(&str, &str, &str, &[&str])] = &[
    (
        "RealFinite",
        "CDL.Logical.Sources.SampleTrigger",
        "shift",
        &["shift is finite"],
    ),
    (
        "RealGreaterThan",
        "CDL.Reals.MovingAverage",
        "delta",
        &["delta > 0.0"],
    ),
    (
        "RealFiniteGreaterThan",
        "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
        "pAtm",
        &["pAtm > 0.0 and finite"],
    ),
    (
        "RealGreaterOrEqual",
        "CDL.Reals.PID",
        "Ti",
        &["Ti >= 1e-13"],
    ),
    (
        "RealLessOrEqualConstant",
        "CDL.Reals.Sources.Pulse",
        "width",
        &["width >= 1e-37", "width <= 1.0"],
    ),
    (
        "IntegerGreaterOrEqual",
        "CDL.Reals.Sources.CalendarTime",
        "yearRef",
        &["yearRef >= 2010", "yearRef <= 2031"],
    ),
    (
        "IntegerLessOrEqualConstant",
        "CDL.Reals.MultiSum",
        "nin",
        &["nin >= 0", "nin <= 1048576"],
    ),
    (
        "RealLessOrEqual",
        "CDL.Reals.Hysteresis",
        "uLow",
        &["uLow <= uHigh", "uLow == uHigh warns"],
    ),
    (
        "RealEqualWarning",
        "CDL.Reals.PID",
        "yMax",
        &["yMin <= yMax", "yMin == yMax warns"],
    ),
    (
        "RealLessOrEqualWarning",
        "CDL.Logical.Proof",
        "feedbackDelay",
        &["debounce > feedbackDelay warns"],
    ),
    (
        "RealGreaterOrEqualScaledWarning",
        "CDL.Logical.VariablePulse",
        "minTruFalHol",
        &["minTruFalHol >= 1e-37", "period < 2.0 * minTruFalHol warns"],
    ),
    (
        "RealTimesIntegerInclusiveRange",
        "CDL.Integers.Stage",
        "h",
        &["0.001 <= h * n <= 0.5"],
    ),
    (
        "IntegerProductLessOrEqualConstant",
        "CDL.Reals.MatrixGain",
        "nout",
        &["nout >= 0", "nout <= 1048576", "nout * nin <= 1048576"],
    ),
    (
        "IntegerArrayElementsInRange",
        "CDL.Routing.BooleanExtractSignal",
        "extract",
        &["1 <= extract_<i> <= nin"],
    ),
    (
        "BooleanArrayTrueCountEquals",
        "CDL.Routing.BooleanVectorFilter",
        "msk",
        &["the number of true msk_<i> == nout"],
    ),
];

/// One example per rule kind that renders nothing (type, shape, structure): a parameter whose
/// rules are all of such kinds, so its range is empty.
const UNRENDERED: &[(&str, &str, &str)] = &[
    ("Required", "CDL.Logical.TrueDelay", "delayTime"),
    ("Boolean", "CDL.Reals.MatrixMax", "rowMax"),
    ("Real", "CDL.Reals.Sources.Sin", "amplitude"),
    ("EnumMembers", "CDL.Reals.Sources.TimeTable", "smoothness"),
    ("Structural", "CDL.Logical.MultiAnd", "nin"),
    (
        "StructuralArrayElements",
        "CDL.Routing.BooleanVectorFilter",
        "msk",
    ),
    ("IntegerArrayElements", "CDL.Integers.MultiSum", "k"),
    ("RealArrayElements", "CDL.Reals.MultiSum", "k"),
    (
        "BooleanArrayElements",
        "CDL.Routing.BooleanVectorFilter",
        "msk",
    ),
    ("RealMatrixElements", "CDL.Reals.MatrixGain", "K"),
    ("TimeTableMatrix", "CDL.Reals.Sources.TimeTable", "table"),
    ("TimeTableOffset", "CDL.Reals.Sources.TimeTable", "offset"),
];

/// Whether a rule of `kind` names `parameter` in the class's catalog entry.
fn class_has_rule(class_path: &str, wanted: &str, parameter: &str) -> bool {
    let entry = catalog()
        .iter()
        .find(|entry| entry.class_path == class_path)
        .unwrap_or_else(|| panic!("{class_path} is a catalog class"));
    entry.param_rules.iter().any(|rule| {
        kind(rule) == wanted && format!("{rule:?}").contains(&format!("\"{parameter}\""))
    })
}

#[test]
fn every_rule_kind_the_catalog_uses_has_a_pinned_rendering() {
    for (rule, class_path, parameter, expected) in RENDERED {
        assert!(
            class_has_rule(class_path, rule, parameter),
            "{class_path} {parameter}: no {rule} rule"
        );
        assert_eq!(range(class_path, parameter), *expected, "{rule}");
    }
    for (rule, class_path, parameter) in UNRENDERED {
        assert!(
            class_has_rule(class_path, rule, parameter),
            "{class_path} {parameter}: no {rule} rule"
        );
    }
    // A parameter under unrendered kinds only has no range.
    for (class_path, parameter) in [
        ("CDL.Logical.TrueDelay", "delayTime"),
        ("CDL.Reals.MatrixMax", "rowMax"),
        ("CDL.Reals.Sources.Sin", "amplitude"),
        ("CDL.Reals.Sources.TimeTable", "smoothness"),
        ("CDL.Integers.MultiSum", "k"),
        ("CDL.Reals.MultiSum", "k"),
        ("CDL.Reals.MatrixGain", "K"),
        ("CDL.Reals.Sources.TimeTable", "table"),
        ("CDL.Reals.Sources.TimeTable", "offset"),
    ] {
        assert!(
            range(class_path, parameter).is_empty(),
            "{class_path} {parameter}"
        );
    }
    // The two tables are every kind the catalog uses, and nothing it does not.
    let pinned: BTreeSet<String> = RENDERED
        .iter()
        .map(|(rule, ..)| (*rule).to_owned())
        .chain(UNRENDERED.iter().map(|(rule, ..)| (*rule).to_owned()))
        .collect();
    let used: BTreeSet<String> = catalog()
        .iter()
        .flat_map(|entry| entry.param_rules.iter().map(kind))
        .collect();
    assert_eq!(pinned, used);
    assert_eq!(used.len(), 27);
}

#[test]
fn every_value_rule_of_every_class_reaches_a_parameter_of_that_class() {
    let unrendered: BTreeSet<&str> = UNRENDERED.iter().map(|(rule, ..)| *rule).collect();
    let (mut lines, mut ranged, mut classes) = (0, 0, 0);
    for entry in catalog() {
        let value_rules = entry
            .param_rules
            .iter()
            .filter(|rule| !unrendered.contains(kind(rule).as_str()))
            .count();
        // Every catalog parameter the rules could name, by its CDL name.
        let names: BTreeSet<&str> = entry
            .param_defaults
            .iter()
            .map(|parameter| parameter_base_name(parameter.name))
            .collect();
        let mut distinct = BTreeSet::new();
        for name in &names {
            let range = parameter_range(&entry.param_rules, name);
            let unique: BTreeSet<&String> = range.iter().collect();
            assert_eq!(unique.len(), range.len(), "{} {name}", entry.class_path);
            ranged += usize::from(!range.is_empty());
            lines += range.len();
            distinct.extend(range);
        }
        // A rule that named a parameter the class does not have would render for no one.
        assert_eq!(distinct.len(), value_rules, "{}", entry.class_path);
        classes += usize::from(value_rules > 0);
    }
    // Volume pins: a change that stops finding rules must not leave this vacuously green.
    // 176 lines: 133 one-parameter bounds, 3 element bounds and 20 relations listed twice.
    assert_eq!((classes, ranged, lines), (51, 101, 176));
}

#[test]
fn the_served_view_carries_the_catalogs_range_for_every_parameter() {
    let mut checked = 0;
    for class in vocabulary().classes() {
        let entry = catalog()
            .iter()
            .find(|entry| entry.class_path == class.class_path)
            .expect("a catalog class");
        for parameter in &class.parameters {
            assert_eq!(
                parameter.range,
                parameter_range(&entry.param_rules, parameter.name),
                "{} {}",
                class.class_path,
                parameter.name
            );
            for line in &parameter.range {
                assert!(line.is_ascii() && !line.is_empty(), "{line:?}");
                assert_eq!(line.trim(), line, "{line:?}");
                assert!(!line.chars().any(char::is_control), "{line:?}");
                assert!(line.len() <= 80, "{line:?}");
                assert!(line.contains(parameter.name), "{line:?}");
                checked += 1;
            }
        }
    }
    // The three reserved lowering identities have no rules, so the view has every line.
    assert_eq!(checked, 176);
}

/// Whether the class rules accept the table, by the same checks a load runs.
fn accepted(class_path: &str, table: &[(&str, Value)]) -> bool {
    match class_ports(class_path, table) {
        Ok(_) => true,
        Err(ClassPortsError::InvalidParameters { diagnostics, .. }) => {
            assert!(
                diagnostics.iter().any(|diagnostic| matches!(
                    diagnostic.code,
                    DiagCode::ParameterOutOfRange | DiagCode::NonFiniteParameter
                )),
                "{class_path} {table:?}: {diagnostics:?}"
            );
            false
        }
        Err(other) => panic!("{class_path} {table:?}: {other}"),
    }
}

/// Whether a load of one block of the class with these parameters reports a range warning.
fn warns(class_path: &str, table: &[(&str, Value)]) -> bool {
    let (report, _) = Engine::new()
        .load_cxf(&one_block(class_path, table, None))
        .unwrap_or_else(|error| panic!("{class_path} {table:?}: {error}"));
    report
        .warnings
        .iter()
        .any(|warning| warning.code == DiagCode::ParameterOutOfRange)
}

fn real(value: f64) -> Value {
    Value::Real(value)
}

/// The next `f64` toward negative infinity of a positive finite value.
fn below(value: f64) -> f64 {
    f64::from_bits(value.to_bits() - 1)
}

/// The next `f64` toward positive infinity of a positive finite value.
fn above(value: f64) -> f64 {
    f64::from_bits(value.to_bits() + 1)
}

#[test]
fn a_bound_line_is_the_bound_the_class_rules_enforce() {
    // `Ti >= 1e-13`: inclusive.
    assert_eq!(range("CDL.Reals.PID", "Ti"), ["Ti >= 1e-13"]);
    assert!(accepted("CDL.Reals.PID", &[("Ti", real(1e-13))]));
    assert!(!accepted("CDL.Reals.PID", &[("Ti", real(below(1e-13)))]));

    // `width <= 1.0`: inclusive.
    let pulse = "CDL.Reals.Sources.Pulse";
    assert!(range(pulse, "width").contains(&"width <= 1.0".to_owned()));
    let period = ("period", real(1.0));
    assert!(accepted(pulse, &[period.clone(), ("width", real(1.0))]));
    assert!(!accepted(
        pulse,
        &[period.clone(), ("width", real(above(1.0)))]
    ));

    // `delta > 0.0`: exclusive.
    assert_eq!(range("CDL.Reals.MovingAverage", "delta"), ["delta > 0.0"]);
    assert!(!accepted(
        "CDL.Reals.MovingAverage",
        &[("delta", real(0.0))]
    ));
    assert!(accepted(
        "CDL.Reals.MovingAverage",
        &[("delta", real(f64::MIN_POSITIVE))]
    ));

    // `pAtm > 0.0 and finite`: exclusive, and infinity is refused.
    let enthalpy = "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi";
    assert_eq!(range(enthalpy, "pAtm"), ["pAtm > 0.0 and finite"]);
    assert!(!accepted(enthalpy, &[("pAtm", real(0.0))]));
    assert!(!accepted(enthalpy, &[("pAtm", real(f64::INFINITY))]));
    assert!(accepted(enthalpy, &[("pAtm", real(f64::MIN_POSITIVE))]));
    assert!(accepted(enthalpy, &[("pAtm", real(f64::MAX))]));

    // `shift is finite`: any finite value, no infinity, no NaN.
    let trigger = "CDL.Logical.Sources.SampleTrigger";
    assert_eq!(range(trigger, "shift"), ["shift is finite"]);
    for (shift, expected) in [
        (-1e300, true),
        (0.0, true),
        (f64::MAX, true),
        (f64::INFINITY, false),
        (f64::NEG_INFINITY, false),
        (f64::NAN, false),
    ] {
        let table = [period.clone(), ("shift", real(shift))];
        assert_eq!(accepted(trigger, &table), expected, "{shift}");
    }

    // `yearRef >= 2010` and `yearRef <= 2031`: both inclusive.
    let calendar = "CDL.Reals.Sources.CalendarTime";
    assert_eq!(
        range(calendar, "yearRef"),
        ["yearRef >= 2010", "yearRef <= 2031"]
    );
    let zero_time = enumeration_types()
        .find(|row| row.class_path == "CDL.Types.ZeroTime")
        .and_then(|row| row.id)
        .expect("the ZeroTime enumeration");
    let custom = Value::Enum {
        class: zero_time,
        ordinal: 3,
    };
    for (year, expected) in [(2009, false), (2010, true), (2031, true), (2032, false)] {
        let table = [
            ("zerTim", custom.clone()),
            ("yearRef", Value::Integer(year)),
        ];
        assert_eq!(accepted(calendar, &table), expected, "{year}");
    }
}

#[test]
fn a_relation_line_is_the_relation_the_class_rules_enforce() {
    // `uLow <= uHigh`: equal is accepted, crossed is refused.
    let hysteresis = "CDL.Reals.Hysteresis";
    assert_eq!(
        range(hysteresis, "uHigh"),
        ["uLow <= uHigh", "uLow == uHigh warns"]
    );
    let table = |low: f64, high: f64| [("uLow", real(low)), ("uHigh", real(high))];
    assert!(accepted(hysteresis, &table(0.0, 1.0)));
    assert!(accepted(hysteresis, &table(1.0, 1.0)));
    assert!(!accepted(hysteresis, &table(above(1.0), 1.0)));

    // `0.001 <= h * n <= 0.5`: inclusive at both ends.
    let stage = "CDL.Integers.Stage";
    assert_eq!(range(stage, "h"), ["0.001 <= h * n <= 0.5"]);
    assert!(range(stage, "n").contains(&"0.001 <= h * n <= 0.5".to_owned()));
    let table = |n: i64, h: f64| {
        [
            ("n", Value::Integer(n)),
            ("holdDuration", real(1.0)),
            ("h", real(h)),
        ]
    };
    assert!(accepted(stage, &table(2, 0.25)));
    assert!(!accepted(stage, &table(2, above(0.25))));
    assert!(accepted(stage, &table(1, 0.001)));
    assert!(!accepted(stage, &table(1, below(0.001))));

    // `nout * nin <= 1048576`: inclusive.
    let gain = "CDL.Reals.MatrixGain";
    assert!(range(gain, "nin").contains(&"nout * nin <= 1048576".to_owned()));
    let table =
        |nout: i64, nin: i64| [("nout", Value::Integer(nout)), ("nin", Value::Integer(nin))];
    assert!(accepted(gain, &table(1024, 1024)));
    assert!(!accepted(gain, &table(1024, 1025)));

    // `1 <= extract_<i> <= nin`: inclusive at both ends, for every element.
    let extract = "CDL.Routing.BooleanExtractSignal";
    assert_eq!(range(extract, "extract"), ["1 <= extract_<i> <= nin"]);
    let table = |element: i64| {
        [
            ("nin", Value::Integer(3)),
            ("nout", Value::Integer(1)),
            ("extract_1", Value::Integer(element)),
        ]
    };
    for (element, expected) in [(0, false), (1, true), (3, true), (4, false)] {
        assert_eq!(accepted(extract, &table(element)), expected, "{element}");
    }

    // `the number of true msk_<i> == nout`.
    let filter = "CDL.Routing.BooleanVectorFilter";
    assert_eq!(range(filter, "msk"), ["the number of true msk_<i> == nout"]);
    let table = |nout: i64, first: bool| {
        [
            ("nin", Value::Integer(2)),
            ("nout", Value::Integer(nout)),
            ("msk_1", Value::Boolean(first)),
            ("msk_2", Value::Boolean(true)),
        ]
    };
    assert!(accepted(filter, &table(2, true)));
    assert!(accepted(filter, &table(1, false)));
    assert!(!accepted(filter, &table(1, true)));
    assert!(!accepted(filter, &table(2, false)));
}

#[test]
fn a_warns_line_is_the_case_the_engine_warns_about_and_still_loads() {
    // `uLow == uHigh warns`.
    let table = |low: f64, high: f64| [("uLow", real(low)), ("uHigh", real(high))];
    assert!(warns("CDL.Reals.Hysteresis", &table(1.0, 1.0)));
    assert!(!warns("CDL.Reals.Hysteresis", &table(1.0, above(1.0))));

    // `debounce > feedbackDelay warns`: equal does not.
    let proof = "CDL.Logical.Proof";
    assert_eq!(range(proof, "debounce"), ["debounce > feedbackDelay warns"]);
    let table =
        |debounce: f64, delay: f64| [("debounce", real(debounce)), ("feedbackDelay", real(delay))];
    assert!(warns(proof, &table(above(1.0), 1.0)));
    assert!(!warns(proof, &table(1.0, 1.0)));
    assert!(!warns(proof, &table(0.5, 1.0)));

    // `period < 2.0 * minTruFalHol warns`: equal does not.
    let pulse = "CDL.Logical.VariablePulse";
    assert_eq!(
        range(pulse, "period"),
        ["period < 2.0 * minTruFalHol warns"]
    );
    let table = |period: f64, hold: f64| [("period", real(period)), ("minTruFalHol", real(hold))];
    assert!(warns(pulse, &table(below(1.0), 0.5)));
    assert!(!warns(pulse, &table(1.0, 0.5)));
    assert!(!warns(pulse, &table(2.0, 0.5)));
}

#[test]
fn the_ranges_of_two_classes_with_the_same_rules_are_the_same_lines() {
    // PID and PIDWithReset share their rules; a per-class cache or a stale join would differ.
    let mut by_class: BTreeMap<&str, Vec<Vec<String>>> = BTreeMap::new();
    for class_path in ["CDL.Reals.PID", "CDL.Reals.PIDWithReset"] {
        for parameter in ["k", "Ti", "Td", "r", "yMax", "yMin", "Ni", "Nd", "xi_start"] {
            by_class
                .entry(class_path)
                .or_default()
                .push(range(class_path, parameter));
        }
    }
    assert_eq!(
        by_class["CDL.Reals.PID"],
        by_class["CDL.Reals.PIDWithReset"]
    );
    assert!(range("CDL.Reals.PID", "xi_start").is_empty());
}
