//! Every bound accepted at the limit and refused one past it (vectors note, "Bounds"; decision
//! 35): the five `VerifyLimits`, at their maxima and lowered, and the three bounds the strict
//! profile fixes, through whole documents.

mod support;

use oce_verify::{MAX_DEPTH, MAX_LIST_LEN, MAX_STRING_BYTES, VectorError, VectorSet, VerifyLimits};

use support::{one, scenario, set_of, set_on};

fn load_under(text: &str, limits: &VerifyLimits) -> Result<VectorSet, VectorError> {
    VectorSet::from_json_bytes(text.as_bytes(), limits)
}

fn code_under(text: &str, limits: &VerifyLimits) -> &'static str {
    match load_under(text, limits) {
        Ok(_) => "loaded",
        Err(error) => error.code(),
    }
}

const MAX: VerifyLimits = VerifyLimits::MAX;

fn scenarios(count: usize) -> String {
    (0..count)
        .map(|index| format!("{{\"name\": \"s{index}\", \"inputs\": {{}}, \"expect\": []}}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn windows(count: usize) -> String {
    vec![r#"{"output": "y", "from_s": 0, "to_s": 60, "equals": true}"#; count].join(",")
}

fn params(count: usize) -> String {
    let entries: Vec<String> = (0..count)
        .map(|index| format!("\"p{index}.k\": {index}"))
        .collect();
    format!("{{{}}}", entries.join(", "))
}

fn set_with_params(count: usize) -> String {
    format!(
        "{{\"schema\": \"oce/vectors/v1\", \"clock\": {{\"step_s\": 60, \"horizon_s\": 1800}}, \"params\": {}, \"scenarios\": []}}",
        params(count)
    )
}

fn scenario_with_params(count: usize) -> String {
    set_of(&format!(
        "{{\"name\": \"s\", \"params\": {}, \"inputs\": {{}}, \"expect\": []}}",
        params(count)
    ))
}

#[test]
fn the_byte_bound_is_checked_from_the_length_before_a_byte_is_read() {
    let limit = usize::try_from(MAX.max_bytes).expect("fits");
    assert_eq!(limit, 4 * 1024 * 1024);
    let document = set_of(&scenario("\"u\": 1", ""));
    // Trailing whitespace is part of the document: padded to the bound it loads, one byte more
    // it does not.
    let at_limit = format!("{document}{}", " ".repeat(limit - document.len()));
    assert_eq!(at_limit.len(), limit);
    assert_eq!(code_under(&at_limit, &MAX), "loaded");
    let one_past = format!("{at_limit} ");
    let refused = load_under(&one_past, &MAX).expect_err("refused");
    assert_eq!(
        (refused.code(), refused.subject()),
        ("input-too-large", "document")
    );
    assert!(refused.detail().contains("4194305"), "{}", refused.detail());
    // The length decides, whatever the bytes are: not UTF-8, not JSON, a byte-order mark.
    let marks = "\u{feff}".repeat(limit).into_bytes();
    for junk in [vec![0xff; limit + 1], vec![b'['; limit + 1], marks] {
        let refused = VectorSet::from_json_bytes(&junk, &MAX).expect_err("refused");
        assert_eq!(refused.code(), "input-too-large");
    }
}

#[test]
fn a_lowered_byte_bound_is_the_bound() {
    let document = set_of("");
    let length = u64::try_from(document.len()).expect("fits");
    let at = VerifyLimits {
        max_bytes: length,
        ..MAX
    };
    let below = VerifyLimits {
        max_bytes: length - 1,
        ..MAX
    };
    assert_eq!(code_under(&document, &at), "loaded");
    assert_eq!(code_under(&document, &below), "input-too-large");
    assert_eq!(
        code_under(
            "",
            &VerifyLimits {
                max_bytes: 0,
                ..MAX
            }
        ),
        "json-syntax"
    );
    assert_eq!(
        code_under(
            " ",
            &VerifyLimits {
                max_bytes: 0,
                ..MAX
            }
        ),
        "input-too-large"
    );
}

#[test]
fn the_scenario_bound_is_accepted_at_the_limit_and_refused_one_past() {
    let at_limit = load_under(&set_of(&scenarios(4_096)), &MAX).expect("4,096 scenarios");
    assert_eq!(at_limit.scenarios().len(), 4_096);
    assert_eq!(
        code_under(&set_of(&scenarios(4_097)), &MAX),
        "too-many-scenarios"
    );
    let two = VerifyLimits {
        max_scenarios: 2,
        ..MAX
    };
    assert_eq!(code_under(&set_of(&scenarios(2)), &two), "loaded");
    assert_eq!(
        code_under(&set_of(&scenarios(3)), &two),
        "too-many-scenarios"
    );
    let none = VerifyLimits {
        max_scenarios: 0,
        ..MAX
    };
    assert_eq!(code_under(&set_of(""), &none), "loaded");
    assert_eq!(
        code_under(&set_of(&scenarios(1)), &none),
        "too-many-scenarios"
    );
}

#[test]
fn the_expectation_bound_is_per_scenario() {
    let at_limit = load_under(&one("", &windows(4_096)), &MAX).expect("4,096 expectations");
    assert_eq!(at_limit.scenarios()[0].expectations().len(), 4_096);
    assert_eq!(
        code_under(&one("", &windows(4_097)), &MAX),
        "too-many-expectations"
    );
    // Two scenarios at the bound are within it: the bound is not a total.
    let both = set_of(&format!(
        "{{\"name\": \"a\", \"inputs\": {{}}, \"expect\": [{0}]}}, {{\"name\": \"b\", \"inputs\": {{}}, \"expect\": [{0}]}}",
        windows(2)
    ));
    let two = VerifyLimits {
        max_expectations: 2,
        ..MAX
    };
    assert_eq!(code_under(&both, &two), "loaded");
    assert_eq!(
        code_under(&one("", &windows(3)), &two),
        "too-many-expectations"
    );
    let none = VerifyLimits {
        max_expectations: 0,
        ..MAX
    };
    assert_eq!(code_under(&one("", ""), &none), "loaded");
    assert_eq!(
        code_under(&one("", &windows(1)), &none),
        "too-many-expectations"
    );
}

#[test]
fn the_parameter_bound_is_per_map_at_the_set_and_in_a_scenario() {
    let set_level = load_under(&set_with_params(256), &MAX).expect("256 set parameters");
    assert_eq!(set_level.params().map(<[_]>::len), Some(256));
    assert_eq!(
        code_under(&set_with_params(257), &MAX),
        "too-many-parameters"
    );
    let scenario_level = load_under(&scenario_with_params(256), &MAX).expect("256 parameters");
    assert_eq!(
        scenario_level.scenarios()[0].params().map(<[_]>::len),
        Some(256)
    );
    assert_eq!(
        code_under(&scenario_with_params(257), &MAX),
        "too-many-parameters"
    );
    // Both maps at the bound are within it.
    let both = set_with_params(256).replace(
        "\"scenarios\": []",
        &format!(
            "\"scenarios\": [{{\"name\": \"s\", \"params\": {}, \"inputs\": {{}}, \"expect\": []}}]",
            params(256)
        ),
    );
    assert_eq!(code_under(&both, &MAX), "loaded");
    let three = VerifyLimits {
        max_parameters: 3,
        ..MAX
    };
    assert_eq!(code_under(&set_with_params(3), &three), "loaded");
    assert_eq!(
        code_under(&set_with_params(4), &three),
        "too-many-parameters"
    );
    assert_eq!(
        code_under(&scenario_with_params(4), &three),
        "too-many-parameters"
    );
    let none = VerifyLimits {
        max_parameters: 0,
        ..MAX
    };
    assert_eq!(code_under(&set_with_params(0), &none), "loaded");
    assert_eq!(
        code_under(&set_with_params(1), &none),
        "too-many-parameters"
    );
}

#[test]
fn the_tick_bound_can_be_lowered() {
    let thirty_one = VerifyLimits {
        max_ticks: 31,
        ..MAX
    };
    let thirty = VerifyLimits {
        max_ticks: 30,
        ..MAX
    };
    assert_eq!(code_under(&set_on("60", "1800", ""), &thirty_one), "loaded");
    assert_eq!(
        code_under(&set_on("60", "1800", ""), &thirty),
        "too-many-ticks"
    );
    let one_tick = VerifyLimits {
        max_ticks: 1,
        ..MAX
    };
    assert_eq!(code_under(&set_on("60", "0", ""), &one_tick), "loaded");
    assert_eq!(
        code_under(&set_on("60", "60", ""), &one_tick),
        "too-many-ticks"
    );
    let no_tick = VerifyLimits {
        max_ticks: 0,
        ..MAX
    };
    assert_eq!(
        code_under(&set_on("60", "0", ""), &no_tick),
        "too-many-ticks"
    );
}

#[test]
fn a_limit_above_its_maximum_is_the_maximum() {
    let raised = VerifyLimits {
        max_bytes: u64::MAX,
        max_ticks: u64::MAX,
        max_scenarios: u32::MAX,
        max_expectations: u32::MAX,
        max_parameters: u32::MAX,
    };
    assert_eq!(raised.effective(), MAX);
    assert_eq!(
        code_under(&set_of(&scenarios(4_097)), &raised),
        "too-many-scenarios"
    );
    assert_eq!(
        code_under(&one("", &windows(4_097)), &raised),
        "too-many-expectations"
    );
    assert_eq!(
        code_under(&set_with_params(257), &raised),
        "too-many-parameters"
    );
    assert_eq!(
        code_under(&set_on("1", "16777216", ""), &raised),
        "too-many-ticks"
    );
    let over = " ".repeat(4 * 1024 * 1024 + 1);
    assert_eq!(code_under(&over, &raised), "input-too-large");
    // And everything the maxima accept is still accepted.
    assert_eq!(code_under(&set_of(&scenarios(4_096)), &raised), "loaded");
    assert_eq!(code_under(&set_on("1", "16777215", ""), &raised), "loaded");
}

#[test]
fn a_step_list_is_accepted_at_the_list_bound_and_refused_one_past() {
    // 65,536 steps at t = 0, 1, 2, …, on a clock whose horizon is the last of them.
    let list = |count: usize| {
        let steps: Vec<String> = (0..count)
            .map(|t| format!("{{\"t\":{t},\"value\":{t}}}"))
            .collect();
        format!("\"u\": [{}]", steps.join(","))
    };
    let clock = |inputs: &str| set_on("1", "65536", &scenario(inputs, ""));
    let at_limit = load_under(&clock(&list(MAX_LIST_LEN)), &MAX).expect("65,536 steps");
    assert_eq!(
        at_limit.scenarios()[0].inputs()[0].steps().len(),
        MAX_LIST_LEN
    );
    let refused = load_under(&clock(&list(MAX_LIST_LEN + 1)), &MAX).expect_err("refused");
    assert_eq!(refused.code(), "list-too-long");
    assert_eq!(
        refused.subject(),
        "line 1 column 118",
        "where the list opens"
    );
}

#[test]
fn nesting_past_the_profile_bound_is_refused_wherever_it_is_written() {
    // The format's own deepest value (a step) nests six levels; a value nested past eight is
    // refused for its depth even where the shape would refuse it anyway.
    let deep = |levels: usize| format!("{}1{}", "[".repeat(levels), "]".repeat(levels));
    // The input sits at depth 4 (set, scenarios, scenario, inputs).
    let within = one(&format!("\"u\": {}", deep(MAX_DEPTH - 4)), "");
    let past = one(&format!("\"u\": {}", deep(MAX_DEPTH - 3)), "");
    assert_eq!(
        code_under(&within, &MAX),
        "wrong-type",
        "a list of lists is no step list"
    );
    assert_eq!(code_under(&past, &MAX), "nesting-too-deep");
    assert_eq!(code_under(&deep(MAX_DEPTH), &MAX), "wrong-type");
    assert_eq!(code_under(&deep(MAX_DEPTH + 1), &MAX), "nesting-too-deep");
    assert_eq!(
        code_under(&"{\"a\":".repeat(100_000), &MAX),
        "nesting-too-deep"
    );
}

#[test]
fn a_string_is_accepted_at_the_profile_bound_and_refused_one_past() {
    let named = |length: usize| {
        set_of(&format!(
            "{{\"name\": \"{}\", \"inputs\": {{}}, \"expect\": []}}",
            "n".repeat(length)
        ))
    };
    let at_limit = load_under(&named(MAX_STRING_BYTES), &MAX).expect("4,096 bytes");
    assert_eq!(at_limit.scenarios()[0].name().len(), MAX_STRING_BYTES);
    assert_eq!(
        code_under(&named(MAX_STRING_BYTES + 1), &MAX),
        "string-too-long"
    );
    // A key is a string: an input name is held to the bound too.
    let input = |length: usize| one(&format!("\"{}\": 1", "u".repeat(length)), "");
    assert_eq!(code_under(&input(MAX_STRING_BYTES), &MAX), "loaded");
    assert_eq!(
        code_under(&input(MAX_STRING_BYTES + 1), &MAX),
        "string-too-long"
    );
}

#[test]
fn the_profile_bounds_do_not_move_with_the_limits() {
    // Lowering every limit to zero leaves the strict-JSON pass as it is: it runs first.
    let zero = VerifyLimits {
        max_bytes: MAX.max_bytes,
        max_ticks: 0,
        max_scenarios: 0,
        max_expectations: 0,
        max_parameters: 0,
    };
    assert_eq!(code_under("[[[[[[[[[1]]]]]]]]]", &zero), "nesting-too-deep");
    assert_eq!(code_under("[[[[[[[[1]]]]]]]]", &zero), "wrong-type");
    assert_eq!(code_under(&set_of(""), &zero), "too-many-ticks");
}
