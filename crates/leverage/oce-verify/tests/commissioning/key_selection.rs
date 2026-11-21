//! `outputs` and `indicators` keys against a sequence's output names (commissioning note,
//! decisions 17 and 27): exact keys, globs anchored at both ends, the literal prefix, a key that
//! selects nothing, and the order overrides apply in.
//!
//! The names are the ones a run passes in: `<sequence>.<last segment>` of each output's
//! canonical path.

use oce_verify::{
    CommissioningSpec, KeySelection, MAX_SPEC_DETAIL_BYTES, OverrideKey, SpecLimits, glob_matches,
    unmatched_key,
};

use crate::{fixture_text, loaded, one_reference};

const LIMITS: SpecLimits = SpecLimits::MAX;

/// The spec of one reference (sequence `s`) with the given `outputs` and `indicators` bodies.
fn spec_with(outputs: &str, indicators: &str) -> CommissioningSpec {
    loaded(&one_reference(&format!(
        "\"outputs\": {{{outputs}}}, \"indicators\": {{{indicators}}}"
    )))
}

/// The key indexes that select each name, by name.
fn selected(selection: &KeySelection) -> Vec<Vec<usize>> {
    (0..selection.name_count())
        .map(|name| selection.keys_for(name).collect())
        .collect()
}

#[test]
fn an_exact_key_selects_only_the_name_it_equals() {
    let spec = spec_with("\"s.u\": {\"atoly\": 1}", "");
    let reference = &spec.references()[0];
    // `u` selects only an output named `u`: no substring, no prefix, no suffix.
    let names = ["s.uFan", "s.u", "s.yu", "s.U", "s.u.u", "s.u "];
    let selection = reference
        .select_output_keys(&names, &LIMITS)
        .expect("selects");
    assert_eq!(
        selected(&selection),
        [vec![], vec![0], vec![], vec![], vec![], vec![]]
    );
    assert!(selection.selects(0, 1) && !selection.selects(0, 0));
    assert_eq!((selection.key_count(), selection.name_count()), (1, 6));
    // With no output named `u`, the key selects nothing, however many names hold a `u`.
    let error = reference
        .select_output_keys(&["s.uFan", "s.yu", "s.Su"], &LIMITS)
        .expect_err("`u` is not `uFan`");
    assert_eq!(error.code(), "unmatched-key");
    assert_eq!(
        error.detail(),
        "the key `s.u` selects no output of the sequence; it was read as an exact name; the outputs are `s.uFan`, `s.yu`, `s.Su`"
    );
}

#[test]
fn a_glob_is_anchored_at_both_ends() {
    for (pattern, text, expected) in [
        // `*` is any run, the empty run included.
        ("TSup*", "TSup", true),
        ("TSup*", "TSupHeaEco", true),
        ("TSup*", "xTSup", false),
        ("TSup*", "TSu", false),
        ("*Fan", "uFan", true),
        ("*Fan", "Fan", true),
        ("*Fan", "uFanSta", false),
        ("y*Val", "yCooVal", true),
        ("y*Val", "yVal", true),
        ("y*Val", "yCooValve", false),
        ("*", "", true),
        ("*", "anything.at all", true),
        ("**", "x", true),
        ("*a*", "a", true),
        ("*a*", "bab", true),
        ("*a*", "bbb", false),
        // `?` is exactly one character.
        ("y?", "y1", true),
        ("y?", "y", false),
        ("y?", "y12", false),
        ("?", "", false),
        ("?", "y", true),
        ("??", "y", false),
        ("?*", "", false),
        ("?*", "y", true),
        ("*?", "y", true),
        ("y?oo*", "yCooVal", true),
        ("y?oo*", "yoo", false),
        // One character is one character, whatever its width in bytes.
        ("t?", "t\u{e9}", true),
        ("t?", "t\u{1f321}", true),
        ("t??", "t\u{1f321}", false),
        ("?\u{e9}", "\u{1f321}\u{e9}", true),
        ("*\u{e9}", "caf\u{e9}", true),
        ("*\u{e9}", "cafe", false),
        ("caf?", "cafe\u{301}", false),
        // Nothing else is special: no class, no escape, no separator, no case folding.
        ("[ab]", "a", false),
        ("[ab]", "[ab]", true),
        ("a.b", "a.b", true),
        ("a.b", "axb", false),
        ("a\\*", "a\\x", true),
        ("a\\*", "a*", false),
        ("{a,b}", "a", false),
        ("tsup*", "TSup", false),
        ("a*", "a/b.c", true),
        // A pattern with neither wildcard matches only itself.
        ("yCooVal", "yCooVal", true),
        ("yCooVal", "yCooVa", false),
        ("", "", true),
        ("", "a", false),
        // Backtracking: the last `*` gives way until the tail fits, or the text runs out.
        ("a*b*c", "abcbc", true),
        ("a*b*c", "abcbd", false),
        ("*aab", "aaaaab", true),
        ("a*a*a*b", "aaaaaaaa", false),
    ] {
        assert_eq!(
            glob_matches(pattern, text),
            expected,
            "{pattern:?} against {text:?}"
        );
    }
}

