//! Key selection against a sequence's outputs: globs over last segments, exact names by the
//! replay name rule, the precedence that no file order can change, its refusals, and its cost.

use super::super::spec::CommissioningSpec;
use super::*;

pub(super) const LIMITS: SpecLimits = SpecLimits::MAX;

/// A spec of one reference (sequence `s`) with the given `outputs` and `indicators` bodies and
/// top-level `tolerances` body.
pub(super) fn spec_with(outputs: &str, indicators: &str, global: &str) -> CommissioningSpec {
    let text = format!(
        "{{\"tolerances\": {{{global}}}, \"references\": [{{\"model\": \"M\", \"sequence\": \"s\", \"outputs\": {{{outputs}}}, \"indicators\": {{{indicators}}}}}]}}"
    );
    CommissioningSpec::from_json_str(&text, &LIMITS)
        .unwrap_or_else(|error| panic!("the spec loads: {error}\n{text}"))
}

/// The outputs of a sequence: canonical paths under one root, sorted as a frame lists them.
pub(super) struct Outputs(pub(super) Vec<String>);

impl Outputs {
    /// `urn:t#top.<name>` for each name, sorted by bytes.
    pub(super) fn of(names: &[&str]) -> Self {
        let mut paths: Vec<String> = names
            .iter()
            .map(|name| format!("urn:t#top.{name}"))
            .collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), names.len(), "no path twice");
        Self(paths)
    }

    pub(super) fn paths(&self) -> Vec<&str> {
        self.0.iter().map(String::as_str).collect()
    }

    /// The position of the output `name` in the sorted list.
    fn at(&self, name: &str) -> usize {
        let path = format!("urn:t#top.{name}");
        self.0
            .iter()
            .position(|each| *each == path)
            .expect("an output of the sequence")
    }
}

/// The keys of `spec`'s reference that select each named output, as key strings.
fn selecting(spec: &CommissioningSpec, outputs: &Outputs, name: &str) -> Vec<String> {
    let reference = &spec.references()[0];
    let selection = reference
        .select_output_keys(&outputs.paths(), &LIMITS)
        .unwrap_or_else(|error| panic!("selects: {error}"));
    selection
        .keys_for(outputs.at(name))
        .map(|key| reference.outputs()[key].key().to_owned())
        .collect()
}

/// The six tolerances of the named output, absent names read as zero.
fn tolerances(spec: &CommissioningSpec, outputs: &Outputs, name: &str) -> [f64; 6] {
    spec.references()[0]
        .select_output_keys(&outputs.paths(), &LIMITS)
        .unwrap_or_else(|error| panic!("selects: {error}"))
        .tolerances_for(outputs.at(name), spec.tolerances())
        .or_zero()
}

fn refusal(spec: &CommissioningSpec, outputs: &Outputs) -> SpecError {
    spec.references()[0]
        .select_output_keys(&outputs.paths(), &LIMITS)
        .expect_err("the selection is refused")
}

