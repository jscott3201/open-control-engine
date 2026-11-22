//! Which names a class declares, which are element spellings, and the suggestion (issue #748).

use alloc::{format, string::String, vec::Vec};

use super::*;
use crate::{ParamNameTemplate, catalog, class_parameters};

/// Every registered class path.
fn classes() -> impl Iterator<Item = &'static str> {
    catalog().iter().map(|entry| entry.class_path)
}

#[test]
fn every_catalog_parameter_and_every_canonical_element_is_declared() {
    let mut names = 0;
    for class in classes() {
        for parameter in class_parameters(class).expect("registered") {
            let template = ParamNameTemplate::parse(parameter.name);
            let probes: Vec<String> = match template.rank() {
                0 => alloc::vec![String::from(parameter.name)],
                1 => [1, 2, 9, 10, 1_048_577]
                    .iter()
                    .map(|i| format!("{}_{i}", template.base()))
                    .collect(),
                _ => [(1, 1), (2, 3), (10, 1), (1, 10)]
                    .iter()
                    .map(|(r, c)| format!("{}_{r}_{c}", template.base()))
                    .collect(),
            };
            for probe in probes {
                assert_eq!(
                    declares_parameter(class, &probe),
                    Some(true),
                    "{class} {probe}"
                );
                names += 1;
            }
        }
    }
    assert!(names > 200, "{names}");
}

#[test]
fn a_name_outside_the_catalog_is_undeclared_in_every_class() {
    for class in classes() {
        for name in [
            "bogus",
            "",
            "_",
            "K_1",
            "amplitude_",
            " k",
            "k ",
            "uLow\0",
            "\u{e9}",
        ] {
            let declared = class_parameters(class)
                .expect("registered")
                .iter()
                .any(|p| ParamNameTemplate::parse(p.name).names(name));
            assert_eq!(
                declares_parameter(class, name),
                Some(declared),
                "{class} {name:?}"
            );
        }
    }
    // The misspelling of issue #748, and its neighbour that is right.
    assert_eq!(
        declares_parameter("CDL.Integers.Sources.Pulse", "amplitud"),
        Some(false)
    );
    assert_eq!(
        declares_parameter("CDL.Integers.Sources.Pulse", "amplitude"),
        Some(true)
    );
    // A name another class declares is not declared by this one.
    assert_eq!(declares_parameter("CDL.Logical.Not", "nin"), Some(false));
    assert_eq!(
        declares_parameter("CDL.Logical.MultiAnd", "nin"),
        Some(true)
    );
    assert_eq!(
        declares_parameter("CDL.Reals.Sources.Constant", "have_hol"),
        Some(false)
    );
}

#[test]
fn an_unregistered_class_declares_nothing_and_suggests_nothing() {
    for class in [
        "",
        "CDL.Reals.Nope",
        "cdl.reals.sources.constant",
        "urn:oce:lowering#",
    ] {
        assert_eq!(declares_parameter(class, "k"), None, "{class}");
        assert!(!spells_array_element(class, "k_1"), "{class}");
        assert_eq!(nearest_declared_parameter(class, "kk"), None, "{class}");
    }
}

#[test]
fn element_spellings_are_told_from_names_that_are_no_element() {
    let sum = "CDL.Reals.MultiSum";
    // Canonical elements, in and past any extent: declared, and element-shaped.
    for name in ["k_1", "k_2", "k_99"] {
        assert_eq!(declares_parameter(sum, name), Some(true), "{name}");
        assert!(spells_array_element(sum, name), "{name}");
    }
    // A sign, a leading zero or index zero: element-shaped, not declared (other rules own them).
    for name in ["k_0", "k_00", "k_01", "k_+1", "k_-1", "k_+0"] {
        assert_eq!(declares_parameter(sum, name), Some(false), "{name}");
        assert!(spells_array_element(sum, name), "{name}");
    }
    // No index, a non-decimal index, or a matrix shape on a vector: neither.
    for name in [
        "k", "k_", "k_x", "k_1x", "k_1_2", "k__1", "k_ 1", "kk_1", "k_+", "k_1.0",
    ] {
        assert_eq!(declares_parameter(sum, name), Some(false), "{name}");
        assert!(!spells_array_element(sum, name), "{name}");
    }
    let table = "CDL.Reals.Sources.TimeTable";
    assert_eq!(declares_parameter(table, "table_1_2"), Some(true));
    for name in ["table_0_1", "table_1_0", "table_01_2", "table_+1_2"] {
        assert_eq!(declares_parameter(table, name), Some(false), "{name}");
        assert!(spells_array_element(table, name), "{name}");
    }
    for name in ["table_1", "table_1_2_3", "table_1_x", "table"] {
        assert_eq!(declares_parameter(table, name), Some(false), "{name}");
        assert!(!spells_array_element(table, name), "{name}");
    }
    // A class with no array has no element spelling.
    assert!(!spells_array_element("CDL.Reals.Sources.Constant", "k_1"));
    assert_eq!(
        declares_parameter("CDL.Reals.Sources.Constant", "k_1"),
        Some(false)
    );
}

