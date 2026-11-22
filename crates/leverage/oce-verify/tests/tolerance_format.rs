//! The tolerance keys of `oce/vectors/v1` at load (vectors note, "Tolerance windows"; decisions
//! 39, 41 and 46): `within`, `of` and `when` on a window and `envelope` on the set, each refused
//! at its bound with its own code and place, and what a loaded set holds and writes.

mod support;

use oce_verify::{Envelope, Expectation, Literal, Tolerances, VectorSet, VerifyLimits, Window};

use support::{code_of, load, one, refusal, set_of};

/// A tolerance window on `y` over 60 s to 120 s with the given `within`, `of` and trailing
/// members, as the only expectation of a set on the 60 s clock.
fn windowed(within: &str, of: &str, rest: &str) -> String {
    one(
        "",
        &format!(
            r#"{{"output": "y", "from_s": 60, "to_s": 120, "within": {within}, "of": {of}{rest}}}"#
        ),
    )
}

/// The first window of a loaded set.
fn first_window(set: &VectorSet) -> &Window {
    match &set.scenarios()[0].expectations()[0] {
        Expectation::Window(window) => window,
        other => panic!("a window, found {other:?}"),
    }
}

/// `document` with `envelope` written as the set's first member.
fn enveloped(envelope: &str) -> String {
    set_of("").replacen('{', &format!("{{\"envelope\": {envelope}, "), 1)
}