#[test]
fn a_glob_selects_every_output_whose_last_segment_it_matches_whole() {
    let spec = spec_with("\"s.TSup*\": {}, \"s.y?oo*\": {}, \"s.*\": {}", "", "");
    let outputs = Outputs::of(&["TSup", "TSupHeaEco", "ahu.yCooVal", "xTSup", "zone.yHea"]);
    assert_eq!(selecting(&spec, &outputs, "TSup"), ["s.*", "s.TSup*"]);
    assert_eq!(selecting(&spec, &outputs, "TSupHeaEco"), ["s.*", "s.TSup*"]);
    // The last segment alone is matched: `ahu.` is not part of what the glob sees.
    assert_eq!(
        selecting(&spec, &outputs, "ahu.yCooVal"),
        ["s.*", "s.y?oo*"]
    );
    assert_eq!(
        selecting(&spec, &outputs, "xTSup"),
        ["s.*"],
        "anchored at the start"
    );
    assert_eq!(selecting(&spec, &outputs, "zone.yHea"), ["s.*"]);
    // A glob never reaches into an earlier segment or across the path's root.
    let spec = spec_with("\"s.ahu*\": {}", "", "");
    assert_eq!(refusal(&spec, &outputs).code(), "unmatched-key");
    let spec = spec_with("\"s.*top*\": {}", "", "");
    assert_eq!(refusal(&spec, &outputs).code(), "unmatched-key");
    // Listing 12.1's key, as published, against outputs of its sequence.
    let listing = include_str!("../../tests/fixtures/commissioning/listing_12_1.json");
    let listing = CommissioningSpec::from_json_str(listing, &LIMITS).expect("loads");
    let second = &listing.references()[1];
    let supply = Outputs::of(&["TSupCoo", "TSupHeaEco", "y"]);
    let chosen = second
        .select_output_keys(&supply.paths(), &LIMITS)
        .expect("selects");
    assert!(chosen.selects(0, supply.at("TSupCoo")) && chosen.selects(0, supply.at("TSupHeaEco")));
    assert!(!chosen.selects(0, supply.at("y")));
    // Its tolerance lies over the global ones for the two it selects, and only for them.
    let global = listing.tolerances();
    assert_eq!(
        chosen
            .tolerances_for(supply.at("TSupCoo"), global)
            .or_zero(),
        [10.0, 0.5, 0.0, 0.0, 0.002, 0.002]
    );
    assert_eq!(
        chosen.tolerances_for(supply.at("y"), global).or_zero(),
        [10.0, 0.0, 0.0, 0.0, 0.002, 0.002]
    );
    let gated = second
        .select_indicator_keys(&supply.paths(), &LIMITS)
        .expect("selects");
    assert_eq!(gated.indicators_for(supply.at("TSupCoo")), ["fanSta.y"]);
    assert!(gated.indicators_for(supply.at("y")).is_empty());
}

#[test]
fn an_exact_key_names_one_output_by_the_replay_name_rule() {
    let outputs = Outputs::of(&["ahu.uFan", "ahu.yFan", "u", "zone.yDam"]);
    for (key, expected) in [
        // The last segment, more of the path, and the whole canonical path.
        ("s.yFan", "ahu.yFan"),
        ("s.ahu.yFan", "ahu.yFan"),
        ("s.urn:t#top.ahu.yFan", "ahu.yFan"),
        ("s.zone.yDam", "zone.yDam"),
        // `u` is the output named `u`, never `uFan`: no substring, no prefix.
        ("s.u", "u"),
    ] {
        let spec = spec_with(&format!("\"{key}\": {{\"atoly\": 1}}"), "", "");
        for name in ["ahu.uFan", "ahu.yFan", "u", "zone.yDam"] {
            let selected = selecting(&spec, &outputs, name);
            assert_eq!(
                selected.len(),
                usize::from(name == expected),
                "{key} and {name}"
            );
        }
    }
    // A `#` is no boundary in the name rule: `Root.y` names `…#top.Root.y`, and does not name
    // `…#Root.y`, which only `y` and the whole path name.
    let rooted = Outputs(vec![
        "urn:x#Root.y".to_owned(),
        "urn:x#top.Root.y".to_owned(),
    ]);
    let selects = |key: &str| -> Vec<usize> {
        let spec = spec_with(&format!("\"{key}\": {{}}"), "", "");
        let selection = spec.references()[0]
            .select_output_keys(&rooted.paths(), &LIMITS)
            .unwrap_or_else(|error| panic!("{key}: {error}"));
        (0..2)
            .filter(|output| selection.selects(0, *output))
            .collect()
    };
    assert_eq!(
        selects("s.Root.y"),
        [1],
        "the path under `top`, not the one at the `#`"
    );
    // `top.Root.y` reaches back to the `#` and names nothing, as in the rule's own examples.
    let spec = spec_with("\"s.top.Root.y\": {}", "", "");
    assert_eq!(refusal(&spec, &rooted).code(), "unmatched-key");
    assert_eq!(selects("s.urn:x#Root.y"), [0]);
    assert_eq!(selects("s.urn:x#top.Root.y"), [1]);
    // What the name rule does not resolve selects nothing: part of a segment, a wrong segment,
    // another case, a rest that starts past a `#` (which is no boundary), an empty rest.
    for key in [
        "s.Fan",
        "s.hu.yFan",
        "s.zone.yFan",
        "s.YFan",
        "s.yFan ",
        "s.top.ahu.yFan",
        "s.t#top.u",
        "s.",
    ] {
        let spec = spec_with(&format!("\"{key}\": {{}}"), "", "");
        let error = refusal(&spec, &outputs);
        assert_eq!(error.code(), "unmatched-key", "{key}");
        assert!(
            error.detail().contains("it was read as an exact name"),
            "{}",
            error.detail()
        );
    }
    // An exact name is one output even where a glob of the same letters would be several.
    let spec = spec_with("\"s.u\": {}", "", "");
    let wide = Outputs::of(&["u", "uFan", "yu"]);
    assert_eq!(selecting(&spec, &wide, "u"), ["s.u"]);
    assert!(selecting(&spec, &wide, "uFan").is_empty() && selecting(&spec, &wide, "yu").is_empty());
}

