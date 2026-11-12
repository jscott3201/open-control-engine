//! The rules a shape cannot say, through whole documents: the clock's cadence, step lists,
//! windows and diagnostic instants (vectors note, "Clock", "Inputs", "Windows", "Diagnostic at an
//! instant"; decisions 3, 4, 11 and 12).

mod support;

use oce_verify::{Expectation, VectorSet};

use support::{code_of, load, one, refusal, scenario, set_on};

/// The shortest decimal spelling of a binary64, which reads back to the same bits.
fn spell(value: f64) -> String {
    format!("{value:?}")
}

fn window(from_s: &str, to_s: &str) -> String {
    format!("{{\"output\": \"y\", \"from_s\": {from_s}, \"to_s\": {to_s}, \"equals\": true}}")
}

fn instant(at_s: &str) -> String {
    format!("{{\"diagnostic\": {{\"block\": \"B\"}}, \"at_s\": {at_s}}}")
}

fn steps(times: &[&str]) -> String {
    let list: Vec<String> = times
        .iter()
        .map(|t| format!("{{\"t\": {t}, \"value\": 1}}"))
        .collect();
    format!("\"u\": [{}]", list.join(", "))
}

/// A one-scenario set on the clock `{ step_s: 0.1, horizon_s: 4.3 }`.
fn tenths(inputs: &str, expect: &str) -> String {
    set_on("0.1", "4.3", &scenario(inputs, expect))
}

fn ticks_of(set: &VectorSet, expectation: usize) -> (u64, u64) {
    match &set.scenarios()[0].expectations()[expectation] {
        Expectation::Window(window) => (window.first_tick(), window.last_tick()),
        Expectation::Diagnostic(instant) => (instant.tick(), instant.tick()),
        other => panic!("an expectation this test does not know: {other:?}"),
    }
}

#[test]
fn a_clock_whose_quotient_floors_short_is_on_the_cadence() {
    let set = load(&set_on("0.1", "4.3", "")).expect("43 steps of 0.1 are exactly 4.3");
    let clock = set.clock();
    assert_eq!((clock.last_tick(), clock.tick_count()), (43, 44));
    assert_eq!(clock.step_s().to_bits(), 0.1_f64.to_bits());
    assert_eq!(clock.horizon_s().to_bits(), 4.3_f64.to_bits());
    assert_eq!(
        clock.tick_time(43).map(f64::to_bits),
        Some(4.3_f64.to_bits())
    );
    // Tick 3 is the product, not the decimal 0.3.
    assert_eq!(
        clock.tick_time(3).map(f64::to_bits),
        Some(0.300_000_000_000_000_04_f64.to_bits())
    );
    assert_ne!(
        clock.tick_time(3).map(f64::to_bits),
        Some(0.3_f64.to_bits())
    );
}

#[test]
fn a_horizon_one_ulp_off_the_cadence_is_refused() {
    for off in [4.3_f64.next_up(), 4.3_f64.next_down()] {
        assert_eq!(
            refusal(&set_on("0.1", &spell(off), "")),
            ("horizon-off-cadence", "clock.horizon_s".to_owned()),
            "{off:?}"
        );
    }
    for off in [1800.0_f64.next_up(), 1800.0_f64.next_down()] {
        assert_eq!(
            code_of(&set_on("60", &spell(off), "")),
            "horizon-off-cadence"
        );
    }
    for horizon in ["1799", "1801", "1830", "30", "59.999", "1e-3"] {
        assert_eq!(
            code_of(&set_on("60", horizon, "")),
            "horizon-off-cadence",
            "{horizon}"
        );
    }
}

#[test]
fn a_clock_loads_however_its_numbers_are_spelled() {
    for (step, horizon, ticks) in [
        ("60", "1800", 31),
        ("60.0", "1800", 31),
        ("6e1", "1.8E3", 31),
        ("300", "14400", 49),
        ("60", "0", 1),
        ("60", "-0", 1),
        ("60", "-0.0", 1),
        ("1e-3", "0.001", 2),
        ("5e-324", "5e-324", 2),
        ("1.5e300", "3e300", 3),
        ("0.25", "16", 65),
        ("1", "16777215", 16_777_216),
    ] {
        let set =
            load(&set_on(step, horizon, "")).unwrap_or_else(|e| panic!("{step}/{horizon}: {e}"));
        assert_eq!(set.clock().tick_count(), ticks, "{step} / {horizon}");
        assert_eq!(set.clock().last_tick() + 1, ticks);
    }
}

