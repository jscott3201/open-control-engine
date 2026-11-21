//! What a key selection costs: the work limit, checked before anything is allocated and met to
//! the unit, and the one bit per key and output a selection holds.

use super::tests::{LIMITS, Outputs, spec_with};
use super::*;

#[test]
fn a_selection_past_the_work_limit_is_refused_before_anything_is_allocated() {
    // 64 exact keys against 100 outputs: 6,400 pairs, and no matcher step.
    let names: Vec<String> = (0..100).map(|index| format!("y{index:03}")).collect();
    let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
    let outputs = Outputs::of(&borrowed);
    let keys: Vec<String> = (0..64)
        .map(|index| format!("\"s.y{index:03}\": {{}}"))
        .collect();
    let spec = spec_with(&keys.join(", "), "", "");
    let reference = &spec.references()[0];
    let paths = outputs.paths();
    let measured = |limit: u64| {
        let limits = SpecLimits {
            max_key_match_work: limit,
            ..LIMITS
        };
        let mut code = "selected";
        let info = allocation_counter::measure(|| {
            if let Err(error) = reference.select_output_keys(&paths, &limits) {
                code = error.code();
            }
        });
        (code, info)
    };
    // At the limit it is selected, and holds one bit per key and output: 64 rows of two words.
    let (code, at) = measured(6_400);
    assert_eq!(code, "selected");
    assert!(at.bytes_max >= 64 * 2 * 8, "{at:?}");
    // One past it is refused, and the refusal is all that is allocated: a sentence.
    let (code, past) = measured(6_399);
    assert_eq!(code, "work-limit-exceeded");
    assert!(past.bytes_max < 512 && past.count_total <= 8, "{past:?}");
    assert!(past.bytes_max < at.bytes_max);
    // The same refusal costs the same against a million outputs as against a hundred: the bit
    // table, the keys' kinds and the name index are never made.
    let million = vec!["urn:t#top.y"; 1_000_000];
    let mut code = "selected";
    let starved = SpecLimits {
        max_key_match_work: 1,
        ..LIMITS
    };
    let huge = allocation_counter::measure(|| {
        if let Err(error) = reference.select_output_keys(&million, &starved) {
            code = error.code();
        }
    });
    assert_eq!(code, "work-limit-exceeded");
    assert_eq!(huge.count_total, past.count_total, "{huge:?}");
    assert!(huge.bytes_max < 512, "{huge:?}");
    // At the documented maximum, 4,096 keys against a million outputs is past the limit too.
    let many: Vec<String> = (0..4_096)
        .map(|index| format!("\"s.y{index}\": {{}}"))
        .collect();
    let wide = spec_with(&many.join(", "), "", "");
    let mut code = "selected";
    let full = allocation_counter::measure(|| {
        if let Err(error) = wide.references()[0].select_output_keys(&million, &LIMITS) {
            code = error.code();
        }
    });
    assert_eq!(code, "work-limit-exceeded");
    assert!(full.bytes_max < 512, "{full:?}");
    // The indicators selection is held to the same check.
    let rules = spec_with("", "\"s.y000\": [], \"s.y001\": []", "");
    let refused = rules.references()[0]
        .select_indicator_keys(
            &paths,
            &SpecLimits {
                max_key_match_work: 199,
                ..LIMITS
            },
        )
        .expect_err("two rules against a hundred outputs");
    assert_eq!(refused.code(), "work-limit-exceeded");
    assert_eq!(
        refused.detail(),
        "matching the keys against the outputs needs more than 199 work units"
    );
    assert!(
        rules.references()[0]
            .select_indicator_keys(
                &paths,
                &SpecLimits {
                    max_key_match_work: 200,
                    ..LIMITS
                }
            )
            .is_ok()
    );
}