#[test]
fn an_exact_key_that_names_several_outputs_is_refused_as_ambiguous() {
    let outputs = Outputs::of(&["ahu.y", "plant.loop.y", "zone.y"]);
    let spec = spec_with("\"s.y\": {\"atoly\": 1}", "", "");
    let error = refusal(&spec, &outputs);
    assert_eq!(error.code(), "ambiguous-output");
    assert_eq!(error.kind(), SpecErrorKind::AmbiguousOutput);
    assert_eq!((error.subject(), error.at()), ("outputs", None));
    assert_eq!(
        error.detail(),
        "the key `s.y` names more than one output of the sequence; write more of its path; it names `urn:t#top.ahu.y`, `urn:t#top.plant.loop.y`, `urn:t#top.zone.y`"
    );
    // More of the path names one.
    for (key, name) in [
        ("s.ahu.y", "ahu.y"),
        ("s.loop.y", "plant.loop.y"),
        ("s.zone.y", "zone.y"),
    ] {
        let spec = spec_with(&format!("\"{key}\": {{}}"), "", "");
        assert_eq!(selecting(&spec, &outputs, name), [key]);
    }
    // The same rule for an indicators key.
    let spec = spec_with("", "\"s.y\": [\"s.u\"]", "");
    let error = spec.references()[0]
        .select_indicator_keys(&outputs.paths(), &LIMITS)
        .expect_err("ambiguous");
    assert_eq!(
        (error.code(), error.subject()),
        ("ambiguous-output", "indicators")
    );
    // A glob over the same segment is not ambiguous: it selects all three.
    let spec = spec_with("\"s.?\": {}", "", "");
    for name in ["ahu.y", "plant.loop.y", "zone.y"] {
        assert_eq!(selecting(&spec, &outputs, name), ["s.?"]);
    }
}