#[test]
fn a_step_that_is_not_positive_and_a_negative_horizon_are_invalid_clocks() {
    for step in ["0", "0.0", "-0", "-0.0", "-60", "-1e-300", "0e5"] {
        assert_eq!(
            refusal(&set_on(step, "1800", "")),
            ("invalid-clock", "clock.step_s".to_owned()),
            "{step}"
        );
    }
    for horizon in ["-1", "-60", "-1e-300", "-1800.0"] {
        assert_eq!(
            refusal(&set_on("60", horizon, "")),
            ("invalid-clock", "clock.horizon_s".to_owned()),
            "{horizon}"
        );
    }
    // A step that underflows to zero is zero.
    assert_eq!(code_of(&set_on("1e-400", "1", "")), "invalid-clock");
    // The step is judged before the horizon, and the clock before any scenario.
    assert_eq!(refusal(&set_on("0", "-1", "")).1, "clock.step_s");
    assert_eq!(
        code_of(&set_on("0", "60", &scenario("\"u\": []", ""))),
        "invalid-clock"
    );
}

#[test]
fn the_tick_bound_is_checked_from_the_quotient_at_load() {
    assert_eq!(code_of(&set_on("1", "16777215", "")), "loaded");
    assert_eq!(
        refusal(&set_on("1", "16777216", "")),
        ("too-many-ticks", "clock".to_owned())
    );
    // Past the bound and off the cadence is still the bound's refusal.
    for (step, horizon) in [
        ("1", "16777216.5"),
        ("1e-9", "1e9"),
        ("5e-324", "1e308"),
        ("60", "1e300"),
        ("0.001", "16777.216"),
    ] {
        assert_eq!(
            code_of(&set_on(step, horizon, "")),
            "too-many-ticks",
            "{step}/{horizon}"
        );
    }
    // Inside the bound and off the cadence is the cadence's refusal.
    assert_eq!(
        code_of(&set_on("1", "16777214.5", "")),
        "horizon-off-cadence"
    );
}

#[test]
fn an_integer_time_past_two_to_the_fifty_third_is_refused_in_every_time_position() {
    // 2^53 is the last integer below which every integer is a binary64.
    const EXACT: &str = "9007199254740992";
    let big = set_on(
        EXACT,
        EXACT,
        &scenario(
            &steps(&["0", EXACT]),
            &format!("{}, {}", window(EXACT, EXACT), instant(EXACT)),
        ),
    );
    let set = load(&big).expect("2^53 is exact in every time position");
    assert_eq!(set.clock().tick_count(), 2);
    assert_eq!((ticks_of(&set, 0), ticks_of(&set, 1)), ((1, 1), (1, 1)));

    for past in [
        "9007199254740993",
        "9007199254740994",
        "-9007199254740993",
        "9223372036854775807",
    ] {
        let positions = [
            set_on(past, EXACT, ""),
            set_on(EXACT, past, ""),
            set_on(EXACT, EXACT, &scenario(&steps(&["0", past]), "")),
            set_on(EXACT, EXACT, &scenario("", &window(past, EXACT))),
            set_on(EXACT, EXACT, &scenario("", &window("0", past))),
            set_on(EXACT, EXACT, &scenario("", &instant(past))),
        ];
        for document in positions {
            let refused = load(&document).expect_err("refused");
            assert_eq!(refused.code(), "integer-out-of-range", "{past}: {refused}");
            assert!(refused.at().is_some(), "named at the token");
        }
    }
    // -2^53 is in range as a token; what it means is then judged like any time.
    assert_eq!(
        code_of(&set_on(EXACT, "-9007199254740992", "")),
        "invalid-clock"
    );
    // The same magnitude written as a real is a real: it rounds as every real does.
    let real = "9007199254740994.0";
    assert_eq!(code_of(&set_on(real, real, "")), "loaded");
    // A value is not a time: an integer token keeps all 64 bits.
    let values = one("\"u\": 9007199254740993", "");
    assert_eq!(code_of(&values), "loaded");
}