#[test]
fn the_suggestion_names_the_parameter_a_slip_of_the_keyboard_meant() {
    let cases = [
        ("CDL.Integers.Sources.Pulse", "amplitud", Some("amplitude")),
        ("CDL.Integers.Sources.Pulse", "Amplitude", Some("amplitude")),
        ("CDL.Integers.Sources.Pulse", "ampiltude", Some("amplitude")),
        (
            "CDL.Integers.Sources.Pulse",
            "amplitudes",
            Some("amplitude"),
        ),
        ("CDL.Integers.Sources.Pulse", "aplitudee", Some("amplitude")),
        ("CDL.Reals.Hysteresis", "ulow", Some("uLow")),
        ("CDL.Reals.Hysteresis", "uLwo", Some("uLow")),
        ("CDL.Reals.Hysteresis", "uMid", None),
        (
            "CDL.Reals.Sources.TimeTable",
            "tabel_1_2",
            Some("table_<row>_<col>"),
        ),
        (
            "CDL.Reals.Sources.TimeTable",
            "table",
            Some("table_<row>_<col>"),
        ),
        (
            "CDL.Reals.Sources.TimeTable",
            "smoothnes",
            Some("smoothness"),
        ),
        ("CDL.Reals.MultiSum", "k", Some("k_<i>")),
        ("CDL.Reals.MultiSum", "nim", Some("nin")),
        ("CDL.Reals.MultiSum", "K_1", Some("k_<i>")),
        ("CDL.Reals.MultiSum", "gain_1", None),
        ("CDL.Reals.PID", "TI", Some("Ti")),
        ("CDL.Reals.PID", "Tx", None),
        ("CDL.Reals.PID", "controlerType", Some("controllerType")),
        ("CDL.Reals.Sources.Constant", "have_hol", None),
        ("CDL.Reals.Sources.Constant", "K", Some("k")),
        ("CDL.Reals.Sources.Constant", "x", None),
        ("CDL.Logical.Not", "nin", None),
        ("CDL.Reals.Sources.Constant", "", None),
    ];
    for (class, name, expected) in cases {
        assert_eq!(
            nearest_declared_parameter(class, name),
            expected,
            "{class} {name}"
        );
    }
}

#[test]
fn a_declared_name_is_never_suggested_for_itself() {
    for class in classes() {
        for parameter in class_parameters(class).expect("registered") {
            if ParamNameTemplate::parse(parameter.name).rank() == 0 {
                let nearest = nearest_declared_parameter(class, parameter.name);
                assert_ne!(nearest, Some(parameter.name), "{class} {}", parameter.name);
            }
        }
    }
}

#[test]
fn the_smallest_distance_wins_and_catalog_order_breaks_a_tie() {
    // `uLow` and `uHigh` on Hysteresis: `uLog` is one edit from `uLow` only.
    assert_eq!(
        nearest_declared_parameter("CDL.Reals.Hysteresis", "uLog"),
        Some("uLow")
    );
    // A case slip beats an edit: `YMAX` matches `yMax` case-blind.
    assert_eq!(
        nearest_declared_parameter("CDL.Reals.PID", "YMAX"),
        Some("yMax")
    );
    // `yMan` is one substitution from `yMax` and one from `yMin`: `yMax` comes first in the
    // catalog, so it is the one named.
    let pid: Vec<&str> = class_parameters("CDL.Reals.PID")
        .expect("registered")
        .iter()
        .map(|p| p.name)
        .collect();
    let max = pid.iter().position(|n| *n == "yMax").expect("yMax");
    let min = pid.iter().position(|n| *n == "yMin").expect("yMin");
    assert!(max < min);
    assert_eq!(
        nearest_declared_parameter("CDL.Reals.PID", "yMan"),
        Some("yMax")
    );
    // `xd_start` is one substitution from `xi_start` and one from `yd_start`, neither of which a
    // rule names again: the first in the catalog is named, not the last.
    let xi = pid.iter().position(|n| *n == "xi_start").expect("xi_start");
    let yd = pid.iter().position(|n| *n == "yd_start").expect("yd_start");
    assert!(xi < yd);
    assert_eq!(
        nearest_declared_parameter("CDL.Reals.PID", "xd_start"),
        Some("xi_start")
    );
}

#[test]
fn every_catalog_name_fits_the_distance_table() {
    for class in classes() {
        for parameter in class_parameters(class).expect("registered") {
            assert!(
                parameter.name.len() < MAX_COMPARED_NAME_BYTES,
                "{class} {}",
                parameter.name
            );
        }
    }
}

/// The optimal string alignment distance by the textbook full table: the oracle the bounded,
/// three-row version is held to.
fn full_osa(a: &[u8], b: &[u8]) -> usize {
    let mut table = alloc::vec![alloc::vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in table.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in table[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut cell = (table[i - 1][j] + 1)
                .min(table[i][j - 1] + 1)
                .min(table[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                cell = cell.min(table[i - 2][j - 2] + 1);
            }
            table[i][j] = cell;
        }
    }
    table[a.len()][b.len()]
}

/// A seeded xorshift64* generator (no dependency).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % bound as u64).expect("small")
    }

    /// A short string over a four-letter alphabet, so edits collide often.
    fn word(&mut self, max: usize) -> Vec<u8> {
        let len = self.below(max + 1);
        (0..len).map(|_| b"abAB"[self.below(4)]).collect()
    }
}