#[test]
fn a_tolerance_window_holds_its_tolerances_signal_and_mask() {
    let set = load(&windowed(
        r#"{"rtoly": 0.5, "atolx": 30}"#,
        r#"[{"t": 60, "value": 1}, {"t": 90, "value": 2.5}]"#,
        r#", "when": ["b", "u.on"]"#,
    ))
    .expect("loads");
    let window = first_window(&set);
    assert_eq!(window.equals(), None);
    let within = window.within().expect("a tolerance window");
    assert_eq!(
        within.tolerances(),
        Tolerances {
            atolx: 30.0,
            rtoly: 0.5,
            ..Tolerances::ZERO
        }
    );
    assert_eq!(within.names().collect::<Vec<_>>(), ["atolx", "rtoly"]);
    assert!(!within.of_is_constant());
    let of: Vec<(f64, Literal)> = within
        .of()
        .iter()
        .map(|s| (s.time_s(), s.value()))
        .collect();
    assert_eq!(
        of,
        [(60.0, Literal::Integer(1)), (90.0, Literal::Real(2.5))]
    );
    assert_eq!(window.when(), ["b", "u.on"]);
    assert_eq!((window.first_tick(), window.last_tick()), (1, 2));
    assert!(set.uses_tolerance());
    // A number is one step at the window's from_s.
    let set = load(&windowed(r#"{"atoly": 1}"#, "-3", "")).expect("loads");
    let within = first_window(&set).within().expect("a tolerance window");
    assert!(within.of_is_constant());
    assert_eq!(within.of().len(), 1);
    assert_eq!(within.of()[0].time_s(), 60.0);
    assert_eq!(within.of()[0].value(), Literal::Integer(-3));
    // An exact window has no tolerance and, without `when`, no mask.
    let exact = load(&one(
        "",
        r#"{"output": "y", "from_s": 0, "to_s": 60, "equals": 1.5}"#,
    ))
    .expect("loads");
    let window = first_window(&exact);
    assert_eq!(window.equals(), Some(Literal::Real(1.5)));
    assert!(window.within().is_none() && window.when().is_empty());
    assert!(!exact.uses_tolerance());
}

#[test]
fn each_tolerance_name_alone_is_a_within() {
    for name in ["atolx", "atoly", "ltolx", "ltoly", "rtolx", "rtoly"] {
        let set = load(&windowed(&format!(r#"{{"{name}": 0.5}}"#), "1", "")).expect(name);
        let tolerances = first_window(&set).within().expect("within").tolerances();
        let named = [
            ("atolx", tolerances.atolx),
            ("atoly", tolerances.atoly),
            ("ltolx", tolerances.ltolx),
            ("ltoly", tolerances.ltoly),
            ("rtolx", tolerances.rtolx),
            ("rtoly", tolerances.rtoly),
        ];
        for (other, value) in named {
            assert_eq!(
                value,
                if other == name { 0.5 } else { 0.0 },
                "{name}: {other}"
            );
        }
    }
}

#[test]
fn a_tolerance_is_finite_not_negative_and_ltolx_at_most_one() {
    let place = "scenarios[0].expect[0].within";
    for (within, code, at) in [
        ("{}", "invalid-tolerance", place.to_owned()),
        (
            r#"{"atoly": -1e-300}"#,
            "invalid-tolerance",
            format!("{place}.atoly"),
        ),
        (
            r#"{"atolx": -1}"#,
            "invalid-tolerance",
            format!("{place}.atolx"),
        ),
        (
            r#"{"rtoly": -5e-324}"#,
            "invalid-tolerance",
            format!("{place}.rtoly"),
        ),
        (
            r#"{"ltolx": 1.0000000000000002}"#,
            "invalid-tolerance",
            format!("{place}.ltolx"),
        ),
        (
            r#"{"ltolx": 2}"#,
            "invalid-tolerance",
            format!("{place}.ltolx"),
        ),
        (
            r#"{"ltolx": 1e308}"#,
            "invalid-tolerance",
            format!("{place}.ltolx"),
        ),
        // The first bad name in the order the six are named wins.
        (
            r#"{"rtoly": -1, "atolx": -1}"#,
            "invalid-tolerance",
            format!("{place}.atolx"),
        ),
    ] {
        assert_eq!(
            refusal(&windowed(within, "1", "")),
            (code, at.clone()),
            "{within}"
        );
    }
    // Lexical refusals come first, at the token.
    for (within, code) in [
        (r#"{"atoly": 1e999}"#, "non-finite-number"),
        (r#"{"atoly": NaN}"#, "non-finite-number"),
        (r#"{"atoly": -Infinity}"#, "non-finite-number"),
        (r#"{"atoly": 9007199254740993}"#, "integer-out-of-range"),
        (r#"{"ltolx": -9007199254740993}"#, "integer-out-of-range"),
        (r#"{"atoly": "0.5"}"#, "wrong-type"),
        (r#"{"atoly": true}"#, "wrong-type"),
        (r#"{"atoly": null}"#, "wrong-type"),
        (r#"{"atol": 1}"#, "unknown-key"),
        ("[1]", "wrong-type"),
        ("0.5", "wrong-type"),
    ] {
        assert_eq!(code_of(&windowed(within, "1", "")), code, "{within}");
    }
    // At each bound the tolerance loads: zero of both signs, ltolx exactly 1, the largest finite
    // real and the integer 2^53.
    for within in [
        r#"{"atoly": 0}"#,
        r#"{"atoly": -0}"#,
        r#"{"atoly": -0.0}"#,
        r#"{"ltolx": 1}"#,
        r#"{"ltolx": 1.0}"#,
        r#"{"atoly": 1.7976931348623157e308}"#,
        r#"{"atolx": 9007199254740992}"#,
        r#"{"atoly": 5e-324, "atolx": 0, "ltolx": 0, "ltoly": 0, "rtolx": 0, "rtoly": 0}"#,
    ] {
        assert_eq!(code_of(&windowed(within, "1", "")), "loaded", "{within}");
    }
    // A negative zero reads as zero.
    let set = load(&windowed(r#"{"atoly": -0.0}"#, "1", "")).expect("loads");
    let atoly = first_window(&set)
        .within()
        .expect("within")
        .tolerances()
        .atoly;
    assert_eq!(atoly.to_bits(), 0.0_f64.to_bits());
}

#[test]
fn an_of_step_list_runs_from_the_window_start_to_no_later_than_its_end() {
    let place = "scenarios[0].expect[0].of";
    for (of, code, at) in [
        ("[]", "step-list-empty", place.to_owned()),
        (
            r#"[{"t": 0, "value": 1}]"#,
            "of-not-from-window-start",
            format!("{place}[0].t"),
        ),
        (
            r#"[{"t": 60.000000000000014, "value": 1}]"#,
            "of-not-from-window-start",
            format!("{place}[0].t"),
        ),
        (
            r#"[{"t": 60, "value": 1}, {"t": 60, "value": 2}]"#,
            "step-times-not-increasing",
            format!("{place}[1].t"),
        ),
        (
            r#"[{"t": 60, "value": 1}, {"t": 90, "value": 2}, {"t": 80, "value": 3}]"#,
            "step-times-not-increasing",
            format!("{place}[2].t"),
        ),
        (
            r#"[{"t": 60, "value": 1}, {"t": 120.00000000000001, "value": 2}]"#,
            "of-step-after-window",
            format!("{place}[1].t"),
        ),
        (
            r#"[{"t": 60, "value": 1}, {"t": 1800, "value": 2}]"#,
            "of-step-after-window",
            format!("{place}[1].t"),
        ),
    ] {
        assert_eq!(
            refusal(&windowed(r#"{"atoly": 1}"#, of, "")),
            (code, at),
            "{of}"
        );
    }
    // On a tick, between two, at to_s exactly, and `60.0` for a window written from `60`.
    for of in [
        r#"[{"t": 60.0, "value": 1}]"#,
        r#"[{"t": 60, "value": 1}, {"t": 90, "value": 2}]"#,
        r#"[{"t": 60, "value": 1}, {"t": 120, "value": 2}]"#,
        r#"[{"t": 60, "value": 1}, {"t": 61, "value": 2}, {"t": 119.5, "value": 3}]"#,
    ] {
        assert_eq!(
            code_of(&windowed(r#"{"atoly": 1}"#, of, "")),
            "loaded",
            "{of}"
        );
    }
}

#[test]
fn an_of_is_a_number_or_a_step_list_of_numbers_and_nothing_else() {
    // Decision 39, read for a boolean as well as a string (orchestrator's reading, issue #684).
    for of in [
        "true",
        "\"yRef\"",
        "null",
        "{}",
        r#"{"t": 60, "value": 1}"#,
        "[1]",
        r#"[[60, 1]]"#,
        r#"[{"t": 60, "value": true}]"#,
        r#"[{"t": 60, "value": "1"}]"#,
        r#"[{"t": "60", "value": 1}]"#,
    ] {
        assert_eq!(
            code_of(&windowed(r#"{"atoly": 1}"#, of, "")),
            "wrong-type",
            "{of}"
        );
    }
    for (of, code) in [
        (r#"[{"t": 60}]"#, "missing-key"),
        (r#"[{"value": 1}]"#, "missing-key"),
        (r#"[{"t": 60, "value": 1, "v": 2}]"#, "unknown-key"),
        ("9007199254740993", "integer-out-of-range"),
        (
            r#"[{"t": 60, "value": -9007199254740993}]"#,
            "integer-out-of-range",
        ),
        ("1e999", "non-finite-number"),
    ] {
        assert_eq!(code_of(&windowed(r#"{"atoly": 1}"#, of, "")), code, "{of}");
    }
    assert_eq!(
        code_of(&windowed(r#"{"atoly": 1}"#, "9007199254740992", "")),
        "loaded"
    );
}

#[test]
fn a_when_names_one_to_sixteen_signals_on_either_window_form() {
    let names = |count: usize| {
        let list: Vec<String> = (0..count).map(|index| format!("\"s{index}\"")).collect();
        format!(r#", "when": [{}]"#, list.join(", "))
    };
    let exact = |rest: &str| {
        one(
            "",
            &format!(r#"{{"output": "y", "from_s": 0, "to_s": 60, "equals": true{rest}}}"#),
        )
    };
    for document in [
        windowed(r#"{"atoly": 1}"#, "1", &names(1)),
        windowed(r#"{"atoly": 1}"#, "1", &names(16)),
        exact(&names(1)),
        exact(&names(16)),
    ] {
        assert_eq!(code_of(&document), "loaded");
    }
    assert_eq!(
        refusal(&exact(", \"when\": []")),
        ("when-empty", "scenarios[0].expect[0].when".to_owned())
    );
    assert_eq!(
        refusal(&windowed(r#"{"atoly": 1}"#, "1", r#", "when": ["b", ""]"#)),
        ("empty-name", "scenarios[0].expect[0].when[1]".to_owned())
    );
    for document in [
        exact(&names(17)),
        windowed(r#"{"atoly": 1}"#, "1", &names(17)),
    ] {
        assert_eq!(code_of(&document), "too-many-when-signals");
    }
    for rest in [
        r#", "when": "b""#,
        r#", "when": [true]"#,
        r#", "when": [["b"]]"#,
    ] {
        assert_eq!(code_of(&exact(rest)), "wrong-type", "{rest}");
    }
    // The bound is lowerable, never raisable.
    let lowered = VerifyLimits {
        max_when_signals: 2,
        ..VerifyLimits::default()
    };
    let at = |count: usize| {
        VectorSet::from_json_bytes(exact(&names(count)).as_bytes(), &lowered)
            .map(|_| ())
            .map_err(|error| error.code())
    };
    assert_eq!(at(2), Ok(()));
    assert_eq!(at(3), Err("too-many-when-signals"));
    let raised = VerifyLimits {
        max_when_signals: 1_000,
        ..VerifyLimits::default()
    };
    assert_eq!(
        VectorSet::from_json_bytes(exact(&names(17)).as_bytes(), &raised)
            .map_err(|error| error.code())
            .err(),
        Some("too-many-when-signals")
    );
}

#[test]
fn an_expectation_is_exactly_one_of_three_forms() {
    for expectation in [
        r#"{"output": "y", "from_s": 0, "to_s": 60, "equals": 1, "within": {"atoly": 1}, "of": 1}"#,
        r#"{"output": "y", "from_s": 0, "to_s": 60, "equals": 1, "within": {"atoly": 1}}"#,
        r#"{"output": "y", "from_s": 0, "to_s": 60, "equals": 1, "of": 1}"#,
        r#"{"output": "y", "from_s": 0, "to_s": 60, "within": {"atoly": 1}}"#,
        r#"{"output": "y", "from_s": 0, "to_s": 60, "of": 1}"#,
        r#"{"diagnostic": {"block": "B"}, "at_s": 60, "when": ["b"]}"#,
        r#"{"diagnostic": {"block": "B"}, "at_s": 60, "of": 1}"#,
        r#"{"at_s": 60, "within": {"atoly": 1}}"#,
    ] {
        assert_eq!(
            refusal(&one("", expectation)),
            ("ambiguous-expectation", "line 1 column 127".to_owned()),
            "{expectation}"
        );
    }
    // A `when` alone makes a window, incomplete: its first missing key is named.
    assert_eq!(code_of(&one("", r#"{"when": ["b"]}"#)), "missing-key");
    // A tolerance window missing a bound names it.
    assert_eq!(
        code_of(&one(
            "",
            r#"{"output": "y", "to_s": 60, "within": {"atoly": 1}, "of": 1}"#
        )),
        "missing-key"
    );
}

#[test]
fn the_envelope_is_funnel_or_aligned_grid_and_funnel_when_absent() {
    let absent = load(&set_of("")).expect("loads");
    assert_eq!(absent.envelope(), Envelope::Funnel);
    assert_eq!(absent.written_envelope(), None);
    for (name, envelope) in [
        ("funnel", Envelope::Funnel),
        ("aligned-grid", Envelope::AlignedGrid),
    ] {
        let set = load(&enveloped(&format!("\"{name}\""))).expect(name);
        assert_eq!(set.envelope(), envelope);
        assert_eq!(set.written_envelope(), Some(envelope));
        // A set with the key and no tolerance window loads; its receipt says exact.
        assert!(!set.uses_tolerance());
    }
    for name in [
        "\"Funnel\"",
        "\"aligned_grid\"",
        "\"\"",
        "\"funnel \"",
        "\"exact\"",
    ] {
        assert_eq!(
            refusal(&enveloped(name)),
            ("unknown-envelope", "envelope".to_owned()),
            "{name}"
        );
    }
    for value in ["1", "true", "null", "[\"funnel\"]", "{}"] {
        assert_eq!(code_of(&enveloped(value)), "wrong-type", "{value}");
    }
    // The envelope is judged straight after the clock: before a scenario's own refusals.
    let both = enveloped("\"nope\"").replacen(
        "\"scenarios\": []",
        r#""scenarios": [{"name": "", "inputs": {}, "expect": []}]"#,
        1,
    );
    assert_eq!(code_of(&both), "unknown-envelope");
    let clock_first = enveloped("\"nope\"").replacen("\"step_s\": 60", "\"step_s\": 0", 1);
    assert_eq!(code_of(&clock_first), "invalid-clock");
}

#[test]
fn the_canonical_form_keeps_what_was_written_and_spec_id_covers_it() {
    // `within` writes the names the file wrote, sorted; an absent envelope stays absent.
    let set = load(&windowed(r#"{"rtoly": 0, "atolx": 60.0}"#, "1", "")).expect("loads");
    let written = set.to_canonical_json();
    assert!(written.contains("\"within\": {\n"));
    assert!(written.contains("\"atolx\": 60.0,\n"));
    assert!(written.contains("\"rtoly\": 0\n"));
    assert!(!written.contains("atoly") && !written.contains("envelope"));
    let again = load(&written).expect("the canonical form loads");
    assert_eq!(again, set);
    assert_eq!(again.to_canonical_json(), written);
    // Each of these is another set, so another id.
    let ids: Vec<String> = [
        windowed(r#"{"atoly": 1}"#, "1", ""),
        windowed(r#"{"atoly": 1}"#, r#"[{"t": 60, "value": 1}]"#, ""),
        windowed(r#"{"atoly": 1}"#, "1.0", ""),
        windowed(r#"{"atoly": 1.0}"#, "1", ""),
        windowed(r#"{"atoly": 1, "rtoly": 0}"#, "1", ""),
        windowed(r#"{"atoly": 1}"#, "1", r#", "when": ["a", "b"]"#),
        windowed(r#"{"atoly": 1}"#, "1", r#", "when": ["b", "a"]"#),
        windowed(r#"{"atoly": 1}"#, "1", "").replacen('{', "{\"envelope\": \"funnel\", ", 1),
        windowed(r#"{"atoly": 1}"#, "1", "").replacen('{', "{\"envelope\": \"aligned-grid\", ", 1),
    ]
    .iter()
    .map(|document| load(document).expect("loads").spec_id())
    .collect();
    for (index, id) in ids.iter().enumerate() {
        assert!(!ids[..index].contains(id), "{index}");
    }
    // Spelling alone moves nothing: member order and whitespace.
    assert_eq!(
        load(&windowed(r#"{"rtoly": 2, "atoly": 1}"#, "1", ""))
            .expect("loads")
            .spec_id(),
        load(&windowed(r#"{ "atoly":1,"rtoly":2 }"#, "1", ""))
            .expect("loads")
            .spec_id()
    );
}