#[test]
fn a_step_list_runs_from_zero_strictly_upward_and_never_past_the_horizon() {
    // On a tick, between ticks, and at the horizon.
    for list in [
        vec!["0"],
        vec!["0", "60"],
        vec!["0", "90"],
        vec!["0", "0.001", "59.999", "60", "1799.5", "1800"],
        vec!["0.0", "1e-300"],
        vec!["-0.0", "1800"],
        vec!["-0", "1"],
    ] {
        assert_eq!(code_of(&one(&steps(&list), "")), "loaded", "{list:?}");
    }
    let subject = |index: usize| format!("scenarios[0].inputs.u[{index}].t");
    // Not from zero.
    for first in ["60", "0.5", "1e-300", "-60", "-1e-300", "1800"] {
        assert_eq!(
            refusal(&one(&steps(&[first, "1800"]), "")),
            ("step-list-not-from-zero", subject(0)),
            "{first}"
        );
    }
    // Not increasing: equal, decreasing, and back to zero.
    for (list, at) in [
        (vec!["0", "0"], 1),
        (vec!["0", "60", "60"], 2),
        (vec!["0", "60", "30"], 2),
        (vec!["0", "60", "120", "0"], 3),
        (vec!["0", "-60"], 1),
        (vec!["0", "-0.0"], 1),
        (vec!["0", "60", "60.0"], 2),
    ] {
        assert_eq!(
            refusal(&one(&steps(&list), "")),
            ("step-times-not-increasing", subject(at)),
            "{list:?}"
        );
    }
    // After the horizon, by a step and by one ulp.
    let ulp_past = spell(1800.0_f64.next_up());
    for (list, at) in [
        (vec!["0", "1801"], 1),
        (vec!["0", "1800", "1860"], 2),
        (vec!["0", ulp_past.as_str()], 1),
        (vec!["0", "1e300"], 1),
    ] {
        assert_eq!(
            refusal(&one(&steps(&list), "")),
            ("step-after-horizon", subject(at)),
            "{list:?}"
        );
    }
    // An out-of-order step past the horizon is out of order first.
    assert_eq!(
        code_of(&one(&steps(&["0", "1900", "1850"]), "")),
        "step-after-horizon"
    );
    assert_eq!(
        code_of(&one(&steps(&["0", "100", "50", "1900"]), "")),
        "step-times-not-increasing"
    );
    // An empty list is no input.
    assert_eq!(
        refusal(&one("\"u\": []", "")),
        ("step-list-empty", "scenarios[0].inputs.u".to_owned())
    );
}

#[test]
fn a_step_list_on_a_one_tick_clock_holds_only_its_step_at_zero() {
    let at_zero = set_on("60", "0", &scenario(&steps(&["0"]), ""));
    assert_eq!(code_of(&at_zero), "loaded");
    let later = set_on("60", "0", &scenario(&steps(&["0", "1"]), ""));
    assert_eq!(code_of(&later), "step-after-horizon");
}

