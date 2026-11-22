//! The load gate refuses a parameter name the block's class does not declare (issue #748),
//! through [`crate::validate_block_parameters`], the load gate's parameter checks on one block.

use std::sync::Arc;

use oce_blocks::{DefaultLiteral, DefaultSource, ParamNameTemplate, catalog, class_parameters};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{ParamTable, Value, ValueType};

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

fn unknown(name: &str, class_path: &str, nearest: Option<&str>) -> Diagnostic {
    let message = match nearest {
        Some(nearest) => format!(
            "parameter `{name}` on block `{class_path}` is not a parameter of its class; did you \
             mean `{nearest}`?"
        ),
        None => {
            format!("parameter `{name}` on block `{class_path}` is not a parameter of its class")
        }
    };
    Diagnostic::error(DiagCode::UnknownParameter, message).with_subject("block#0".to_owned())
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<DiagCode> {
    diagnostics.iter().map(|d| d.code).collect()
}

const PULSE: &str = "CDL.Integers.Sources.Pulse";

#[test]
fn the_misspelt_pulse_amplitude_is_refused_and_names_the_parameter_it_meant() {
    // The issue's document: `period = 4`, `amplitud = 3`. Before this rule the table loaded
    // clean and the block output the default amplitude 1.
    assert_eq!(
        refusal(
            PULSE,
            &[
                ("period", Value::Real(4.0)),
                ("amplitud", Value::Integer(3))
            ]
        ),
        vec![unknown("amplitud", PULSE, Some("amplitude"))]
    );
    // Spelt right, it loads.
    assert_eq!(
        refusal(
            PULSE,
            &[
                ("period", Value::Real(4.0)),
                ("amplitude", Value::Integer(3))
            ]
        ),
        vec![]
    );
}

/// The literal default of a catalog parameter as a value, when it has one.
fn literal(default: DefaultSource) -> Option<Value> {
    match default {
        DefaultSource::Literal(DefaultLiteral::Real(x)) => Some(Value::Real(x)),
        DefaultSource::Literal(DefaultLiteral::Integer(n)) => Some(Value::Integer(n)),
        DefaultSource::Literal(DefaultLiteral::Boolean(b)) => Some(Value::Boolean(b)),
        _ => None,
    }
}

#[test]
fn an_undeclared_name_adds_exactly_one_finding_on_every_class_whatever_its_value() {
    let values = [
        Value::Real(1.5),
        Value::Integer(2),
        Value::Boolean(true),
        Value::String(Arc::from("x")),
    ];
    let mut classes = 0;
    for entry in catalog() {
        let class = entry.class_path;
        let base = refusal(class, &[]);
        for (index, name) in ["bogus", "k_x", "zzz_1_2_3"].into_iter().enumerate() {
            let value = values[index % values.len()].clone();
            let with = refusal(class, &[(name, value)]);
            let added: Vec<&Diagnostic> = with.iter().filter(|d| !base.contains(d)).collect();
            assert_eq!(added.len(), 1, "{class} {name}: {with:?}");
            assert_eq!(added[0].code, DiagCode::UnknownParameter, "{class} {name}");
            assert!(
                added[0]
                    .message
                    .starts_with(&format!("parameter `{name}` on block `{class}`")),
                "{}",
                added[0].message
            );
            assert_eq!(added[0].subject.as_deref(), Some("block#0"));
            // Nothing the class already reported moved.
            assert_eq!(with.len(), base.len() + 1, "{class} {name}");
        }
        classes += 1;
    }
    assert!(classes > 130, "{classes}");
}

#[test]
fn every_declared_parameter_bound_to_its_default_draws_no_unknown_name_finding() {
    let mut bound = 0;
    for entry in catalog() {
        let class = entry.class_path;
        for parameter in class_parameters(class).expect("registered") {
            let template = ParamNameTemplate::parse(parameter.name);
            let name = template
                .element_name(&vec![1; template.rank()])
                .expect("an element name");
            let value = literal(parameter.default).unwrap_or(Value::Real(1.0));
            let found = refusal(class, &[(&name, value)]);
            assert!(
                !codes(&found).contains(&DiagCode::UnknownParameter),
                "{class} {name}: {found:?}"
            );
            bound += 1;
        }
    }
    assert!(bound > 180, "{bound}");
}

#[test]
fn an_element_spelling_is_refused_once_by_its_own_rule_and_a_non_element_is_unknown() {
    let sum = "CDL.Reals.MultiSum";
    let nin = ("nin", Value::Integer(2));
    for (name, expected) in [
        ("k_01", DiagCode::NonCanonicalElementName),
        ("k_+1", DiagCode::NonCanonicalElementName),
        ("k_-1", DiagCode::NonCanonicalElementName),
        ("k_0", DiagCode::ParameterOutOfRange),
        ("k_3", DiagCode::ParameterOutOfRange),
    ] {
        assert_eq!(
            codes(&refusal(sum, &[nin.clone(), (name, Value::Real(1.0))])),
            vec![expected],
            "{name}"
        );
    }
    for (name, nearest) in [
        ("k_x", None),
        ("k_", None),
        ("k_1_2", None),
        ("k", Some("k_<i>")),
        ("K_1", Some("k_<i>")),
        ("gain_1", None),
    ] {
        assert_eq!(
            refusal(sum, &[nin.clone(), (name, Value::Real(1.0))]),
            vec![unknown(name, sum, nearest)],
            "{name}"
        );
    }
    // Inside the extent each element loads.
    assert_eq!(
        refusal(
            sum,
            &[nin, ("k_1", Value::Real(1.0)), ("k_2", Value::Real(2.0))]
        ),
        vec![]
    );
}

/// The parameters a class needs before an element of its arrays can be judged: every required
/// scalar (an Integer 2, a Real 1.0, a Boolean `true`), and for a `TimeTable` a 2x2 table (rows
/// `0, 1` and `1, 2`).
fn judging_base(class: &str) -> Vec<(&'static str, Value)> {
    let mut entries = Vec::new();
    for parameter in class_parameters(class).expect("registered") {
        if parameter.name == "table_<row>_<col>" {
            entries.extend([
                ("table_1_1", Value::Real(0.0)),
                ("table_1_2", Value::Real(1.0)),
                ("table_2_1", Value::Real(1.0)),
                ("table_2_2", Value::Real(2.0)),
            ]);
        } else if parameter.default == DefaultSource::Required {
            let value = match parameter.value_type {
                ValueType::Integer => Value::Integer(2),
                ValueType::Boolean => Value::Boolean(true),
                _ => Value::Real(1.0),
            };
            entries.push((parameter.name, value));
        }
    }
    entries
}

/// What `extra` adds to the refusal of `class` over [`judging_base`] alone.
fn added_by(class: &str, extra: (&str, Value)) -> Vec<Diagnostic> {
    let mut entries = judging_base(class);
    let base = refusal(class, &entries);
    entries.push(extra);
    refusal(class, &entries)
        .into_iter()
        .filter(|d| !base.contains(d))
        .collect()
}

#[test]
fn an_element_at_index_zero_of_every_array_is_refused_by_a_rule_other_than_this_one() {
    // The rule leaves a zero index to the extent rules: hold them to refusing it on every class
    // with an array, so nothing at index zero loads.
    let mut arrays = 0;
    for entry in catalog() {
        let class = entry.class_path;
        for parameter in class_parameters(class).expect("registered") {
            let template = ParamNameTemplate::parse(parameter.name);
            let zeros: Vec<String> = match template.rank() {
                0 => continue,
                1 => vec![format!("{}_0", template.base())],
                _ => vec![
                    format!("{}_0_1", template.base()),
                    format!("{}_1_0", template.base()),
                ],
            };
            for name in zeros {
                let added = codes(&added_by(class, (&name, Value::Real(1.0))));
                assert!(
                    !added.is_empty() && !added.contains(&DiagCode::UnknownParameter),
                    "{class} {name}: {added:?}"
                );
                arrays += 1;
            }
        }
    }
    assert!(arrays >= 10, "{arrays}");
}

#[test]
fn a_name_under_a_matrix_or_offset_prefix_is_refused_once_by_that_rule() {
    // The matrix and offset rules read every name under their prefix: a name that is no cell of
    // their shape is theirs to refuse, and this rule says nothing beside it.
    for (class, names) in [
        (
            "CDL.Reals.MatrixGain",
            &["K_1", "K_x", "K_1_2_3", "K_", "K_1_x"][..],
        ),
        (
            "CDL.Reals.Sources.TimeTable",
            &[
                "table_1",
                "table_1_2_3",
                "table_x_1",
                "offset_x",
                "offset_1_1",
                "offset_",
            ][..],
        ),
        (
            "CDL.Integers.Sources.TimeTable",
            &["table_1", "table_x"][..],
        ),
        (
            "CDL.Logical.Sources.TimeTable",
            &["table_1", "table_1_x"][..],
        ),
    ] {
        for name in names {
            let added = codes(&added_by(class, (name, Value::Real(1.0))));
            assert_eq!(added, vec![DiagCode::ParameterOutOfRange], "{class} {name}");
        }
    }
    // Without a table no offset can be judged, and the block is refused for the table.
    let refused = codes(&refusal(
        "CDL.Reals.Sources.TimeTable",
        &[("offset_x", Value::Real(1.0))],
    ));
    assert_eq!(refused, vec![DiagCode::MissingRequiredParameter]);
    // A one-column table leaves the offset rule nothing to measure against: still refused.
    let one_column = refusal(
        "CDL.Reals.Sources.TimeTable",
        &[
            ("table_1_1", Value::Real(0.0)),
            ("table_2_1", Value::Real(1.0)),
            ("offset_x", Value::Real(1.0)),
        ],
    );
    assert!(!one_column.is_empty(), "a one-column table loaded");
}

#[test]
fn findings_follow_parameter_order_and_a_repeated_unknown_name_is_reported_at_each_binding() {
    let constant = "CDL.Reals.Sources.Constant";
    let found = refusal(
        constant,
        &[
            ("zeta", Value::Real(1.0)),
            ("k", Value::Real(2.0)),
            ("alpha", Value::Real(3.0)),
            ("zeta", Value::Real(4.0)),
        ],
    );
    let unknowns: Vec<&str> = found
        .iter()
        .filter(|d| d.code == DiagCode::UnknownParameter)
        .map(|d| d.message.split('`').nth(1).expect("named"))
        .collect();
    assert_eq!(unknowns.len(), 3, "{found:?}");
    let mut sorted = unknowns.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, ["alpha", "zeta", "zeta"]);
    assert!(codes(&found).contains(&DiagCode::DuplicateParameter));
}

