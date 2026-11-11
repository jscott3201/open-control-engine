//! `unit-suffix-mismatch`'s two halves: the declared unit read from the vocabulary, and the
//! judgement of a suffix against it (issue #501).

use super::super::catalog::{lookup_class, lookup_parameter};
use super::{declared_unit, mismatch};

fn unit(class_path: &str, name: &str) -> &'static str {
    let class = lookup_class(class_path).expect("a catalog class");
    let parameter = lookup_parameter(class.metadata, name).expect("a parameter");
    declared_unit(class_path, parameter)
}

#[test]
fn declared_units_are_the_vocabularys() {
    assert_eq!(unit("CDL.Logical.TrueDelay", "delayTime"), "s");
    assert_eq!(unit("CDL.Reals.PID", "Ti"), "s");
    assert_eq!(unit("CDL.Discrete.Sampler", "samplePeriod"), "s");
    assert_eq!(unit("CDL.Reals.Hysteresis", "uLow"), "");
    // An array element is looked up under its template's base name.
    assert_eq!(unit("CDL.Reals.MultiSum", "k_2"), "");
}

#[test]
fn every_declared_unit_the_vocabulary_names_is_reachable_through_the_catalog() {
    // The index is keyed by catalog class path and base name: every non-empty unit in the data
    // is found again through the catalog's own parameter, so none is silently unchecked.
    let mut found = 0;
    for class in &oce_vocab::vocabulary().generated().classes {
        let Some(view) = lookup_class(&class.class_path) else {
            continue;
        };
        for parameter in &class.parameters {
            let entry = view
                .metadata
                .parameters
                .iter()
                .find(|p| oce_vocab::parameter_base_name(p.name) == parameter.name)
                .expect("the vocabulary names catalog parameters");
            assert_eq!(
                declared_unit(&class.class_path, entry),
                parameter.unit,
                "{}.{}",
                class.class_path,
                parameter.name
            );
            found += usize::from(!parameter.unit.is_empty());
        }
    }
    // 36 in `s`, 7 in `1`, 4 in `1/s`, 3 in `rad`, 1 in `Hz`, 1 in `Pa`.
    assert_eq!(found, 52);
}

#[test]
fn a_suffix_must_reduce_to_the_declared_unit() {
    let what = "parameter `delayTime` of `CDL.Logical.TrueDelay`";
    for fits in ["s", "min", "h", "d"] {
        assert_eq!(mismatch(what, "s", fits), None, "{fits}");
    }
    for (suffix, needle) in [
        ("K", "is not that unit"),
        ("Hz", "is not that unit"),
        ("1/s", "is not that unit"),
        ("m.s", "is not that unit"),
    ] {
        let detail = mismatch(what, "s", suffix).expect(suffix);
        assert!(detail.starts_with(what), "{detail}");
        assert!(detail.contains("declared in `s`"), "{detail}");
        assert!(detail.contains(needle), "{detail}");
    }
    // A scaled symbol names its base when the declared unit is something else.
    let detail = mismatch("parameter `k` of `C`", "K", "h").expect("h on K");
    assert!(detail.contains("a multiple of `s`"), "{detail}");
    // Spelled alike: Hz on a parameter declared in Hz fits; a different spelling of the same
    // dimension does not.
    assert_eq!(mismatch(what, "Hz", "Hz"), None);
    assert!(mismatch(what, "1/s", "Hz").is_some());
}

#[test]
fn with_no_declared_unit_a_base_suffix_is_an_annotation_and_a_scaled_one_is_refused() {
    let what = "parameter `uLow` of `CDL.Reals.Hysteresis`";
    for annotation in ["K", "s", "m/s2"] {
        assert_eq!(mismatch(what, "", annotation), None, "{annotation}");
    }
    for scaled in ["min", "h", "d"] {
        let detail = mismatch(what, "", scaled).expect(scaled);
        assert!(detail.contains("declares no unit"), "{detail}");
        assert!(detail.contains("write the value in `s`"), "{detail}");
    }
}

#[test]
fn a_suffix_that_is_no_unit_is_refused() {
    // Only a model built in code can hold one: the parser refuses it.
    for bad in ["", "%", "degC", "m3/h", "1"] {
        let detail = mismatch("sequence parameter `p`", "", bad).expect(bad);
        assert!(detail.contains("the suffix is no unit"), "{bad}: {detail}");
    }
}
