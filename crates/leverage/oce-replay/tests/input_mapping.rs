//! The one-time name→slot mapping: exact paths, unique and ambiguous suffixes, unknown targets,
//! unmapped inputs, two columns on one input, type mismatches, policy overrides, and a stale map
//! after a reload (design note, test plan: mapping; decision 4).

#[path = "support/fixtures.rs"]
mod fixtures;

use fixtures::{columnar, load, path_map, path_specs};
use oce_api::{InputDefinition, Value, ValueType};
use oce_replay::{
    ColumnKind, ColumnMap, ColumnSpec, InputMap, MapError, MissingPolicy, Replay, ReplayError,
    ReplayOptions, StepSource, TimeBase,
};

fn real(path: &str) -> InputDefinition {
    InputDefinition {
        path: path.to_owned(),
        value_type: ValueType::Real,
        min: None,
        max: None,
    }
}

fn bounded(path: &str, lo: f64, hi: f64) -> InputDefinition {
    InputDefinition {
        path: path.to_owned(),
        value_type: ValueType::Real,
        min: Some(Value::Real(lo)),
        max: Some(Value::Real(hi)),
    }
}

fn inputs() -> Vec<InputDefinition> {
    vec![
        real("plant.ahu1.TSup"),
        bounded("plant.ahu1.uDam", 0.0, 1.0),
        real("plant.ahu2.TSup"),
        InputDefinition {
            path: "plant.ahu2.uMode".to_owned(),
            value_type: ValueType::Integer,
            min: None,
            max: None,
        },
    ]
}

fn spec(name: &str, kind: ColumnKind) -> ColumnSpec {
    ColumnSpec::new(name, kind)
}

fn entries(pairs: &[(&str, &str)]) -> ColumnMap {
    ColumnMap {
        entries: pairs
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect(),
    }
}

fn specs() -> Vec<ColumnSpec> {
    vec![
        spec("t_sup_1", ColumnKind::Real),
        spec("damper", ColumnKind::Real),
        spec("t_sup_2", ColumnKind::Real),
        spec("mode", ColumnKind::Integer),
        spec("comment", ColumnKind::Real),
    ]
}

fn good_map() -> ColumnMap {
    entries(&[
        ("t_sup_1", "plant.ahu1.TSup"), // exact path
        ("damper", "uDam"),             // unique suffix
        ("t_sup_2", "ahu2.TSup"),       // unique multi-segment suffix
        ("mode", "uMode"),
    ])
}

fn resolve(map: &ColumnMap) -> Result<InputMap, MapError> {
    InputMap::resolve(&inputs(), &specs(), map, &MissingPolicy::Refuse, &[])
}

#[test]
fn exact_paths_and_unique_suffixes_resolve_to_slots_in_canonical_order() {
    let map = resolve(&good_map()).unwrap();
    assert_eq!(map.len(), 4);
    let columns: Vec<&str> = (0..4).map(|s| map.column(s)).collect();
    assert_eq!(columns, ["t_sup_1", "damper", "t_sup_2", "mode"]);
    assert_eq!(map.slot_of_column("t_sup_2"), Some(2));
    assert_eq!(map.slot_of_column("comment"), None);
    assert_eq!(map.ignored_columns(), ["comment"]);
    assert!(!map.can_substitute());
}

#[test]
fn an_ambiguous_suffix_is_refused_with_every_candidate() {
    let mut map = good_map();
    map.entries[0].1 = "TSup".to_owned();
    assert_eq!(
        resolve(&map).unwrap_err(),
        MapError::AmbiguousTarget {
            target: "TSup".to_owned(),
            candidates: vec!["plant.ahu1.TSup".to_owned(), "plant.ahu2.TSup".to_owned()],
        }
    );
}

#[test]
fn a_suffix_matches_only_at_a_dot_boundary() {
    let mut map = good_map();
    map.entries[1].1 = "Dam".to_owned(); // `uDam` ends with it, but not at a `.`
    assert_eq!(
        resolve(&map).unwrap_err(),
        MapError::UnknownTarget {
            column: "damper".to_owned(),
            target: "Dam".to_owned()
        }
    );
}

#[test]
fn unknown_targets_columns_and_empty_targets_are_refused() {
    let mut map = good_map();
    map.entries[3].1 = "plant.ahu3.uMode".to_owned();
    assert!(matches!(resolve(&map), Err(MapError::UnknownTarget { .. })));
    let mut map = good_map();
    map.entries[3].1 = String::new();
    assert!(matches!(resolve(&map), Err(MapError::UnknownTarget { .. })));
    let mut map = good_map();
    map.entries[3].0 = "no_such_column".to_owned();
    assert_eq!(
        resolve(&map).unwrap_err(),
        MapError::UnknownColumn {
            column: "no_such_column".to_owned()
        }
    );
}

#[test]
fn an_engine_input_no_column_supplies_is_refused() {
    let mut map = good_map();
    map.entries.remove(1);
    assert_eq!(
        resolve(&map).unwrap_err(),
        MapError::UnmappedInput {
            path: "plant.ahu1.uDam".to_owned()
        }
    );
}

