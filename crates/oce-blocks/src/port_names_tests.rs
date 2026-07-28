//! Internal consistency of the declared port-name table against the registry it describes.
//!
//! Nothing here reads vendored upstream source — that comparison lives in
//! `oce-cxf/tests/fixture_port_order.rs`, and keeping it there is deliberate: this crate must not
//! grow a dependency on `third_party/`, or the audit would be checking a table against its own
//! input. What these tests can prove is the property that makes the table *usable* — that it lines
//! up index-for-index with the signatures the resolver will bind against — and that the set of
//! classes it deliberately omits has not drifted.

use crate::port_names::{all_port_names, port_names};
use crate::{ParamRule, PortKind, lookup};
use oce_model::ParamTable;

/// Classes that bind positionally, and the reason each one does. Pinned by name rather than by
/// count alone: a class silently joining this set loses its nominal guard, which is exactly the
/// direction that fails open.
const EXPECTED_UNNAMED: &[&str] = &[
    // No vendored upstream source and present in no fixture, so no name for it could be falsified.
    "CDL.Logical.TrueHoldWithReset",
    // Upstream declares an array output `y[nout]`; the registry carries a single output, so no
    // scalar name is truthful here.
    "CDL.Integers.Sources.TimeTable",
    "CDL.Logical.Sources.TimeTable",
    "CDL.Reals.Sources.TimeTable",
];

/// Every named class is a registered class, and the two agree on arity in both directions.
///
/// This is the load-bearing check in this file. The table and the `BlockSignature`s are authored
/// separately — different files, different shapes — so an index-for-index arity agreement across
/// all 104 entries is a real constraint rather than a restatement.
#[test]
fn every_named_class_matches_its_registry_signature_arity() {
    let mut checked = 0usize;
    for entry in all_port_names() {
        let reg = lookup(entry.class_path)
            .unwrap_or_else(|| panic!("{} is named but not registered", entry.class_path));
        let probe = (reg.make)(&ParamTable::default());
        let sig = probe.resolved_signature();
        assert_eq!(
            (entry.inputs.len(), entry.outputs.len()),
            (sig.inputs.len(), sig.outputs.len()),
            "{}: named {} input(s)/{} output(s), signature has {}/{}",
            entry.class_path,
            entry.inputs.len(),
            entry.outputs.len(),
            sig.inputs.len(),
            sig.outputs.len()
        );
        checked += 1;
    }
    assert_eq!(
        checked, 104,
        "named-class count changed. It is exactly the set the vendored-source audit can reach; \
         growing it adds names nothing in this repository can contradict."
    );
}

/// A port name is a Modelica identifier, and no two ports on the same side share one.
///
/// Uniqueness is what makes name-based binding well defined: a duplicate would make the
/// document-order-to-signature-order mapping ambiguous, and the resolver would have to pick.
#[test]
fn port_names_are_unique_identifiers_within_a_side() {
    for entry in all_port_names() {
        for (side, names) in [("input", entry.inputs), ("output", entry.outputs)] {
            for name in names {
                let mut chars = name.chars();
                let first = chars.next().unwrap_or_else(|| {
                    panic!("{}: empty {side} port name", entry.class_path);
                });
                assert!(
                    first.is_ascii_alphabetic() || first == '_',
                    "{}: {side} port name {name:?} does not start a Modelica identifier",
                    entry.class_path
                );
                assert!(
                    chars.all(|c| c.is_ascii_alphanumeric() || c == '_'),
                    "{}: {side} port name {name:?} is not a Modelica identifier",
                    entry.class_path
                );
            }
            let mut sorted: Vec<&str> = names.to_vec();
            sorted.sort_unstable();
            let before = sorted.len();
            sorted.dedup();
            assert_eq!(
                sorted.len(),
                before,
                "{}: duplicate {side} port name makes name binding ambiguous",
                entry.class_path
            );
        }
    }
}

/// One entry per class.
#[test]
fn no_class_is_named_twice() {
    let mut paths: Vec<&str> = all_port_names().iter().map(|e| e.class_path).collect();
    let before = paths.len();
    paths.sort_unstable();
    paths.dedup();
    assert_eq!(
        before,
        paths.len(),
        "a class_path appears twice in the table"
    );
}