#[test]
fn an_unknown_name_with_a_non_finite_value_is_refused_for_both() {
    let found = refusal(
        "CDL.Reals.Sources.Constant",
        &[("kk", Value::Real(f64::NAN))],
    );
    let mut found = codes(&found);
    found.sort_by_key(|code| code.as_str());
    assert_eq!(
        found,
        vec![DiagCode::NonFiniteParameter, DiagCode::UnknownParameter]
    );
}

#[test]
fn a_block_of_an_unregistered_class_gets_no_unknown_name_finding() {
    for class in ["CDL.Reals.Nope", "urn:example#Thing", ""] {
        let found = refusal(class, &[("anything", Value::Real(1.0))]);
        assert!(
            !codes(&found).contains(&DiagCode::UnknownParameter),
            "{class}: {found:?}"
        );
    }
}

#[test]
fn a_composite_parameter_name_reaches_no_block_and_a_name_another_class_declares_is_unknown() {
    // `nin` is MultiAnd's, not Not's: the class of the block decides, never the name alone.
    assert_eq!(
        refusal("CDL.Logical.Not", &[("nin", Value::Integer(1))]),
        vec![unknown("nin", "CDL.Logical.Not", None)]
    );
    assert_eq!(
        refusal("CDL.Logical.MultiAnd", &[("nin", Value::Integer(2))]),
        vec![]
    );
}