#[test]
fn two_columns_on_one_input_are_refused() {
    let mut map = good_map();
    map.entries
        .push(("comment".to_owned(), "plant.ahu1.uDam".to_owned()));
    assert_eq!(
        resolve(&map).unwrap_err(),
        MapError::TwoColumnsOneInput {
            input: "plant.ahu1.uDam".to_owned(),
            first: "damper".to_owned(),
            second: "comment".to_owned(),
        }
    );
    // The same column listed twice is the same refusal.
    let mut map = good_map();
    map.entries.push(("damper".to_owned(), "uDam".to_owned()));
    assert!(matches!(
        resolve(&map),
        Err(MapError::TwoColumnsOneInput { .. })
    ));
}

#[test]
fn a_column_whose_kind_cannot_carry_its_input_is_refused() {
    let mut map = good_map();
    map.entries[3].0 = "comment".to_owned(); // Real column on the Integer input
    assert_eq!(
        resolve(&map).unwrap_err(),
        MapError::TypeMismatch {
            column: "comment".to_owned(),
            input: "plant.ahu2.uMode".to_owned()
        }
    );
    for kind in [
        ColumnKind::Integer,
        ColumnKind::Boolean,
        ColumnKind::Enum("CDL.Types.SimpleController".to_owned()),
    ] {
        assert!(!kind.carries(ValueType::Real), "{kind:?}");
    }
    assert!(ColumnKind::Real.carries(ValueType::Real));
}

#[test]
fn malformed_specs_and_engine_inputs_are_refused() {
    let mut specs = specs();
    specs.push(spec("damper", ColumnKind::Real));
    assert_eq!(
        InputMap::resolve(&inputs(), &specs, &good_map(), &MissingPolicy::Refuse, &[]).unwrap_err(),
        MapError::DuplicateColumn {
            column: "damper".to_owned()
        }
    );
    let mut unsorted = inputs();
    unsorted.swap(0, 1);
    assert!(matches!(
        InputMap::resolve(
            &unsorted,
            &self::specs(),
            &good_map(),
            &MissingPolicy::Refuse,
            &[]
        ),
        Err(MapError::InputsNotCanonical { .. })
    ));
    let mut with_string = inputs();
    with_string.push(InputDefinition {
        path: "plant.z".to_owned(),
        value_type: ValueType::String,
        min: None,
        max: None,
    });
    assert_eq!(
        InputMap::resolve(
            &with_string,
            &self::specs(),
            &good_map(),
            &MissingPolicy::Refuse,
            &[]
        )
        .unwrap_err(),
        MapError::UnsupportedInputType {
            path: "plant.z".to_owned()
        }
    );
}

#[test]
fn policies_resolve_per_input_with_typed_refusals() {
    let resolve_with = |missing: &MissingPolicy, overrides: &[(&str, MissingPolicy)]| {
        let overrides: Vec<(String, MissingPolicy)> = overrides
            .iter()
            .map(|(t, p)| ((*t).to_owned(), p.clone()))
            .collect();
        InputMap::resolve(&inputs(), &specs(), &good_map(), missing, &overrides)
    };
    let map = resolve_with(
        &MissingPolicy::HoldLast { max_age: 30.0 },
        &[
            ("uDam", MissingPolicy::Fallback(Value::Real(1.0))),
            ("ahu2.TSup", MissingPolicy::SkipFrame),
        ],
    )
    .unwrap();
    assert!(matches!(map.policy(0), MissingPolicy::HoldLast { max_age } if *max_age == 30.0));
    assert!(matches!(map.policy(1), MissingPolicy::Fallback(Value::Real(x)) if *x == 1.0));
    assert!(matches!(map.policy(2), MissingPolicy::SkipFrame));
    assert!(map.can_substitute());

    assert_eq!(
        resolve_with(&MissingPolicy::Fallback(Value::Real(0.0)), &[]).unwrap_err(),
        MapError::FallbackIsPerInput
    );
    for bad in [f64::NAN, -1.0, f64::INFINITY] {
        assert_eq!(
            resolve_with(&MissingPolicy::HoldLast { max_age: bad }, &[]).unwrap_err(),
            MapError::InvalidMaxAge { input: None }
        );
        assert_eq!(
            resolve_with(
                &MissingPolicy::Refuse,
                &[("uMode", MissingPolicy::HoldLast { max_age: bad })]
            )
            .unwrap_err(),
            MapError::InvalidMaxAge {
                input: Some("plant.ahu2.uMode".to_owned())
            }
        );
    }
    assert!(resolve_with(&MissingPolicy::HoldLast { max_age: 0.0 }, &[]).is_ok());
    assert_eq!(
        resolve_with(
            &MissingPolicy::Refuse,
            &[
                ("uDam", MissingPolicy::SkipFrame),
                ("plant.ahu1.uDam", MissingPolicy::Refuse)
            ]
        )
        .unwrap_err(),
        MapError::OverrideTwice {
            input: "plant.ahu1.uDam".to_owned()
        }
    );
    assert!(matches!(
        resolve_with(
            &MissingPolicy::Refuse,
            &[("TSup", MissingPolicy::SkipFrame)]
        ),
        Err(MapError::AmbiguousTarget { .. })
    ));
    assert!(matches!(
        resolve_with(
            &MissingPolicy::Refuse,
            &[("nothing", MissingPolicy::SkipFrame)]
        ),
        Err(MapError::UnknownTarget { .. })
    ));
    // Fallback type and domain: the domain is the input's declared bounds, inclusive; NaN is missing.
    let fallback = |target: &str, value: Value| {
        resolve_with(
            &MissingPolicy::Refuse,
            &[(target, MissingPolicy::Fallback(value))],
        )
    };
    assert_eq!(
        fallback("uMode", Value::Real(1.0)).unwrap_err(),
        MapError::FallbackType {
            input: "plant.ahu2.uMode".to_owned()
        }
    );
    assert!(fallback("uMode", Value::Integer(-7)).is_ok());
    assert!(fallback("uDam", Value::Real(0.0)).is_ok());
    assert!(fallback("uDam", Value::Real(1.0)).is_ok());
    for outside in [1.000_000_000_000_000_2, -0.1, f64::NAN, f64::INFINITY] {
        assert_eq!(
            fallback("uDam", Value::Real(outside)).unwrap_err(),
            MapError::FallbackDomain {
                input: "plant.ahu1.uDam".to_owned()
            }
        );
    }
    assert!(
        fallback("ahu1.TSup", Value::Real(f64::INFINITY)).is_ok(),
        "±∞ is a value on an unbounded input"
    );
    assert_eq!(
        fallback("ahu1.TSup", Value::Real(f64::NAN)).unwrap_err(),
        MapError::FallbackDomain {
            input: "plant.ahu1.TSup".to_owned()
        }
    );
}