#[test]
fn a_key_that_selects_nothing_is_refused_with_the_outputs_as_a_glob_sees_them() {
    let outputs = Outputs::of(&["TSup", "TSupHeaEco", "ahu.yFan"]);
    let spec = spec_with(
        "\"s.yFan\": {}, \"s.TSpu*\": {}, \"s.zzz\": {}",
        "\"s.q?\": []",
        "",
    );
    // The first key in the order of the sorted map that selects nothing is the one refused.
    let error = refusal(&spec, &outputs);
    assert_eq!(error.code(), "unmatched-key");
    assert_eq!((error.subject(), error.at()), ("outputs", None));
    assert_eq!(
        error.detail(),
        "the key `s.TSpu*` selects no output of the sequence; it was read as a glob; the outputs are `s.TSup`, `s.TSupHeaEco`, `s.yFan`"
    );
    let error = spec.references()[0]
        .select_indicator_keys(&outputs.paths(), &LIMITS)
        .expect_err("refused");
    assert_eq!(error.subject(), "indicators");
    assert!(
        error
            .detail()
            .starts_with("the key `s.q?` selects no output")
    );
    // A key without the reference's prefix says so.
    for key in ["yFan", "t.yFan", "sFan", "S.yFan", "fanSta.y"] {
        let spec = spec_with(&format!("\"{key}\": {{}}"), "", "");
        assert_eq!(
            refusal(&spec, &outputs).detail(),
            format!(
                "the key `{key}` selects no output of the sequence; it does not start with the sequence's prefix `s.`; the outputs are `s.TSup`, `s.TSupHeaEco`, `s.yFan`"
            )
        );
    }
    // A sequence with no outputs.
    let spec = spec_with("\"s.y\": {}", "", "");
    assert_eq!(
        refusal(&spec, &Outputs::of(&[])).detail(),
        "the key `s.y` selects no output of the sequence; it was read as an exact name; the sequence has no outputs"
    );
    // Many long outputs: the sentence stays within its bound and counts what it left out, and
    // only the names that are listed are built.
    let long: Vec<String> = (0..2_000)
        .map(|index| format!("urn:t#top.output_number_{index:04}_{}", "x".repeat(60)))
        .collect();
    let borrowed: Vec<&str> = long.iter().map(String::as_str).collect();
    let mut error = None;
    let info = allocation_counter::measure(|| {
        error = Some(unmatched_key("outputs", "s.none", "s", &borrowed));
    });
    let error = error.expect("built");
    assert!(
        error.detail().len() <= MAX_SPEC_DETAIL_BYTES,
        "{}",
        error.detail().len()
    );
    let listed = error.detail().matches("`s.output_number_").count();
    assert!((1..2_000).contains(&listed), "{listed}");
    assert!(
        error
            .detail()
            .ends_with(&format!(", and {} more", 2_000 - listed))
    );
    assert!(info.bytes_total < 8_192, "{info:?}");
    // A hostile name is quoted as printable ASCII.
    let error = unmatched_key("outputs", "s.x", "s", &["urn:t#top.caf\u{e9}\n\u{1b}[0m"]);
    assert!(
        error
            .detail()
            .ends_with("the outputs are `s.caf\\u{e9}\\n\\u{1b}[0m`")
    );
}

#[test]
fn an_exact_key_beats_a_glob_field_by_field() {
    let outputs = Outputs::of(&["TSup", "ahu.yDam", "ahu.yFan"]);
    let global = "\"atolx\": 10, \"atoly\": 9, \"rtolx\": 0.002";
    let spec = spec_with(
        "\"s.y*\": {\"atoly\": 0.5, \"rtoly\": 0.1}, \"s.yFan\": {\"atoly\": 0.25}, \"s.*\": {\"ltolx\": 0.01}",
        "",
        global,
    );
    assert_eq!(
        selecting(&spec, &outputs, "ahu.yFan"),
        ["s.*", "s.y*", "s.yFan"]
    );
    // atolx, atoly, ltolx, ltoly, rtolx, rtoly.
    // yFan: the exact key's `atoly` over the glob's; the glob's `rtoly` stays, the exact key
    // does not give one; the other glob gives `ltolx`; the rest is the global object's.
    assert_eq!(
        tolerances(&spec, &outputs, "ahu.yFan"),
        [10.0, 0.25, 0.01, 0.0, 0.002, 0.1]
    );
    // yDam: the two globs, which set different fields.
    assert_eq!(
        tolerances(&spec, &outputs, "ahu.yDam"),
        [10.0, 0.5, 0.01, 0.0, 0.002, 0.1]
    );
    // TSup: one glob over the global object.
    assert_eq!(
        tolerances(&spec, &outputs, "TSup"),
        [10.0, 9.0, 0.01, 0.0, 0.002, 0.0]
    );
    // The exact key wins wherever the two keys sort: a glob that sorts before the exact key,
    // and one that sorts after it. Laying the keys over one another in their order would let
    // the second glob win.
    let fan = Outputs::of(&["yFan"]);
    let spec = spec_with(
        "\"s.yFan\": {\"atoly\": 0.25}, \"s.yFa?\": {\"atoly\": 0.5}",
        "",
        "",
    );
    assert_eq!(selecting(&spec, &fan, "yFan"), ["s.yFa?", "s.yFan"]);
    assert_eq!(tolerances(&spec, &fan, "yFan")[1], 0.25);
    let spec = spec_with(
        "\"s.yFan\": {\"atoly\": 0.25}, \"s.yFan*\": {\"atoly\": 0.5}",
        "",
        "",
    );
    assert_eq!(selecting(&spec, &fan, "yFan"), ["s.yFan", "s.yFan*"]);
    assert_eq!(tolerances(&spec, &fan, "yFan")[1], 0.25);
    // An explicit zero in the exact key is a value: it beats the glob's.
    let spec = spec_with(
        "\"s.y*\": {\"rtoly\": 0.1}, \"s.yFan\": {\"rtoly\": 0}",
        "",
        "",
    );
    let selection = spec.references()[0]
        .select_output_keys(&fan.paths(), &LIMITS)
        .expect("selects");
    assert_eq!(selection.tolerances_for(0, None).rtoly(), Some(0.0));
    // With no global object, what no key gives is absent, and reads as zero.
    assert_eq!(selection.tolerances_for(0, None).atolx(), None);
    assert_eq!(selection.tolerances_for(0, None).or_zero(), [0.0; 6]);
    // An output past the end has the global tolerances and no key.
    let global = PartialTolerances::EMPTY;
    assert_eq!(selection.tolerances_for(7, Some(&global)), global);
    assert_eq!(selection.keys_for(7).count(), 0);
}

