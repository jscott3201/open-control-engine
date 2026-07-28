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
/// all 108 entries is a real constraint rather than a restatement.
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
        checked, 108,
        "named-class count changed. 104 of these are reachable by the vendored-source audit; the \
         remaining four are TrueHoldWithReset, which has no `.mo`, and the three internal lowering \
         classes. Growing the set past that adds names nothing in this repository can contradict."
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
    assert_eq!(registered, 136, "registered class count changed");
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

/// Every class side that reorders under the reference generator's sort without changing its kind
/// sequence carries names — computed, not listed.
///
/// The reference toolchain sorts port labels **case-insensitively**; that is measured from its own
/// output, where all 50 multi-input classes agree with a case-insensitive ordering and only 46 with
/// a case-sensitive one. The distinction is not academic: an earlier revision of this analysis used
/// a case-sensitive sort and got the exposed set wrong in both directions, dropping the three
/// `Psychrometrics.*_TDryBulPhi` classes and wrongly including `Reals.Derivative`.
///
/// Deriving the set here rather than pinning a hand-written list means a class that *becomes*
/// exposed — by a rename, or by joining the registry — is caught instead of quietly omitted.
#[test]
fn every_silently_exposed_side_carries_names() {
    let reorders_silently = |names: &[&str], kinds: &[PortKind]| -> bool {
        if names.len() < 2 {
            return false;
        }
        let mut sorted: Vec<(&str, PortKind)> =
            names.iter().copied().zip(kinds.iter().copied()).collect();
        sorted.sort_by_key(|(n, _)| n.to_ascii_lowercase());
        let reordered = sorted.iter().map(|(n, _)| *n).ne(names.iter().copied());
        let kinds_unchanged = sorted.iter().map(|(_, k)| *k).eq(kinds.iter().copied());
        reordered && kinds_unchanged
    };

    let mut exposed: Vec<String> = Vec::new();
    for entry in all_port_names() {
        let reg = lookup(entry.class_path).expect("registered");
        let probe = (reg.make)(&ParamTable::default());
        let sig = probe.resolved_signature();
        if reorders_silently(entry.inputs, &sig.inputs) {
            exposed.push(format!("{} inputs", entry.class_path));
        }
        if reorders_silently(entry.outputs, &sig.outputs) {
            exposed.push(format!("{} outputs", entry.class_path));
        }
    }
    exposed.sort();

    let expected = [
        "CDL.Integers.Change outputs",
        "CDL.Integers.OnCounter inputs",
        "CDL.Logical.Latch inputs",
        "CDL.Logical.Proof inputs",
        "CDL.Logical.TimerAccumulating inputs",
        "CDL.Logical.Toggle inputs",
        "CDL.Logical.TrueHoldWithReset inputs",
        "CDL.Psychrometrics.DewPoint_TDryBulPhi inputs",
        "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi inputs",
        "CDL.Psychrometrics.WetBulb_TDryBulPhi inputs",
        "CDL.Reals.Line inputs",
        "CDL.Reals.PID inputs",
    ];
    assert_eq!(
        exposed, expected,
        "the set of sides that reorder silently under the reference generator's sort changed"
    );
}

/// `TrueHoldWithReset`'s declared names are checked against its *behaviour*, since nothing else can.
///
/// It is the one entry in the table with no vendored `.mo` — the class is absent from Buildings
/// master, and `logical_timing.rs` records the CDL specification's §7.6 catalog as its oracle — so
/// `fixture_port_order` skips it and a transcription check is impossible. That matters more than it
/// would for another class: `u` and `clr` are both `Boolean` and `clr` sorts first, so the class is
/// one of the twelve silently exposed sides, and a WRONG name here would not fall back safely. It
/// would match one port and not the other, and a valid document would be rejected outright.
///
/// So the name is pinned to what the block does rather than to a source listing: driving the port
/// declared `clr` must clear the held output. If the two names were ever transposed, the port
/// asserted to clear would be the one that sets, and this fails.
#[test]
fn true_hold_with_reset_names_match_its_behaviour() {
    use crate::{Block, Ctx, NoopDiagnostics, TrueHoldWithReset};
    use oce_model::Value;

    let named = port_names("CDL.Logical.TrueHoldWithReset").expect("named");
    assert_eq!(named.inputs, ["u", "clr"]);
    let clr_idx = named
        .inputs
        .iter()
        .position(|n| *n == "clr")
        .expect("a clear input is declared");

    let block = TrueHoldWithReset { duration: 10.0 };
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    let diag = NoopDiagnostics;
    let emit = |region: &[u64], t: f64, inputs: &[Value]| -> Vec<Value> {
        let cx = Ctx::new(t, &diag);
        let mut out = Vec::new();
        block.emit_from_state(&cx, inputs, region, &mut |_, val| out.push(val));
        out
    };

    // Raise the set input and hold it: the output latches true.
    let mut inputs = vec![Value::Boolean(false); named.inputs.len()];
    inputs[1 - clr_idx] = Value::Boolean(true);
    let held = emit(&region, 0.0, &inputs);
    {
        let cx = Ctx::new(0.0, &diag);
        block.update_state(&cx, &inputs, &mut region);
    }
    assert!(
        matches!(held[0], Value::Boolean(true)),
        "the port declared `u` must set the held output, got {:?}",
        held[0]
    );

    // Now raise the port declared `clr`. If the declared names were transposed, this would be the
    // set input and the output would stay true.
    inputs[clr_idx] = Value::Boolean(true);
    let cleared = emit(&region, 1.0, &inputs);
    assert!(
        matches!(cleared[0], Value::Boolean(false)),
        "the port declared `clr` must clear the held output well inside the 10 s hold, got {:?}",
        cleared[0]
    );
}