#[test]
fn a_stale_map_after_reloading_another_model_is_refused() {
    let mut engine = load("generic_time_suppression");
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let frames = vec![(0.0, vec![Value::Real(295.0), Value::Real(293.0)])];
    // Reloading the same bytes keeps the map valid (same paths, types, bounds, order).
    engine
        .load_cxf(&fixtures::g36_bytes("generic_time_suppression"))
        .unwrap();
    let source = columnar(&map, &frames);
    assert!(
        Replay::new(
            engine,
            map.clone(),
            source,
            ReplayOptions::new(TimeBase::Recorded)
        )
        .is_ok()
    );
    // Another model: refused.
    let mut engine = load("generic_time_suppression");
    engine
        .load_cxf(&fixtures::g36_bytes("thermal_zones_zone_states"))
        .unwrap();
    let source = columnar(&map, &frames);
    assert!(matches!(
        Replay::new(
            engine,
            map.clone(),
            source,
            ReplayOptions::new(TimeBase::Recorded)
        ),
        Err(ReplayError::StaleMap)
    ));
    // No model at all: the facade's own refusal.
    let source = columnar(&map, &frames);
    assert!(matches!(
        Replay::new(
            oce_api::Engine::new(),
            map,
            source,
            ReplayOptions::new(TimeBase::Recorded)
        ),
        Err(ReplayError::Engine(_))
    ));
}

#[test]
fn a_source_bound_to_another_map_is_refused() {
    let engine = load("generic_time_suppression");
    let first = path_map(&engine, &MissingPolicy::Refuse);
    let second = path_map(&engine, &MissingPolicy::Refuse);
    assert_ne!(first.id(), second.id());
    assert_eq!(first.id(), first.clone().id());
    let steps = (0..2)
        .map(|s| (first.column(s).to_owned(), vec![(0.0, Value::Real(1.0))]))
        .collect();
    let source = StepSource::new(&first, steps, &oce_replay::ReplayLimits::default()).unwrap();
    assert!(matches!(
        Replay::new(
            engine,
            second,
            source,
            ReplayOptions::new(TimeBase::Recorded)
        ),
        Err(ReplayError::SourceNotBound)
    ));
}

#[test]
fn every_g36_fixture_maps_by_exact_path_and_the_controller_by_suffix() {
    let engine = load("cooling_only_controller");
    let map = path_map(&engine, &MissingPolicy::Refuse);
    assert_eq!(map.len(), 14);
    let specs = path_specs(&engine);
    let suffixes: Vec<ColumnSpec> = specs
        .iter()
        .map(|s| ColumnSpec::new(s.name.rsplit('.').next().unwrap(), s.kind.clone()))
        .collect();
    let by_suffix = InputMap::resolve(
        &engine.input_definitions().unwrap(),
        &suffixes,
        &ColumnMap::by_name(&suffixes),
        &MissingPolicy::Refuse,
        &[],
    )
    .unwrap();
    for slot in 0..14 {
        assert!(
            map.column(slot)
                .ends_with(&format!(".{}", by_suffix.column(slot)))
        );
    }
}
