//! The class signature hash: reference vectors, registry coverage and one-field perturbation
//! controls (compiled-schedule blob decision 2, GitHub #400).

use std::collections::BTreeSet;

use crate::class_signature::{ClassRecord, Fnv64, entry_signature_hash, with_entry_record};
use crate::{
    BlockKind, DefaultLiteral, DefaultSource, ParamDefault, ParamRule, PortKind, PortNaming,
    catalog, class_is_target_bound, class_signature_hash, registry,
};

fn fnv(bytes: &[u8]) -> u64 {
    let mut h = Fnv64::new();
    h.bytes(bytes);
    h.finish()
}

#[test]
fn fnv1a_64_matches_the_published_reference_vectors() {
    assert_eq!(fnv(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv(b"foobar"), 0x8594_4171_f739_67e8);
}

#[test]
fn every_registered_class_has_one_stable_hash_that_the_catalog_publishes() {
    let mut seen = BTreeSet::new();
    for entry in catalog() {
        let hash = class_signature_hash(entry.class_path).expect("registered class");
        assert_eq!(hash, entry.signature_hash, "{}", entry.class_path);
        assert_eq!(
            class_signature_hash(entry.class_path),
            Some(hash),
            "{} hashes differently on a second call",
            entry.class_path
        );
        assert!(seen.insert(hash), "{} collides", entry.class_path);
    }
    assert_eq!(seen.len(), catalog().len());
    assert_eq!(class_signature_hash("CDL.Reals.NotAClass"), None);
    assert_eq!(class_signature_hash(""), None);
}

#[test]
fn target_bound_flag_names_exactly_the_fifteen_pinned_classes() {
    let flagged: Vec<&str> = catalog()
        .iter()
        .filter(|entry| entry.target_bound)
        .map(|entry| entry.class_path)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(
        flagged,
        [
            "CDL.Psychrometrics.DewPoint_TDryBulPhi",
            "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
            "CDL.Psychrometrics.WetBulb_TDryBulPhi",
            "CDL.Reals.Acos",
            "CDL.Reals.Asin",
            "CDL.Reals.Atan",
            "CDL.Reals.Atan2",
            "CDL.Reals.Cos",
            "CDL.Reals.Exp",
            "CDL.Reals.Log",
            "CDL.Reals.Log10",
            "CDL.Reals.Sin",
            "CDL.Reals.Sources.Sin",
            "CDL.Reals.Tan",
            "CDL.Utilities.SunRiseSet",
        ]
    );
    for entry in catalog() {
        assert_eq!(
            entry.target_bound,
            class_is_target_bound(entry.class_path),
            "{}",
            entry.class_path
        );
    }
    for path in [
        "",
        "CDL.Reals.Add",
        "CDL.Reals.sin",
        "CDL.Reals.Sin ",
        "CDL.Reals.Sin\0",
    ] {
        assert!(!class_is_target_bound(path), "{path:?}");
    }
}

/// The parts of a class record a default instance owns, copied out so a test can borrow them for
/// as long as it likes.
struct Instance {
    class_path: &'static str,
    kind: BlockKind,
    state_len: usize,
    revision: u32,
    inputs: Vec<PortKind>,
    outputs: Vec<PortKind>,
}

fn instance(class_path: &str) -> Instance {
    let entry = registry::lookup(class_path).expect("registered class");
    let block = (entry.make)(&oce_model::ParamTable::default());
    let signature = block.resolved_signature();
    Instance {
        class_path: entry.class_path,
        kind: block.kind(),
        state_len: block.state_len(),
        revision: block.state_contract_revision(),
        inputs: signature.inputs.to_vec(),
        outputs: signature.outputs.to_vec(),
    }
}

/// The class's canonical record, rebuilt independently of `with_entry_record` from the registry's
/// public metadata (checked equal to the registry's own hash below).
fn record(instance: &Instance) -> ClassRecord<'_> {
    let entry = registry::lookup(instance.class_path).unwrap();
    let names = crate::port_names::port_names(instance.class_path);
    let (naming, width_driven) = crate::class_signature::naming_policy(entry);
    ClassRecord {
        class_path: instance.class_path,
        kind: instance.kind,
        state_len: instance.state_len,
        state_contract_revision: instance.revision,
        inputs: &instance.inputs,
        outputs: &instance.outputs,
        input_names: names.map(|n| n.inputs),
        output_names: names.map(|n| n.outputs),
        naming,
        width_driven,
        reserved: instance.class_path.starts_with("urn:oce:lowering#"),
        target_bound: class_is_target_bound(instance.class_path),
        rules: entry.param_rules(),
        defaults: registry::param_defaults(instance.class_path),
    }
}