/// A seeded xorshift64* generator (no dependency).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % bound as u64).expect("small")
    }
}

#[test]
fn random_class_and_name_pairs_are_refused_exactly_when_the_class_does_not_declare_them() {
    // The property, over random names built from catalog fragments: a declared name never draws
    // the finding; an undeclared name that no other rule owns (no element shape of the class's
    // arrays, no matrix or offset prefix) draws exactly one, naming it, whatever its value.
    let classes: Vec<&str> = catalog().iter().map(|entry| entry.class_path).collect();
    let pieces = [
        "k",
        "K",
        "nin",
        "nout",
        "u",
        "amplitude",
        "Ti",
        "table",
        "offset",
        "msk",
        "extract",
        "_",
        "_1",
        "_0",
        "_01",
        "_+1",
        "_x",
        "Low",
        "uLow",
        "uHigh",
        "delta",
        "period",
        "y_start",
        "é",
    ];
    let values = [
        Value::Real(1.0),
        Value::Integer(2),
        Value::Boolean(false),
        Value::String(Arc::from("s")),
    ];
    let mut rng = Rng(0x748_0003);
    let (mut refused, mut admitted) = (0, 0);
    for _ in 0..10_000 {
        let class = classes[rng.below(classes.len())];
        let mut name = String::new();
        for _ in 0..=rng.below(3) {
            name.push_str(pieces[rng.below(pieces.len())]);
        }
        let value = values[rng.below(values.len())].clone();
        let mut entries = judging_base(class);
        entries.retain(|(declared, _)| *declared != name);
        entries.push((&name, value));
        let found: Vec<Diagnostic> = refusal(class, &entries)
            .into_iter()
            .filter(|d| d.code == DiagCode::UnknownParameter)
            .collect();
        let declared = oce_blocks::declares_parameter(class, &name) == Some(true);
        let owned_elsewhere = oce_blocks::spells_array_element(class, &name)
            || ["K_", "table_", "offset_"]
                .iter()
                .any(|prefix| name.starts_with(prefix));
        if declared {
            assert!(found.is_empty(), "{class} {name}: {found:?}");
            admitted += 1;
        } else if !owned_elsewhere {
            assert_eq!(found.len(), 1, "{class} {name}");
            assert_eq!(
                found[0].message.split('`').nth(1),
                Some(name.as_str()),
                "{class}"
            );
            refused += 1;
        }
    }
    assert!(
        refused > 1_000 && admitted > 30,
        "{refused} refused, {admitted} admitted"
    );
}