#[test]
fn a_glob_key_selects_every_name_it_matches_whole() {
    let spec = spec_with("\"s.TSup*\": {}, \"s.y?oo*\": {}, \"s.*\": {}", "");
    let reference = &spec.references()[0];
    let names = ["s.TSup", "s.TSupHeaEco", "s.yCooVal", "s.xTSup", "s.yHea"];
    let selection = reference
        .select_output_keys(&names, &LIMITS)
        .expect("selects");
    assert_eq!(
        selected(&selection),
        [vec![0, 2], vec![0, 2], vec![1, 2], vec![2], vec![2]]
    );
    // Listing 12.1's key, against the outputs of its sequence as a glob sees them.
    let listing = loaded(&fixture_text("listing_12_1.json"));
    let second = &listing.references()[1];
    let outputs = [
        "setPoiVAV1.TSupHeaEco",
        "setPoiVAV1.TSupCoo",
        "setPoiVAV1.y",
    ];
    let chosen = second
        .select_output_keys(&outputs, &LIMITS)
        .expect("selects");
    assert_eq!(selected(&chosen), [vec![0], vec![0], vec![]]);
    // Its tolerance lies over the global ones for the two it selects, and only for them.
    let supply = listing.tolerances_for(second, &chosen, 0);
    assert_eq!(supply.or_zero(), [10.0, 0.5, 0.0, 0.0, 0.002, 0.002]);
    let other = listing.tolerances_for(second, &chosen, 2);
    assert_eq!(other.or_zero(), [10.0, 0.0, 0.0, 0.0, 0.002, 0.002]);
    // The indicator rule selects the same two outputs, and names the outside signal.
    let gated = second
        .select_indicator_keys(&outputs, &LIMITS)
        .expect("selects");
    assert_eq!(second.indicators_for(&gated, 0), ["fanSta.y"]);
    assert_eq!(second.indicators_for(&gated, 1), ["fanSta.y"]);
    assert!(second.indicators_for(&gated, 2).is_empty());
    // The first reference has no key: every output keeps the global tolerances.
    let first = &listing.references()[0];
    let none = first
        .select_output_keys(&["setPoiVAV.y"], &LIMITS)
        .expect("selects");
    assert_eq!(none.key_count(), 0);
    assert_eq!(
        listing.tolerances_for(first, &none, 0),
        *listing.tolerances().expect("global")
    );
}