#[test]
fn a_step_keeps_its_time_and_value_and_a_one_step_list_stays_a_list() {
    let set = load(&one(
        "\"u\": [{\"t\": 0, \"value\": 1}, {\"t\": 90.5, \"value\": 2.5}, {\"t\": 1800, \"value\": true}], \"v\": [{\"t\": -0.0, \"value\": 7}]",
        "",
    ))
    .expect("loads");
    let inputs = set.scenarios()[0].inputs();
    assert!(!inputs[0].is_constant() && !inputs[1].is_constant());
    let times: Vec<u64> = inputs[0]
        .steps()
        .iter()
        .map(|s| s.time_s().to_bits())
        .collect();
    assert_eq!(
        times,
        [0.0_f64.to_bits(), 90.5_f64.to_bits(), 1800.0_f64.to_bits()]
    );
    assert_eq!(inputs[0].steps()[1].value(), oce_verify::Literal::Real(2.5));
    assert_eq!(
        inputs[0].steps()[2].value(),
        oce_verify::Literal::Boolean(true)
    );
    // A step at negative zero is at zero: a time carries no sign.
    assert_eq!(inputs[1].steps()[0].time_s().to_bits(), 0.0_f64.to_bits());
    assert_eq!(inputs[1].steps().len(), 1);
    // A constant and a one-step list are the same schedule and different spellings.
    let constant = load(&one("\"u\": 7", "")).expect("loads");
    let listed = load(&one("\"u\": [{\"t\": 0, \"value\": 7}]", "")).expect("loads");
    assert_eq!(
        constant.scenarios()[0].inputs()[0].steps(),
        listed.scenarios()[0].inputs()[0].steps()
    );
    assert_ne!(constant, listed);
}

#[test]
fn a_window_touching_a_tick_at_either_bound_holds_it() {
    let set = load(&one(
        "",
        &[
            window("0", "1800"),
            window("30", "60"),
            window("60", "90"),
            window("60", "60"),
            window("1800", "1800"),
            window("0", "0"),
            window("0.0", "59.999"),
            window("1740.001", "1800.0"),
            window("-0.0", "-0"),
        ]
        .join(", "),
    ))
    .expect("every window holds a tick");
    let ticks: Vec<(u64, u64)> = (0..9).map(|index| ticks_of(&set, index)).collect();
    assert_eq!(
        ticks,
        [
            (0, 30),
            (1, 1),
            (1, 1),
            (1, 1),
            (30, 30),
            (0, 0),
            (0, 0),
            (30, 30),
            (0, 0)
        ]
    );
}

#[test]
fn a_window_between_two_ticks_is_refused() {
    let subject = "scenarios[0].expect[0]".to_owned();
    let below = spell(60.0_f64.next_down());
    let above = spell(60.0_f64.next_up());
    for (from, to) in [
        ("61", "119"),
        ("0.001", "59.999"),
        ("30", below.as_str()),
        (above.as_str(), "90"),
        ("90", "90"),
        ("1799.5", "1799.9"),
    ] {
        assert_eq!(
            refusal(&one("", &window(from, to))),
            ("window-uncovered", subject.clone()),
            "[{from}, {to}]"
        );
    }
    // One ulp wider at the tick, and the window holds it.
    assert_eq!(code_of(&one("", &window("30", "60"))), "loaded");
    assert_eq!(code_of(&one("", &window("60", "90"))), "loaded");
}

#[test]
fn a_window_lies_inside_the_clock_in_order() {
    let subject = "scenarios[0].expect[0]".to_owned();
    for (from, to) in [
        ("120", "60"),
        ("1800", "0"),
        ("0.001", "0"),
        ("60", "59.999"),
    ] {
        assert_eq!(
            refusal(&one("", &window(from, to))),
            ("invalid-window", subject.clone()),
            "[{from}, {to}]"
        );
    }
    let ulp_past = spell(1800.0_f64.next_up());
    for (from, to) in [
        ("0", "1860"),
        ("0", ulp_past.as_str()),
        ("-60", "60"),
        ("-1e-300", "60"),
        ("-120", "-60"),
        ("1860", "1920"),
        ("0", "1e300"),
    ] {
        assert_eq!(
            refusal(&one("", &window(from, to))),
            ("window-outside-clock", subject.clone()),
            "[{from}, {to}]"
        );
    }
    // Reversed is judged before outside, and outside before uncovered.
    assert_eq!(code_of(&one("", &window("1900", "1850"))), "invalid-window");
    assert_eq!(
        code_of(&one("", &window("1801", "1859"))),
        "window-outside-clock"
    );
    // The whole clock, to the last bit of the horizon, is inside.
    assert_eq!(code_of(&one("", &window("0", "1800.0"))), "loaded");
}