#[test]
fn the_bounded_distance_agrees_with_the_full_table_on_random_pairs() {
    let mut rng = Rng(0x748_0001);
    for _ in 0..20_000 {
        let a = rng.word(9);
        let b = rng.word(9);
        let limit = rng.below(4);
        let full = full_osa(&a, &b);
        let bounded = bounded_edit_distance(&a, &b, limit);
        assert_eq!(
            bounded,
            (full <= limit).then_some(full),
            "{a:?} {b:?} {limit}"
        );
    }
}

#[test]
fn a_long_name_is_rejected_by_its_length_alone() {
    let long = "a".repeat(64 * 1024);
    assert_eq!(
        bounded_edit_distance(long.as_bytes(), b"amplitude", 2),
        None
    );
    assert_eq!(
        nearest_declared_parameter("CDL.Integers.Sources.Pulse", &long),
        None
    );
    let declared = "b".repeat(MAX_COMPARED_NAME_BYTES + 1);
    assert_eq!(
        bounded_edit_distance(declared.as_bytes(), declared.as_bytes(), 2),
        None
    );
}

#[test]
fn random_class_and_name_pairs_are_judged_by_the_catalog() {
    // The property: a name is declared exactly when a catalog name covers it, an element spelling
    // is an element-shaped name of an array of the class, and a suggestion is a catalog name of
    // the class, never the name itself, within the allowed distance of it or its stem.
    let paths: Vec<&str> = classes().collect();
    let pieces = [
        "k",
        "K",
        "nin",
        "nout",
        "u",
        "y",
        "amplitude",
        "Ti",
        "table",
        "offset",
        "_",
        "_1",
        "_0",
        "_01",
        "_+1",
        "_x",
        "x",
        "Low",
        "uLow",
        "delta",
        "period",
        "msk",
        "extract",
        "\u{e9}",
    ];
    let mut rng = Rng(0x748_0002);
    for _ in 0..20_000 {
        let class = paths[rng.below(paths.len())];
        let mut name = String::new();
        for _ in 0..=rng.below(3) {
            name.push_str(pieces[rng.below(pieces.len())]);
        }
        let catalog = class_parameters(class).expect("registered");
        let covered = catalog
            .iter()
            .any(|p| ParamNameTemplate::parse(p.name).names(&name));
        assert_eq!(
            declares_parameter(class, &name),
            Some(covered),
            "{class} {name}"
        );
        if let Some(suggested) = nearest_declared_parameter(class, &name) {
            let template = ParamNameTemplate::parse(suggested);
            assert!(
                catalog.iter().any(|p| p.name == suggested),
                "{class} {name}"
            );
            assert_ne!(suggested, name, "{class}");
            let text = match template.rank() {
                0 => name.as_str(),
                rank => element_stem(&name, rank).unwrap_or(&name),
            };
            let distance = full_osa(text.as_bytes(), template.base().as_bytes());
            assert!(
                text.eq_ignore_ascii_case(template.base())
                    || distance <= MAX_SUGGESTION_DISTANCE.min(template.base().len() / 3),
                "{class} {name} -> {suggested} at {distance}"
            );
        }
        if spells_array_element(class, &name) {
            assert!(
                catalog
                    .iter()
                    .any(|p| ParamNameTemplate::parse(p.name).rank() > 0),
                "{class} {name}"
            );
        }
    }
}

#[test]
fn a_parameter_name_is_structural_in_every_class_that_declares_it_or_in_none() {
    // With undeclared names refused, an edit a subject admits is structural on a mutant only if
    // some name is settable on one class and structural on another; `oce-verify` documents that
    // none is, and this holds it.
    use alloc::collections::BTreeMap;
    let structural = |class: &str, name: &str| {
        crate::registry::lookup(class)
            .expect("registered")
            .param_rules()
            .iter()
            .any(|rule| match *rule {
                crate::ParamRule::Structural { name: ruled } => ruled == name,
                crate::ParamRule::StructuralArrayElements { base } => {
                    ParamNameTemplate::Vector { base }.names(name)
                }
                _ => false,
            })
    };
    let mut by_name: BTreeMap<String, (Vec<&str>, Vec<&str>)> = BTreeMap::new();
    for class in classes() {
        for parameter in class_parameters(class).expect("registered") {
            let template = ParamNameTemplate::parse(parameter.name);
            let probe = template
                .element_name(&alloc::vec![1; template.rank()])
                .expect("an element");
            let entry = by_name.entry(probe.clone()).or_default();
            if structural(class, &probe) {
                entry.0.push(class);
            } else {
                entry.1.push(class);
            }
        }
    }
    let mixed: Vec<_> = by_name
        .iter()
        .filter(|(_, (yes, no))| !yes.is_empty() && !no.is_empty())
        .collect();
    assert!(mixed.is_empty(), "{mixed:?}");
    assert!(
        by_name.values().any(|(yes, _)| !yes.is_empty()),
        "no structural name at all"
    );
}