#[test]
fn the_work_of_a_selection_is_its_pairs_and_its_matcher_steps_to_the_unit() {
    let spec = spec_with("\"s.yFan\": {}, \"s.T*p\": {}", "\"s.*\": []", "");
    let reference = &spec.references()[0];
    let outputs = Outputs::of(&["TSup", "TSupHeaEco", "yFan"]);
    let paths = outputs.paths();
    let under = |work: u64| {
        let limits = SpecLimits {
            max_key_match_work: work,
            ..LIMITS
        };
        reference.select_output_keys(&paths, &limits)
    };
    // The least limit that admits the selection.
    let needed = (0..10_000_u64)
        .find(|work| under(*work).is_ok())
        .expect("a small selection needs little work");
    // One unit per key and output (2 * 3), and the glob's steps over the three last segments:
    // `T*p` against `TSup` takes `T`, the `*`, two characters into the `*` and the `p` (5);
    // against `TSupHeaEco` it does the same, finds text after the `p`, gives the `*` the other
    // seven characters one at a time and is left with a `p` that nothing matches (5 + 7 + 1);
    // against `yFan` it stops at the first byte (1). The exact key costs no step.
    assert_eq!(needed, 6 + 5 + 13 + 1);
    assert!(under(needed).is_ok());
    let refused = under(needed - 1).expect_err("one unit short");
    assert_eq!(refused.code(), "work-limit-exceeded");
    assert_eq!(refused.subject(), "outputs");
    // The same selection costs the same work every time, and a raised limit is the maximum.
    assert_eq!(under(needed), under(u64::MAX));
    assert_eq!(
        under(u64::MAX),
        reference.select_output_keys(&paths, &LIMITS)
    );
    // No key, no work: a reference without overrides is selected under a limit of zero, and a
    // key against no output costs nothing and selects nothing.
    let none = SpecLimits {
        max_key_match_work: 0,
        ..LIMITS
    };
    let bare = spec_with("", "", "");
    assert!(
        bare.references()[0]
            .select_output_keys(&paths, &none)
            .is_ok()
    );
    let error = reference
        .select_indicator_keys(&[], &none)
        .expect_err("a rule and no output");
    assert_eq!(error.code(), "unmatched-key");
}

#[test]
fn a_selection_holds_one_bit_per_key_and_output() {
    let mut selected = Selected::new(3, 130);
    assert_eq!(selected.bits.len(), 3 * 3, "three keys of three words");
    for (key, output) in [(0, 0), (0, 63), (0, 64), (0, 129), (2, 1), (2, 65)] {
        selected.set(key, output);
    }
    let outputs_of = |key: usize| -> Vec<usize> {
        (0..130)
            .filter(|output| selected.has(key, *output))
            .collect()
    };
    assert_eq!(outputs_of(0), [0, 63, 64, 129]);
    assert!(outputs_of(1).is_empty());
    assert_eq!(outputs_of(2), [1, 65]);
    // Out of range: no bit, no panic, and a write past either end is dropped, never carried
    // into the next key's row.
    assert!(!selected.has(3, 0) && !selected.has(0, 130) && !selected.has(usize::MAX, usize::MAX));
    let before = selected.clone();
    selected.set(0, 130);
    selected.set(0, 191);
    selected.set(3, 0);
    selected.set(usize::MAX / 2, 1);
    assert_eq!(selected, before);
    // No output, or no key: no word.
    assert!(Selected::new(5, 0).bits.is_empty() && Selected::new(0, 5).bits.is_empty());
    assert!(!Selected::new(5, 0).has(0, 0));
    // More than one machine word of outputs through a selection: each key's bit is its own.
    let names: Vec<String> = (0..130).map(|index| format!("y{index:03}")).collect();
    let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
    let outputs = Outputs::of(&borrowed);
    let keys: Vec<String> = names
        .iter()
        .map(|name| format!("\"s.{name}\": {{}}"))
        .collect();
    let spec = spec_with(&keys.join(", "), "", "");
    let selection = spec.references()[0]
        .select_output_keys(&outputs.paths(), &LIMITS)
        .expect("selects");
    for index in 0..130 {
        assert_eq!(selection.keys_for(index).collect::<Vec<_>>(), [index]);
    }
}