#[test]
fn two_keys_of_one_kind_that_disagree_about_one_output_are_refused() {
    let outputs = Outputs::of(&["ahu.yDam", "ahu.yFan"]);
    // Two globs.
    let spec = spec_with(
        "\"s.y*\": {\"atoly\": 0.5}, \"s.?Fan\": {\"atoly\": 0.25}",
        "",
        "",
    );
    let error = refusal(&spec, &outputs);
    assert_eq!(error.code(), "conflicting-override");
    assert_eq!(error.kind(), SpecErrorKind::ConflictingOverride);
    assert_eq!((error.subject(), error.at()), ("outputs", None));
    assert_eq!(
        error.detail(),
        "the keys `s.?Fan` and `s.y*` both select the output `s.yFan` and give `atoly` different values, 0.25 and 0.5; both are globs, so neither takes precedence"
    );
    // Two exact names for one output: its last segment, and more of its path.
    let spec = spec_with(
        "\"s.yFan\": {\"rtolx\": 1}, \"s.ahu.yFan\": {\"rtolx\": 2.5}",
        "",
        "",
    );
    assert_eq!(
        refusal(&spec, &outputs).detail(),
        "the keys `s.ahu.yFan` and `s.yFan` both select the output `s.yFan` and give `rtolx` different values, 2.5 and 1; both are exact names, so neither takes precedence"
    );
    // One field of several is enough, whichever field it is.
    for (index, field) in TOLERANCE_NAMES.iter().enumerate() {
        let first: Vec<String> = TOLERANCE_NAMES
            .iter()
            .map(|name| format!("\"{name}\": 1"))
            .collect();
        let second: Vec<String> = TOLERANCE_NAMES
            .iter()
            .enumerate()
            .map(|(each, name)| format!("\"{name}\": {}", if each == index { 2 } else { 1 }))
            .collect();
        let spec = spec_with(
            &format!(
                "\"s.y*\": {{{}}}, \"s.*Fan\": {{{}}}",
                first.join(", "),
                second.join(", ")
            ),
            "",
            "",
        );
        let error = refusal(&spec, &outputs);
        assert_eq!(error.code(), "conflicting-override", "{field}");
        assert!(
            error
                .detail()
                .contains(&format!("give `{field}` different values")),
            "{}",
            error.detail()
        );
    }
    // Three globs, the third disagreeing with the first two: still refused.
    let spec = spec_with(
        "\"s.y*\": {\"atoly\": 1}, \"s.yF*\": {\"atoly\": 1}, \"s.yFa*\": {\"atoly\": 2}",
        "",
        "",
    );
    assert_eq!(refusal(&spec, &outputs).code(), "conflicting-override");
    // A long key and a hostile one: the sentence is bounded and printable.
    let long = format!("s.{}*", "y".repeat(4_000));
    let spec = spec_with(
        &format!("\"{long}\": {{\"atoly\": 1}}, \"s.y*\u{e9}\": {{\"atoly\": 2}}"),
        "",
        "",
    );
    let wide = Outputs(vec![format!("urn:t#top.{}\u{e9}", "y".repeat(4_000))]);
    let error = refusal(&spec, &wide);
    assert_eq!(error.code(), "conflicting-override");
    assert!(error.detail().len() <= MAX_SPEC_DETAIL_BYTES && error.detail().is_ascii());
}