#[test]
fn the_prefix_is_literal_and_a_key_without_it_selects_nothing() {
    // The sequence's name is not a pattern: its `*` is a `*`.
    let key = OverrideKey::parse("a*b.y", "a*b").expect("the prefix");
    assert_eq!(key.rest(), "y");
    assert!(
        !key.is_glob(),
        "the wildcard is in the prefix, which is literal"
    );
    assert!(key.selects("y") && !key.selects("yy"));
    assert_eq!(OverrideKey::parse("axb.y", "a*b"), None);
    for (key, sequence) in [
        ("y", "s"),
        ("s", "s"),
        ("sy", "s"),
        ("S.y", "s"),
        ("s2.y", "s"),
        (" s.y", "s"),
        ("t.s.y", "s"),
        ("fanSta.y", "setPoiVAV1"),
        ("setPoiVAV.TSup*", "setPoiVAV1"),
    ] {
        assert_eq!(
            OverrideKey::parse(key, sequence),
            None,
            "{key} under {sequence}"
        );
    }
    // A sequence whose name holds a dot is one prefix; an empty rest is an exact name.
    let dotted = OverrideKey::parse("ahu.1.y*", "ahu.1").expect("the prefix");
    assert_eq!((dotted.rest(), dotted.is_glob()), ("y*", true));
    let empty = OverrideKey::parse("s.", "s").expect("the prefix");
    assert_eq!((empty.rest(), empty.is_glob()), ("", false));
    assert!(empty.selects("") && !empty.selects("y"));

    // In a selection, a key without the prefix selects nothing, and says why.
    let spec = spec_with("\"y\": {}", "");
    let error = spec.references()[0]
        .select_output_keys(&["s.y"], &LIMITS)
        .expect_err("no prefix");
    assert_eq!(error.code(), "unmatched-key");
    assert_eq!(
        error.detail(),
        "the key `y` selects no output of the sequence; it does not start with the sequence's prefix `s.`; the outputs are `s.y`"
    );
    // A glob does not reach across the prefix: `*` matches a rest, never another sequence.
    let spec = spec_with("\"s.*\": {}", "");
    let selection = spec.references()[0]
        .select_output_keys(&["s.y", "t.y", "sy", "s."], &LIMITS)
        .expect("selects");
    assert_eq!(selected(&selection), [vec![0], vec![], vec![], vec![0]]);
}

#[test]
fn a_key_that_selects_nothing_is_refused_with_the_names_a_glob_sees() {
    let spec = spec_with(
        "\"s.yFan\": {}, \"s.TSpu*\": {}, \"s.zzz\": {}",
        "\"s.q?\": []",
    );
    let reference = &spec.references()[0];
    let names = ["s.yFan", "s.TSup", "s.TSupHeaEco"];
    // The first key in declaration order that selects nothing is the one refused.
    let error = reference
        .select_output_keys(&names, &LIMITS)
        .expect_err("a typo");
    assert_eq!(error.code(), "unmatched-key");
    assert_eq!((error.subject(), error.at()), ("outputs", None));
    assert_eq!(
        error.detail(),
        "the key `s.TSpu*` selects no output of the sequence; it was read as a glob; the outputs are `s.yFan`, `s.TSup`, `s.TSupHeaEco`"
    );
    let error = reference
        .select_indicator_keys(&names, &LIMITS)
        .expect_err("refused");
    assert_eq!(error.subject(), "indicators");
    assert!(
        error
            .detail()
            .starts_with("the key `s.q?` selects no output")
    );
    // A sequence with no outputs.
    let error = reference
        .select_output_keys(&[], &LIMITS)
        .expect_err("refused");
    assert_eq!(
        error.detail(),
        "the key `s.yFan` selects no output of the sequence; it was read as an exact name; the sequence has no outputs"
    );
    // The same refusal, raised by a caller that resolved the key another way.
    assert_eq!(
        unmatched_key("outputs", "s.TSpu*", "s", &names),
        reference
            .select_output_keys(&names, &LIMITS)
            .expect_err("refused")
    );
    // Many long names: the sentence stays within its bound and counts what it left out.
    let long: Vec<String> = (0..500)
        .map(|index| format!("s.output_number_{index:03}_{}", "x".repeat(60)))
        .collect();
    let borrowed: Vec<&str> = long.iter().map(String::as_str).collect();
    let error = unmatched_key("outputs", "s.none", "s", &borrowed);
    assert!(
        error.detail().len() <= MAX_SPEC_DETAIL_BYTES,
        "{}",
        error.detail().len()
    );
    assert!(
        error.detail().contains("`s.output_number_000_"),
        "{}",
        error.detail()
    );
    let listed = error.detail().matches("`s.output_number_").count();
    assert!((1..500).contains(&listed), "{listed}");
    assert!(
        error
            .detail()
            .ends_with(&format!(", and {} more", 500 - listed)),
        "{}",
        error.detail()
    );
    // A hostile name is quoted as printable ASCII.
    let error = unmatched_key("outputs", "s.x", "s", &["s.caf\u{e9}\n\u{1b}[0m"]);
    assert!(
        error
            .detail()
            .ends_with("the outputs are `s.caf\\u{e9}\\n\\u{1b}[0m`")
    );
}