#[test]
fn the_record_the_tests_perturb_is_the_record_the_registry_hashes() {
    for entry in registry::all_entries() {
        let owned = instance(entry.class_path);
        assert_eq!(
            record(&owned).hash(),
            entry_signature_hash(entry),
            "{}",
            entry.class_path
        );
        assert_eq!(
            with_entry_record(entry, |record| record.hash()),
            entry_signature_hash(entry)
        );
    }
}

#[test]
fn gaining_or_retyping_or_renaming_a_port_changes_the_hash() {
    let add = instance("CDL.Reals.Add");
    let base = record(&add);
    let extra_input = [PortKind::Real, PortKind::Real, PortKind::Real];
    let extra_output = [PortKind::Real, PortKind::Real];
    let retyped = [PortKind::Real, PortKind::Integer];
    let renamed = ["u1", "u3"];
    let edits: [(&str, ClassRecord<'_>); 5] = [
        (
            "a third input",
            ClassRecord {
                inputs: &extra_input,
                ..base
            },
        ),
        (
            "a second output",
            ClassRecord {
                outputs: &extra_output,
                ..base
            },
        ),
        (
            "a retyped input",
            ClassRecord {
                inputs: &retyped,
                ..base
            },
        ),
        (
            "a renamed input",
            ClassRecord {
                input_names: Some(&renamed),
                ..base
            },
        ),
        (
            "dropped port names",
            ClassRecord {
                input_names: None,
                output_names: None,
                ..base
            },
        ),
    ];
    for (what, edited) in edits {
        assert_ne!(base.hash(), edited.hash(), "{what} must change the hash");
    }
}

#[test]
fn state_layout_kind_and_flags_each_change_the_hash() {
    let add = instance("CDL.Reals.Add");
    let base = record(&add);
    let delay = instance("CDL.Discrete.UnitDelay");
    let stateful = record(&delay);
    let edits = [
        (
            base,
            ClassRecord {
                kind: BlockKind::Stateful,
                ..base
            },
        ),
        (
            stateful,
            ClassRecord {
                state_len: stateful.state_len + 1,
                ..stateful
            },
        ),
        (
            stateful,
            ClassRecord {
                state_contract_revision: stateful.state_contract_revision + 1,
                ..stateful
            },
        ),
        (
            base,
            ClassRecord {
                naming: PortNaming::Positional,
                ..base
            },
        ),
        (
            base,
            ClassRecord {
                width_driven: !base.width_driven,
                ..base
            },
        ),
        (
            base,
            ClassRecord {
                reserved: !base.reserved,
                ..base
            },
        ),
        (
            base,
            ClassRecord {
                target_bound: !base.target_bound,
                ..base
            },
        ),
        (
            base,
            ClassRecord {
                class_path: "CDL.Reals.Add2",
                ..base
            },
        ),
    ];
    for (index, (original, edited)) in edits.iter().enumerate() {
        assert_ne!(
            original.hash(),
            edited.hash(),
            "edit {index} must change the hash"
        );
    }
}

#[test]
fn a_changed_parameter_rule_changes_the_hash() {
    let hysteresis = instance("CDL.Reals.Hysteresis");
    let original = record(&hysteresis);
    assert!(!original.rules.is_empty());
    let fewer = ClassRecord {
        rules: &original.rules[..original.rules.len() - 1],
        ..original
    };
    assert_ne!(
        original.hash(),
        fewer.hash(),
        "a dropped rule must change the hash"
    );

    let add = instance("CDL.Reals.Add");
    let base = record(&add);
    let rule = [ParamRule::RealGreaterThan {
        name: "delta",
        min: 0.0,
    }];
    let signed_zero = [ParamRule::RealGreaterThan {
        name: "delta",
        min: -0.0,
    }];
    let renamed = [ParamRule::RealGreaterThan {
        name: "delta2",
        min: 0.0,
    }];
    let retagged = [ParamRule::RealFiniteGreaterThan {
        name: "delta",
        min: 0.0,
    }];
    let with_rule = ClassRecord {
        rules: &rule,
        ..base
    }
    .hash();
    assert_ne!(with_rule, base.hash(), "a gained rule must change the hash");
    for (what, rules) in [
        ("a signed-zero bound", &signed_zero[..]),
        ("a renamed rule parameter", &renamed[..]),
        ("a different rule variant", &retagged[..]),
    ] {
        assert_ne!(
            with_rule,
            ClassRecord { rules, ..base }.hash(),
            "{what} must change the hash"
        );
    }
}

#[test]
fn a_derived_default_formula_is_documentation_and_does_not_change_the_hash() {
    let pid = instance("CDL.Reals.PIDWithReset");
    let base = record(&pid);
    let with_default = |default: DefaultSource| {
        let defaults = [ParamDefault {
            name: "y_reset",
            default,
        }];
        ClassRecord {
            defaults: &defaults,
            ..base
        }
        .hash()
    };
    let derived = with_default(DefaultSource::Derived {
        formula: "xi_start",
    });
    let reworded = with_default(DefaultSource::Derived {
        formula: "equal to xi_start",
    });
    assert_eq!(
        derived, reworded,
        "formula text is documentation, not identity"
    );
    let required = with_default(DefaultSource::Required);
    let literal = with_default(DefaultSource::Literal(DefaultLiteral::Real(0.0)));
    let other_literal = with_default(DefaultSource::Literal(DefaultLiteral::Real(-0.0)));
    assert_ne!(derived, required);
    assert_ne!(derived, literal);
    assert_ne!(required, literal);
    assert_ne!(literal, other_literal, "a literal's bits are identity");

    // The live PIDWithReset record carries a derived default, so rewording it in place is the same
    // control.
    assert!(
        base.defaults
            .iter()
            .any(|d| matches!(d.default, DefaultSource::Derived { .. }))
    );
    let reworded_live: Vec<ParamDefault> = base
        .defaults
        .iter()
        .map(|d| match d.default {
            DefaultSource::Derived { .. } => ParamDefault {
                name: d.name,
                default: DefaultSource::Derived {
                    formula: "reworded documentation",
                },
            },
            _ => *d,
        })
        .collect();
    assert_eq!(
        base.hash(),
        ClassRecord {
            defaults: &reworded_live,
            ..base
        }
        .hash()
    );
}

#[test]
fn reordering_two_defaults_or_two_rules_changes_the_hash() {
    let pid = instance("CDL.Reals.PID");
    let base = record(&pid);
    assert!(base.defaults.len() >= 2 && base.rules.len() >= 2);
    let mut defaults = base.defaults.to_vec();
    defaults.swap(0, 1);
    let mut rules = base.rules.to_vec();
    rules.swap(0, 1);
    assert_ne!(
        base.hash(),
        ClassRecord {
            defaults: &defaults,
            ..base
        }
        .hash()
    );
    assert_ne!(
        base.hash(),
        ClassRecord {
            rules: &rules,
            ..base
        }
        .hash()
    );
}