#[test]
fn two_globs_that_disagree_are_refused_even_where_an_exact_key_would_set_the_field() {
    // The exact key would give `atoly` to the one output, and so hide what the two globs say
    // about it. They still say two things at one level: refused, and with the two globs named.
    let outputs = Outputs::of(&["ahu.yFan"]);
    let globs = "\"s.y*\": {\"atoly\": 1}, \"s.?Fan\": {\"atoly\": 2}";
    let spec = spec_with(
        &format!("{globs}, \"s.ahu.yFan\": {{\"atoly\": 3}}"),
        "",
        "",
    );
    let error = refusal(&spec, &outputs);
    assert_eq!(error.code(), "conflicting-override");
    assert_eq!(
        error.detail(),
        "the keys `s.?Fan` and `s.y*` both select the output `s.yFan` and give `atoly` different values, 2 and 1; both are globs, so neither takes precedence"
    );
    // The same refusal as without the exact key: adding or removing it changes nothing.
    assert_eq!(error, refusal(&spec_with(globs, "", ""), &outputs));
    // And it is the globs' disagreement that is refused, not the exact key's presence: with
    // the globs agreeing, the exact key sets the field.
    let agreed = spec_with(
        "\"s.y*\": {\"atoly\": 1}, \"s.?Fan\": {\"atoly\": 1.0}, \"s.ahu.yFan\": {\"atoly\": 3}",
        "",
        "",
    );
    assert_eq!(tolerances(&agreed, &outputs, "ahu.yFan")[1], 3.0);
    // Two exact names that disagree are refused with a glob beside them as well.
    let exact = spec_with(
        "\"s.yFan\": {\"atoly\": 1}, \"s.ahu.yFan\": {\"atoly\": 2}, \"s.y*\": {\"atoly\": 3}",
        "",
        "",
    );
    let error = refusal(&exact, &outputs);
    assert!(
        error
            .detail()
            .ends_with("both are exact names, so neither takes precedence")
    );
}