/// The classes that bind positionally are exactly the ones documented as doing so.
///
/// A class dropping out of the table is invisible at runtime — it simply stops being checked — so
/// the omission set is pinned by name, not merely counted.
#[test]
fn unnamed_classes_are_exactly_the_documented_exemptions() {
    let mut unnamed: Vec<&str> = Vec::new();
    let mut width_driven = 0usize;
    let mut registered = 0usize;
    for entry in crate::registry::all_entries() {
        registered += 1;
        if port_names(entry.class_path).is_some() {
            continue;
        }
        // A width-driven class expands one upstream array connector into N scalars, so it has no
        // 1:1 names to record. Those need no per-class pin; the rest do. Width-drivenness is read
        // from the param rules, the same source the registry manifest uses.
        let is_width_driven = entry.param_rules().iter().any(|rule| {
            matches!(
                rule,
                ParamRule::Structural { .. } | ParamRule::StructuralArrayElements { .. }
            )
        });
        if is_width_driven {
            width_driven += 1;
        } else {
            unnamed.push(entry.class_path);
        }
    }
    assert_eq!(registered, 133, "registered class count changed");
    unnamed.sort_unstable();
    let mut expected: Vec<&str> = EXPECTED_UNNAMED.to_vec();
    expected.sort_unstable();
    assert_eq!(
        unnamed, expected,
        "the set of fixed-arity classes that bind positionally changed"
    );
    assert_eq!(
        width_driven, 25,
        "width-driven class count changed; those classes carry no port names by construction"
    );
}

/// `port_names` answers `None` for an unregistered path rather than panicking, and finds a known
/// one. Guards the lookup itself, which is the only code in the module.
#[test]
fn lookup_finds_named_classes_and_rejects_unknown_ones() {
    let pid = port_names("CDL.Reals.PID").expect("PID is named");
    assert_eq!(pid.inputs, ["u_s", "u_m"]);
    assert_eq!(pid.outputs, ["y"]);
    assert!(port_names("CDL.Reals.NotAClass").is_none());
    assert!(port_names("").is_none());
    // Width-driven: absent on purpose, and absence must not read as "no ports".
    assert!(port_names("CDL.Reals.MultiSum").is_none());
    assert_eq!(
        lookup("CDL.Reals.MultiSum").map(|r| (r.make)(&ParamTable::default())
            .resolved_signature()
            .outputs
            .len()),
        Some(1),
        "MultiSum really does have an output; None from port_names means positional, not portless"
    );
}

/// The named table agrees with the signature on port KIND ordering for a spot-checked severe case.
///
/// `Reals.PID` is the reason this table exists: both inputs are Real, so the kind guard cannot see
/// a swap, and a swap inverts the control action.
#[test]
fn severe_same_kind_classes_are_named() {
    for (class_path, inputs) in [
        ("CDL.Reals.PID", &["u_s", "u_m"][..]),
        ("CDL.Logical.Latch", &["u", "clr"][..]),
        ("CDL.Logical.Toggle", &["u", "clr"][..]),
        ("CDL.Reals.Derivative", &["k", "T", "u"][..]),
        ("CDL.Reals.Line", &["x1", "f1", "x2", "f2", "u"][..]),
        ("CDL.Logical.Proof", &["u_s", "u_m"][..]),
        ("CDL.Logical.TimerAccumulating", &["u", "reset"][..]),
        ("CDL.Integers.OnCounter", &["trigger", "reset"][..]),
    ] {
        let named = port_names(class_path).unwrap_or_else(|| {
            panic!(
                "{class_path} reorders under an alphabetical sort without \
                                       changing its kind sequence and must carry names"
            )
        });
        assert_eq!(named.inputs, inputs, "{class_path} input names");
        let reg = lookup(class_path).expect("registered");
        let probe = (reg.make)(&ParamTable::default());
        let kinds = probe.resolved_signature();
        let first = kinds.inputs.first().copied();
        assert!(
            kinds.inputs.iter().all(|k| Some(*k) == first),
            "{class_path} was recorded as same-kind but its signature kinds differ: {:?}",
            kinds.inputs.iter().collect::<Vec<&PortKind>>()
        );
    }
}