#[test]
fn an_invisible_character_in_the_name_is_written_as_an_escape_and_the_meant_name_suggested() {
    // `amplitude` and a zero-width space: quoted raw, the finding would read as the declared
    // name refused. The escape shows the character; the suggestion is tried without it.
    for (name, quoted) in [
        ("amplitude\u{200B}", "amplitude\\u{200B}"),
        ("\u{202E}amplitude", "\\u{202E}amplitude"),
        ("ampli\u{202E}tude", "ampli\\u{202E}tude"),
        ("amplitud\u{0007}", "amplitud\\u{0007}"),
    ] {
        let found = refusal(
            PULSE,
            &[("period", Value::Real(4.0)), (name, Value::Integer(3))],
        );
        assert_eq!(
            found,
            vec![unknown(quoted, PULSE, Some("amplitude"))],
            "{name:?}"
        );
        assert!(!found[0].message.contains(name), "{name:?} quoted raw");
    }
    // A name that is invisible characters only suggests nothing.
    assert_eq!(
        refusal(
            PULSE,
            &[
                ("period", Value::Real(4.0)),
                ("\u{200B}", Value::Integer(3))
            ]
        ),
        vec![unknown("\\u{200B}", PULSE, None)]
    );
}

#[test]
fn a_long_name_is_quoted_by_an_excerpt_not_whole() {
    let long = format!("{}\u{200B}", "q".repeat(60 * 1024));
    let found = refusal(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(1.0)), (&long, Value::Real(1.0))],
    );
    let excerpt = format!("{}…", "q".repeat(oce_diag::EXCERPT_BYTES));
    assert_eq!(
        found,
        vec![unknown(&excerpt, "CDL.Reals.Sources.Constant", None)]
    );
    assert!(found[0].message.len() < 256, "{}", found[0].message.len());
}