#[test]
fn two_keys_of_one_kind_combine_when_they_agree_or_give_different_fields() {
    let outputs = Outputs::of(&["ahu.yDam", "ahu.yFan"]);
    // Different fields.
    let spec = spec_with(
        "\"s.y*\": {\"atoly\": 0.5}, \"s.?Fan\": {\"rtoly\": 0.25}",
        "",
        "",
    );
    assert_eq!(
        tolerances(&spec, &outputs, "ahu.yFan"),
        [0.0, 0.5, 0.0, 0.0, 0.0, 0.25]
    );
    assert_eq!(
        tolerances(&spec, &outputs, "ahu.yDam"),
        [0.0, 0.5, 0.0, 0.0, 0.0, 0.0]
    );
    // The same value, in any spelling of it: `10` and `10.0`, `0` and `-0.0`, `1e1` and `10`.
    for (one, other) in [
        ("0.5", "0.5"),
        ("10", "10.0"),
        ("0", "-0.0"),
        ("1e1", "10"),
        ("-0", "0.0"),
    ] {
        let spec = spec_with(
            &format!("\"s.y*\": {{\"atoly\": {one}}}, \"s.?Fan\": {{\"atoly\": {other}}}"),
            "",
            "",
        );
        let selection = spec.references()[0].select_output_keys(&outputs.paths(), &LIMITS);
        assert!(selection.is_ok(), "{one} and {other} are one value");
    }
    // The nearest two values that are not one value are a conflict.
    let spec = spec_with(
        "\"s.y*\": {\"atoly\": 0.5}, \"s.?Fan\": {\"atoly\": 0.5000000000000001}",
        "",
        "",
    );
    assert_eq!(refusal(&spec, &outputs).code(), "conflicting-override");
    // Two exact names for one output that agree.
    let spec = spec_with(
        "\"s.yFan\": {\"atoly\": 1, \"rtolx\": 2}, \"s.ahu.yFan\": {\"atoly\": 1.0, \"ltoly\": 3}",
        "",
        "",
    );
    assert_eq!(
        tolerances(&spec, &outputs, "ahu.yFan"),
        [0.0, 1.0, 0.0, 3.0, 2.0, 0.0]
    );
    // A glob and an exact name that disagree are no conflict: the exact name wins.
    let spec = spec_with(
        "\"s.y*\": {\"atoly\": 0.5}, \"s.yFan\": {\"atoly\": 0.25}",
        "",
        "",
    );
    assert_eq!(tolerances(&spec, &outputs, "ahu.yFan")[1], 0.25);
    assert_eq!(tolerances(&spec, &outputs, "ahu.yDam")[1], 0.5);
    // Two globs that disagree are no conflict unless one output is selected by both.
    let spec = spec_with(
        "\"s.*Dam\": {\"atoly\": 0.5}, \"s.*Fan\": {\"atoly\": 0.25}",
        "",
        "",
    );
    assert_eq!(tolerances(&spec, &outputs, "ahu.yFan")[1], 0.25);
    assert_eq!(tolerances(&spec, &outputs, "ahu.yDam")[1], 0.5);
    // And then they are, whichever output it is: the first in the outputs' order is named.
    let both = Outputs::of(&["ahu.yDam", "ahu.yDamFan", "ahu.yFan"]);
    let spec = spec_with(
        "\"s.yDam*\": {\"atoly\": 0.5}, \"s.*Fan\": {\"atoly\": 0.25}",
        "",
        "",
    );
    let error = refusal(&spec, &both);
    assert!(
        error
            .detail()
            .contains("both select the output `s.yDamFan`"),
        "{}",
        error.detail()
    );
    // The top-level tolerances are below every key: they never conflict with one.
    let spec = spec_with("\"s.y*\": {\"atoly\": 0.5}", "", "\"atoly\": 9");
    assert_eq!(tolerances(&spec, &outputs, "ahu.yFan")[1], 0.5);
}