#[test]
fn a_window_is_decided_from_tick_products_not_from_decimals() {
    // Tick 3 of a 0.1 s clock is 0.30000000000000004, so a window ending at the decimal 0.3
    // does not reach it, and one starting there does not start at tick 3's decimal neighbour.
    let set = load(&tenths(
        "",
        &[
            window("0.25", "0.35"),
            window("0.30000000000000004", "0.30000000000000004"),
            window("0.2", "0.3"),
            window("0.3", "0.4"),
            window("4.25", "4.3"),
            window("0", "4.3"),
        ]
        .join(", "),
    ))
    .expect("loads");
    let ticks: Vec<(u64, u64)> = (0..6).map(|index| ticks_of(&set, index)).collect();
    assert_eq!(ticks, [(3, 3), (3, 3), (2, 2), (3, 4), (43, 43), (0, 43)]);
    assert_eq!(
        code_of(&tenths("", &window("0.3", "0.3"))),
        "window-uncovered"
    );
    assert_eq!(
        code_of(&tenths("", &window("0.21", "0.3"))),
        "window-uncovered"
    );
}

#[test]
fn a_diagnostic_instant_is_a_tick_of_the_clock() {
    let set = load(&one(
        "",
        &[
            instant("0"),
            instant("60"),
            instant("1800"),
            instant("6e1"),
            instant("-0.0"),
        ]
        .join(", "),
    ))
    .expect("every instant is a tick");
    let ticks: Vec<u64> = (0..5).map(|index| ticks_of(&set, index).0).collect();
    assert_eq!(ticks, [0, 1, 30, 1, 0]);

    let below = spell(60.0_f64.next_down());
    let above = spell(60.0_f64.next_up());
    for at in [
        "90",
        "30",
        "0.001",
        "1860",
        "1e300",
        "-60",
        "-1e-300",
        below.as_str(),
        above.as_str(),
    ] {
        assert_eq!(
            refusal(&one("", &instant(at))),
            (
                "instant-off-cadence",
                "scenarios[0].expect[0].at_s".to_owned()
            ),
            "{at}"
        );
    }
    // On a 0.1 s clock the decimal 0.3 is between ticks and the product is tick 3.
    assert_eq!(code_of(&tenths("", &instant("0.3"))), "instant-off-cadence");
    let exact = load(&tenths("", &instant("0.30000000000000004"))).expect("tick 3");
    assert_eq!(ticks_of(&exact, 0), (3, 3));
    assert_eq!(
        ticks_of(&load(&tenths("", &instant("4.3"))).expect("the horizon"), 0),
        (43, 43)
    );
}

#[test]
fn the_first_refusal_follows_the_documented_order() {
    // The clock, then the set's params, then scenarios in file order; in a scenario its name,
    // its params, its inputs in file order, its expectations in file order.
    let bad_step = steps(&["60"]);
    let bad_window = window("120", "60");
    let second =
        format!("{{\"name\": \"t\", \"inputs\": {{{bad_step}}}, \"expect\": [{bad_window}]}}");
    let first_ok = scenario("", "");
    assert_eq!(
        refusal(&support::set_of(&format!("{first_ok}, {second}"))),
        (
            "step-list-not-from-zero",
            "scenarios[1].inputs.u[0].t".to_owned()
        )
    );
    let inputs_in_file_order = one("\"z\": [], \"a\": [{\"t\": 5, \"value\": 1}]", "");
    assert_eq!(refusal(&inputs_in_file_order).1, "scenarios[0].inputs.z");
    let expectations_in_file_order = one("", &format!("{}, {bad_window}", instant("90")));
    assert_eq!(
        refusal(&expectations_in_file_order).0,
        "instant-off-cadence"
    );
    let name_first = support::set_of(&format!(
        "{{\"name\": \"\", \"params\": {{\"\": 1}}, \"inputs\": {{{bad_step}}}, \"expect\": []}}"
    ));
    assert_eq!(refusal(&name_first).1, "scenarios[0].name");
    let params_before_inputs = support::set_of(&format!(
        "{{\"name\": \"s\", \"inputs\": {{{bad_step}}}, \"params\": {{\"\": 1}}, \"expect\": []}}"
    ));
    assert_eq!(refusal(&params_before_inputs).1, "scenarios[0].params");
}
