//! The two in-repo library rules' vector files, read in place from the facade's fixtures
//! (`crates/oce-api/tests/fixtures/library_rules/`), load as `oce/vectors/v1` and hold what the
//! vectors note counts: 16 scenarios, 21 windows, 8 step-list inputs, every `equals` a Boolean,
//! and 217 and 441 frames.

mod support;

use oce_verify::{Expectation, Literal, VectorSet, VerifyLimits};

use support::{LIBRARY_RULES, library_vectors};

fn rule(id: &str) -> VectorSet {
    VectorSet::from_json_bytes(&library_vectors(id), &VerifyLimits::default())
        .unwrap_or_else(|error| panic!("{id}: {error}"))
}

fn windows(set: &VectorSet) -> Vec<(&str, &oce_verify::Window)> {
    set.scenarios()
        .iter()
        .flat_map(|scenario| {
            scenario
                .expectations()
                .iter()
                .map(move |expectation| match expectation {
                    Expectation::Window(window) => (scenario.name(), window),
                    other => panic!("{}: not a window: {other:?}", scenario.name()),
                })
        })
        .collect()
}

#[test]
fn both_files_load_with_the_counts_the_note_records() {
    let (ahu_0016, ahu_0004) = (rule("AHU-0016"), rule("AHU-0004"));
    assert_eq!(
        (ahu_0016.scenarios().len(), ahu_0004.scenarios().len()),
        (7, 9)
    );
    assert_eq!(
        (windows(&ahu_0016).len(), windows(&ahu_0004).len()),
        (8, 13)
    );
    let step_lists = |set: &VectorSet| {
        set.scenarios()
            .iter()
            .flat_map(|scenario| scenario.inputs())
            .filter(|input| !input.is_constant())
            .count()
    };
    assert_eq!(step_lists(&ahu_0016) + step_lists(&ahu_0004), 8);
    assert_eq!((step_lists(&ahu_0016), step_lists(&ahu_0004)), (1, 7));
    for set in [&ahu_0016, &ahu_0004] {
        assert_eq!(set.params(), None);
        for scenario in set.scenarios() {
            assert!(scenario.description().is_some_and(|text| !text.is_empty()));
            assert_eq!(scenario.params(), None);
            assert!(!scenario.expectations().is_empty());
        }
        for (name, window) in windows(set) {
            assert!(matches!(window.equals(), Literal::Boolean(_)), "{name}");
            assert_eq!(window.output(), "yFault", "{name}");
        }
    }
}

#[test]
fn the_clocks_give_the_frame_counts_the_note_records() {
    let (ahu_0016, ahu_0004) = (rule("AHU-0016"), rule("AHU-0004"));
    let bits = |set: &VectorSet| {
        (
            set.clock().step_s().to_bits(),
            set.clock().horizon_s().to_bits(),
        )
    };
    assert_eq!(bits(&ahu_0016), (60.0_f64.to_bits(), 1800.0_f64.to_bits()));
    assert_eq!(ahu_0016.clock().tick_count(), 31);
    let frames = |set: &VectorSet| {
        set.clock().tick_count() * u64::try_from(set.scenarios().len()).expect("fits")
    };
    assert_eq!(frames(&ahu_0016), 217);
    assert_eq!(
        bits(&ahu_0004),
        (300.0_f64.to_bits(), 14_400.0_f64.to_bits())
    );
    assert_eq!(ahu_0004.clock().tick_count(), 49);
    assert_eq!(frames(&ahu_0004), 441);
    // The instants a run of AHU-0016 checks: every tick of every window, 216 of the 217 frames
    // (the tick at 900 s of `both_open` is in neither of its windows).
    let instants: u64 = windows(&ahu_0016)
        .iter()
        .map(|(_, window)| window.last_tick() - window.first_tick() + 1)
        .sum();
    assert_eq!(instants, 216);
}

#[test]
fn the_persistence_rule_holds_its_scenarios_in_file_order_with_their_windows() {
    let set = rule("AHU-0016");
    let names: Vec<&str> = set
        .scenarios()
        .iter()
        .map(|scenario| scenario.name())
        .collect();
    assert_eq!(
        names,
        [
            "normal_heating",
            "normal_cooling",
            "both_open",
            "both_below_threshold",
            "deadband",
            "threshold_edge",
            "transient_overlap"
        ]
    );
    let both_open = &set.scenarios()[2];
    let inputs: Vec<(&str, Literal)> = both_open
        .inputs()
        .iter()
        .map(|input| (input.name(), input.steps()[0].value()))
        .collect();
    assert_eq!(
        inputs,
        [
            ("clg_vlv_cmd", Literal::Real(30.0)),
            ("htg_vlv_cmd", Literal::Real(20.0))
        ]
    );
    let ticks: Vec<(u64, u64, Literal)> = windows(&set)
        .iter()
        .filter(|(name, _)| *name == "both_open")
        .map(|(_, window)| (window.first_tick(), window.last_tick(), window.equals()))
        .collect();
    assert_eq!(
        ticks,
        [
            (0, 14, Literal::Boolean(false)),
            (16, 30, Literal::Boolean(true))
        ]
    );
    // The one step list: the cooling valve closes at 600 s.
    let transient = &set.scenarios()[6];
    let clg = &transient.inputs()[0];
    assert_eq!((clg.name(), clg.is_constant()), ("clg_vlv_cmd", false));
    let steps: Vec<(u64, Literal)> = clg
        .steps()
        .iter()
        .map(|step| (step.time_s().to_bits(), step.value()))
        .collect();
    assert_eq!(
        steps,
        [
            (0.0_f64.to_bits(), Literal::Real(30.0)),
            (600.0_f64.to_bits(), Literal::Real(0.0))
        ]
    );
}

#[test]
fn the_moving_window_rule_writes_integer_tokens_for_its_integer_input() {
    let set = rule("AHU-0004");
    for scenario in set.scenarios() {
        assert_eq!(scenario.inputs().len(), 1, "{}", scenario.name());
        let input = &scenario.inputs()[0];
        assert_eq!(input.name(), "operating_state");
        for step in input.steps() {
            assert!(
                matches!(step.value(), Literal::Integer(_)),
                "{}",
                scenario.name()
            );
        }
    }
    let longest = set
        .scenarios()
        .iter()
        .map(|scenario| scenario.inputs()[0].steps().len())
        .max();
    assert_eq!(longest, Some(49), "one step on every tick of the clock");
}

#[test]
fn neither_file_is_canonical_and_both_load_all_the_same() {
    for id in LIBRARY_RULES {
        let bytes = library_vectors(id);
        let set = rule(id);
        assert_ne!(set.to_canonical_json().as_bytes(), bytes, "{id}");
        assert!(
            bytes.starts_with(b"{\n  \"schema\": \"oce/vectors/v1\",\n"),
            "{id}"
        );
    }
}

#[test]
fn the_same_bytes_under_the_library_tag_are_refused_by_name() {
    for id in LIBRARY_RULES {
        let text = String::from_utf8(library_vectors(id)).expect("UTF-8");
        for tag in [
            "cxf-library/vectors/v1",
            "cxf-library/vectors/v2",
            "oce/vectors/v2",
        ] {
            let retagged = text.replacen("oce/vectors/v1", tag, 1);
            assert_ne!(retagged, text);
            let refused = VectorSet::from_json_bytes(retagged.as_bytes(), &VerifyLimits::default())
                .expect_err("another tag is another format");
            assert_eq!(refused.code(), "unknown-schema", "{id} as {tag}");
        }
    }
}