#[test]
fn the_order_of_a_files_keys_changes_neither_the_result_nor_the_refusal() {
    let outputs = Outputs::of(&["TSup", "ahu.yDam", "ahu.yFan"]);
    let members = [
        "\"s.y*\": {\"atoly\": 0.5, \"rtoly\": 0.1}",
        "\"s.yFan\": {\"atoly\": 0.25}",
        "\"s.*\": {\"ltolx\": 0.01}",
        "\"s.?Dam\": {\"atolx\": 3}",
    ];
    // Every order of the four keys: one spec, one id, one answer per output.
    let orders: [[usize; 4]; 6] = [
        [0, 1, 2, 3],
        [3, 2, 1, 0],
        [1, 0, 3, 2],
        [2, 3, 0, 1],
        [1, 3, 0, 2],
        [3, 0, 2, 1],
    ];
    let written = |order: [usize; 4]| order.map(|index| members[index]).join(", ");
    let first = spec_with(&written(orders[0]), "", "\"atolx\": 10");
    for order in orders {
        let spec = spec_with(&written(order), "", "\"atolx\": 10");
        assert_eq!(spec, first, "{order:?}");
        assert_eq!(spec.spec_id(), first.spec_id(), "{order:?}");
        for name in ["TSup", "ahu.yDam", "ahu.yFan"] {
            assert_eq!(
                tolerances(&spec, &outputs, name),
                tolerances(&first, &outputs, name)
            );
        }
    }
    assert_eq!(
        tolerances(&first, &outputs, "ahu.yFan"),
        [10.0, 0.25, 0.01, 0.0, 0.0, 0.1]
    );
    assert_eq!(
        tolerances(&first, &outputs, "ahu.yDam"),
        [3.0, 0.5, 0.01, 0.0, 0.0, 0.1]
    );
    // A conflict is the same refusal, word for word, whichever key the file writes first.
    let forward = spec_with(
        "\"s.y*\": {\"atoly\": 0.5}, \"s.?Fan\": {\"atoly\": 0.25}",
        "",
        "",
    );
    let backward = spec_with(
        "\"s.?Fan\": {\"atoly\": 0.25}, \"s.y*\": {\"atoly\": 0.5}",
        "",
        "",
    );
    assert_eq!(refusal(&forward, &outputs), refusal(&backward, &outputs));
    // And so is the first unmatched key.
    let forward = spec_with("\"s.b\": {}, \"s.a\": {}", "", "");
    let backward = spec_with("\"s.a\": {}, \"s.b\": {}", "", "");
    assert_eq!(refusal(&forward, &outputs), refusal(&backward, &outputs));
    assert!(
        refusal(&forward, &outputs)
            .detail()
            .starts_with("the key `s.a` ")
    );
}

#[test]
fn every_rule_that_selects_an_output_adds_its_indicators_each_once() {
    let spec = spec_with(
        "",
        "\"s.y*\": [\"s.uFan\", \"fanSta.y\"], \"s.yFan\": [\"occupied\", \"s.uFan\"], \"s.T*\": [], \"s.TSup\": [\"s.uFan\"]",
        "",
    );
    let reference = &spec.references()[0];
    let outputs = Outputs::of(&["TSup", "ahu.yDam", "ahu.yFan", "q"]);
    let selection = reference
        .select_indicator_keys(&outputs.paths(), &LIMITS)
        .expect("selects");
    // In the order of the sorted rules (`s.y*` before `s.yFan`), each name once: the rules are
    // ANDed, so a repeat adds nothing, and a glob and an exact name both apply.
    assert_eq!(
        selection.indicators_for(outputs.at("ahu.yFan")),
        ["s.uFan", "fanSta.y", "occupied"]
    );
    assert_eq!(
        selection.indicators_for(outputs.at("ahu.yDam")),
        ["s.uFan", "fanSta.y"]
    );
    // A rule with an empty list selects its output and gates nothing.
    assert_eq!(selection.indicators_for(outputs.at("TSup")), ["s.uFan"]);
    assert!(
        selection.indicators_for(outputs.at("q")).is_empty(),
        "no rule selects it"
    );
    assert!(selection.indicators_for(99).is_empty(), "past the end");
    assert!(selection.selects(0, outputs.at("TSup")) && !selection.selects(0, outputs.at("q")));
    // Rules never conflict: two globs with different lists over one output both apply.
    let spec = spec_with("", "\"s.y*\": [\"a\"], \"s.?Fan\": [\"b\"]", "");
    let selection = spec.references()[0]
        .select_indicator_keys(&outputs.paths(), &LIMITS)
        .expect("no conflict among rules");
    assert_eq!(selection.indicators_for(outputs.at("ahu.yFan")), ["b", "a"]);
    // A rule that selects nothing is refused as an outputs key is.
    let spec = spec_with("", "\"s.zzz\": [\"a\"]", "");
    let error = spec.references()[0]
        .select_indicator_keys(&outputs.paths(), &LIMITS)
        .expect_err("refused");
    assert_eq!(error.code(), "unmatched-key");
    // The two maps are selected separately: a reference with rules and no overrides has no
    // output key.
    assert!(
        spec.references()[0]
            .select_output_keys(&outputs.paths(), &LIMITS)
            .is_ok()
    );
}
