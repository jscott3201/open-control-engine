//! The CDL §7.17 seam (authoring note, test plan; decisions 2 and 21): filling and mutating every
//! `SemanticPoint` field changes the emitted bytes only in the boundary `unit` attribute, and the
//! loaded executable — the compiled-schedule blob, apart from the export content id it records —
//! is identical, so semantics never reach execution.

mod support;

use oce_author::{EmitOptions, Sequence, emit};

use support::cxf::{executable, identity, load_clean};
use support::econ_enable;

fn emitted(s: &Sequence) -> String {
    String::from_utf8(emit(s, &EmitOptions::new()).expect("emit")).expect("utf8")
}

/// Every `SemanticPoint` field of every boundary filled, with `variant` making each value distinct.
fn filled(variant: u32, unit: &str) -> Sequence {
    let mut s = econ_enable();
    for boundary in s.inputs.iter_mut().chain(s.outputs.iter_mut()) {
        let p = &mut boundary.semantic;
        p.name = Some(format!("Point {variant}"));
        p.label = Some(format!("Label \"{variant}\""));
        // A unit belongs on a Real connector; the Boolean output carries none.
        p.unit = (boundary.value_type == oce_author::SignalType::Real).then(|| unit.to_owned());
        p.brick = Some(format!("brick:Class{variant}"));
        p.s223 = Some(format!("s223:Class{variant}"));
        p.qudt = Some(format!("quantitykind:Kind{variant}"));
        p.haystack = vec![format!("tag{variant}"), "point".to_owned()];
        p.provisional = variant.is_multiple_of(2);
    }
    s
}

/// The lines of `a` and `b` that differ, pairwise (the writer puts one attribute per line).
fn differing_lines<'a>(a: &'a str, b: &'a str) -> Vec<(&'a str, &'a str)> {
    assert_eq!(a.lines().count(), b.lines().count());
    a.lines().zip(b.lines()).filter(|(x, y)| x != y).collect()
}

#[test]
fn non_unit_semantic_fields_never_reach_the_bytes() {
    let plain = {
        let mut s = econ_enable();
        for b in s.inputs.iter_mut().chain(s.outputs.iter_mut()) {
            b.semantic = oce_author::SemanticPoint::default();
            if b.value_type == oce_author::SignalType::Real {
                b.semantic.unit = Some("K".to_owned());
            }
        }
        s
    };
    let reference = emitted(&plain);
    for variant in 0..4 {
        assert_eq!(
            emitted(&filled(variant, "K")),
            reference,
            "variant {variant}"
        );
    }
}

#[test]
fn a_unit_change_moves_only_unit_lines_and_not_the_executable() {
    let kelvin = emitted(&filled(1, "K"));
    let celsius = emitted(&filled(2, "degC"));
    let changed = differing_lines(&kelvin, &celsius);
    assert_eq!(
        changed.len(),
        2,
        "one unit line per Real boundary: {changed:?}"
    );
    for (a, b) in &changed {
        assert!(a.trim().starts_with("\"S231:unit\": \"K\""), "{a}");
        assert!(b.trim().starts_with("\"S231:unit\": \"degC\""), "{b}");
    }
    // The blob records the export content id, which carries `unit`; everything else in the
    // blob — classes, blocks, parameters, order, connections, boundary I/O — is identical.
    let kelvin_engine = load_clean(kelvin.as_bytes());
    let celsius_engine = load_clean(celsius.as_bytes());
    assert_ne!(identity(&kelvin_engine), identity(&celsius_engine));
    assert_eq!(
        executable(&kelvin_engine),
        executable(&celsius_engine),
        "the executable is unit-blind"
    );
}