#[test]
fn overrides_apply_in_declaration_order_later_fields_winning() {
    let outputs = "\"s.y*\": {\"atoly\": 0.5, \"rtoly\": 0.1}, \"s.yFan\": {\"atoly\": 0.25}, \"s.*\": {\"ltolx\": 0.01}, \"s.y?an\": {\"rtoly\": 0}";
    let text = one_reference(&format!("\"outputs\": {{{outputs}}}"));
    let text = text.replacen(
        '{',
        "{\"tolerances\": {\"atolx\": 10, \"atoly\": 9, \"rtolx\": 0.002}, ",
        1,
    );
    let spec = loaded(&text);
    let reference = &spec.references()[0];
    let names = ["s.yFan", "s.yDam", "s.TSup"];
    let selection = reference
        .select_output_keys(&names, &LIMITS)
        .expect("selects");
    assert_eq!(
        selected(&selection),
        [vec![0, 1, 2, 3], vec![0, 2], vec![2]]
    );
    // atolx, atoly, ltolx, ltoly, rtolx, rtoly.
    // yFan: the global, then y* (atoly 0.5, rtoly 0.1), yFan (atoly 0.25), * (ltolx), y?an (rtoly 0).
    let fan = spec.tolerances_for(reference, &selection, 0);
    assert_eq!(fan.or_zero(), [10.0, 0.25, 0.01, 0.0, 0.002, 0.0]);
    assert_eq!(fan.rtoly(), Some(0.0), "an explicit zero, set last");
    assert_eq!(fan.ltoly(), None, "nothing gave it");
    // yDam: the global, then y*, then *.
    let damper = spec.tolerances_for(reference, &selection, 1);
    assert_eq!(damper.or_zero(), [10.0, 0.5, 0.01, 0.0, 0.002, 0.1]);
    // TSup: the global and * alone.
    let supply = spec.tolerances_for(reference, &selection, 2);
    assert_eq!(supply.or_zero(), [10.0, 9.0, 0.01, 0.0, 0.002, 0.0]);
    // A name past the end has the global tolerances and no key.
    assert_eq!(
        spec.tolerances_for(reference, &selection, 3),
        *spec.tolerances().expect("global")
    );

    // The same keys declared the other way round: the earlier, wider key no longer wins.
    let reversed = "\"s.yFan\": {\"atoly\": 0.25}, \"s.y*\": {\"atoly\": 0.5, \"rtoly\": 0.1}";
    let forward = "\"s.y*\": {\"atoly\": 0.5, \"rtoly\": 0.1}, \"s.yFan\": {\"atoly\": 0.25}";
    let atoly = |outputs: &str| {
        let spec = spec_with(outputs, "");
        let reference = &spec.references()[0];
        let selection = reference
            .select_output_keys(&["s.yFan"], &LIMITS)
            .expect("selects");
        spec.tolerances_for(reference, &selection, 0).atoly()
    };
    assert_eq!(atoly(forward), Some(0.25), "the exact key is declared last");
    assert_eq!(atoly(reversed), Some(0.5), "the glob is declared last");
    // With no global tolerances, what no key gives is absent, and reads as zero.
    let spec = spec_with("\"s.y\": {\"atoly\": 1}", "");
    let reference = &spec.references()[0];
    let selection = reference
        .select_output_keys(&["s.y"], &LIMITS)
        .expect("selects");
    let merged = spec.tolerances_for(reference, &selection, 0);
    assert_eq!((merged.atoly(), merged.atolx()), (Some(1.0), None));
    assert_eq!(merged.or_zero(), [0.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn every_rule_that_selects_an_output_adds_its_indicators() {
    let spec = spec_with(
        "",
        "\"s.y*\": [\"s.uFan\", \"fanSta.y\"], \"s.yFan\": [\"occupied\", \"s.uFan\"], \"s.T*\": [], \"s.TSup\": [\"s.uFan\"]",
    );
    let reference = &spec.references()[0];
    let names = ["s.yFan", "s.yDam", "s.TSup", "s.q"];
    let error = reference
        .select_indicator_keys(&["s.q"], &LIMITS)
        .expect_err("refused");
    assert_eq!(error.code(), "unmatched-key");
    let selection = reference
        .select_indicator_keys(&names, &LIMITS)
        .expect("selects");
    // In declaration order, each name once: the rules are ANDed, so a repeat adds nothing.
    assert_eq!(
        reference.indicators_for(&selection, 0),
        ["s.uFan", "fanSta.y", "occupied"]
    );
    assert_eq!(
        reference.indicators_for(&selection, 1),
        ["s.uFan", "fanSta.y"]
    );
    // A rule with an empty list selects its output and gates nothing.
    assert_eq!(reference.indicators_for(&selection, 2), ["s.uFan"]);
    assert!(
        reference.indicators_for(&selection, 3).is_empty(),
        "no rule selects it"
    );
    assert!(
        reference.indicators_for(&selection, 99).is_empty(),
        "past the end"
    );
    // An outputs selection and an indicators selection are separate maps with separate keys.
    assert_eq!(
        reference
            .select_output_keys(&names, &LIMITS)
            .expect("no key")
            .key_count(),
        0
    );
}

#[test]
fn a_selection_is_the_same_whatever_the_order_of_the_names() {
    let spec = spec_with("\"s.y*\": {}, \"s.?Fan\": {}, \"s.TSup\": {}", "");
    let reference = &spec.references()[0];
    let names = ["s.yFan", "s.uFan", "s.TSup", "s.yDam", "s.other"];
    let forward = reference
        .select_output_keys(&names, &LIMITS)
        .expect("selects");
    let mut reversed_names = names;
    reversed_names.reverse();
    let backward = reference
        .select_output_keys(&reversed_names, &LIMITS)
        .expect("selects");
    for (index, name) in names.iter().enumerate() {
        let mirrored = names.len() - 1 - index;
        assert_eq!(
            forward.keys_for(index).collect::<Vec<_>>(),
            backward.keys_for(mirrored).collect::<Vec<_>>(),
            "{name}"
        );
    }
    // Twice is the same selection.
    assert_eq!(
        reference.select_output_keys(&names, &LIMITS),
        Ok(forward.clone())
    );
    // An index past either end selects nothing and panics nowhere.
    assert!(
        !forward.selects(3, 0)
            && !forward.selects(0, 5)
            && !forward.selects(usize::MAX, usize::MAX)
    );
    assert_eq!(forward.keys_for(usize::MAX).count(), 0);
    // More than one machine word of keys: the bit of each key is its own.
    let many: Vec<String> = (0..130)
        .map(|index| format!("\"s.y{index}\": {{}}"))
        .collect();
    let spec = spec_with(&many.join(", "), "");
    let reference = &spec.references()[0];
    let names: Vec<String> = (0..130).map(|index| format!("s.y{index}")).collect();
    let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
    let selection = reference
        .select_output_keys(&borrowed, &LIMITS)
        .expect("selects");
    for index in 0..130 {
        assert_eq!(selection.keys_for(index).collect::<Vec<_>>(), [index]);
    }
}
