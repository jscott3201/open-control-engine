//! Conformance audit: every fixture lists its block ports in upstream CDL **declaration order**.
//!
//! # Why this exists
//!
//! The resolver assigns a block's port positions from the CXF document's `hasInput`/`hasOutput`
//! **array order** (`resolve/mod.rs`). Two guards sit downstream, and neither closes this gap:
//!
//! - the arity guard compares port *counts* against the resolved class signature;
//! - `oce-validate`'s `check_ports_dir` compares each position's *kind* against the signature.
//!
//! A transposition between two ports of the **same kind** passes both and silently computes a
//! different answer. 30 of the 133 registered blocks are exposed, with 282 instances live across
//! 33 of the 46 G36 fixtures. The severe cases are asymmetric: `Reals.PID` with `u_s`/`u_m`
//! transposed inverts the control action, and `Logical.Latch` with `u`/`clr` transposed inverts
//! the latch. Neither is a type error, so nothing else in the workspace can see it.
//!
//! This audit closes that gap for the checked-in corpus by deriving each class's interface
//! declaration order from **vendored upstream Modelica source** at the pinned reference commit
//! (`third_party/modelica-buildings-cdl/`) and comparing every fixture instance's port list
//! against it. The derivation runs here, at test time; there is no catalog artifact in between.
//!
//! # What this is, and what it is not
//!
//! It is **input hygiene, not code coverage.** No engine, resolver, or shipping code path is
//! exercised here — it reads the fixture documents and a lookup table and compares strings. By
//! the four-pillar standard in `TESTING.md` this is not coverage of anything.
//!
//! It matters one level up: the fixtures are the *inputs to every conformance test in the
//! workspace*. Tier-2 goldens and Tier-A oracles are all derived from them. A transposed fixture
//! does not fail anything — it makes the whole suite validate the wrong sequence, silently and
//! permanently. This guards the **validity of the tests that test production code**.
//!
//! # Why it gates
//!
//! Its value is entirely future-tense: the corpus is verified clean today, so it stays silent
//! until someone adds or edits a fixture — which is exactly when a person will not remember to
//! run it by hand. It costs ~1.4 s inside the existing gate job (`.agents/gate.sh`), so it earns
//! that place. Run it directly with:
//!
//! ```text
//! cargo nextest run -p oce-cxf --locked -E 'binary(fixture_port_order)'
//! ```
//!
//! Note the nextest selector is `binary(...)`, not `test(...)`: `fixture_port_order` is the test
//! binary; the test function is `every_fixture_lists_ports_in_upstream_declaration_order`.
//!
//! # What it does NOT check
//!
//! Sides whose upstream interface is an **array port** (`u[nin]`) are skipped: we flatten those
//! into N scalar connectors, so names cannot correspond one-to-one. Skips are asserted against an
//! exact count rather than tolerated, so a change that silently widens the skip set fails.
//!
//! A class can also shed *some* connectors without shedding all of them, and that is invisible to
//! every per-class check here — `Reals.Sort` losing `yIdx` keeps its other ports, keeps the array
//! flag `y[nin]` gives it, and is skipped by the registry cross-check and the fixture comparison
//! alike. The 175-input / 144-output totals in `cdl_source` are what close that case; they answer
//! to no exemption.
//!
//! Totals are a *counting* invariant, so they hold only while a port cannot be forged to offset
//! one that was lost. Two routes to forging one are shut in `cdl_source`: comment and string
//! bodies are blanked before scanning, and a multi-component clause is refused rather than read
//! in part. What remains is a forgery written as ordinary code, which is not something this test
//! can distinguish — the control for that is the `diff` against upstream, not this file.
//!
//! The pins are 2129 checked / 37 skipped. A revision in between reported 2112/60 and claimed the
//! 23-side difference was vacuous array comparisons being correctly reclassified. That was wrong
//! in the dangerous direction: the generator OR-ed the registry's `width_driven` flag into BOTH
//! direction flags, so a class with an array input and a scalar output — `MultiSum` — had its
//! scalar output side skipped. Those 23 sides are real, checkable, and can fail. Array-ness is now
//! derived per DECLARATION, from the subscript on the connector itself.
//!
//! Also unchecked: parameter names and defaults, and composite wiring.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

mod cdl_source;

use cdl_source::ClassPorts;

/// Cross-check the table against the **shipping block registry** before trusting a word of it.
///
/// The registry is a genuinely independent artifact: it is compiled shipping code
/// (`oce_blocks::lookup` → `resolved_signature`), authored separately from this table and
/// exercised by every other test in the workspace. Agreeing with it on arity *and* kind means a
/// hand-edit to the table has to also be a correct edit, which is the only property that matters.
///
/// Coverage is asserted, not hoped for: the counts below pin how many classes actually reach the
/// comparison, so a lookup that silently starts returning `None` — a class renamed out of the
/// registry, say — fails here instead of quietly shrinking the check to nothing.
///
/// Names are checked too, for the 104 classes that declare them
/// (`oce_blocks::port_names`). That closes the **coordinated rename**, which two earlier revisions
/// of this file documented as passing: renaming a port in the reference data *and* in every fixture
/// instance used to leave the suite green, for a name upstream never used. It cannot now — the
/// registry is a third artifact that has to be renamed as well, and it is shipping code rather than
/// test data.
///
/// # What this does NOT check, and why the limits are load-bearing
///
/// **1. Agreement is not independence.** The registry's names were transcribed from CDL, the same
/// place the vendored source comes from, so this comparison detects **drift** between two
/// artifacts — it is not an oracle and would not catch a name that was wrong in both from the
/// start. What gives the names teeth is the resolver binding against them, where a wrong name stops
/// matching real documents.
///
/// **2. The 28 array-port classes get no registry check at all.** Upstream declares one array
/// connector (`u[nin]`) where we carry N flattened scalars, so arity legitimately differs and
/// comparison is meaningless — but that means their interfaces are unverified by anything here,
/// including on kinds and including on names. The vendored source is still their authority.
fn cross_check_against_registry(table: &BTreeMap<String, ClassPorts>) {
    let mut disagreements = Vec::new();
    let mut compared = 0usize;
    let mut exempt_array = 0usize;
    let mut missing_from_registry = Vec::new();
    let mut named_compared = 0usize;
    let mut unnamed_in_registry = Vec::new();
    for (class_path, ports) in table {
        if ports.inputs_are_array || ports.outputs_are_array {
            exempt_array += 1;
            continue;
        }
        let Some(entry) = oce_blocks::lookup(class_path) else {
            missing_from_registry.push(class_path.as_str());
            continue;
        };
        compared += 1;
        // Default parameters: every class compared here is non-array, so its signature is fixed
        // and does not depend on a width parameter.
        let probe = (entry.make)(&oce_model::ParamTable::default());
        let sig = probe.resolved_signature();
        let kind_of = |k: &oce_blocks::PortKind| match k {
            oce_blocks::PortKind::Real => "Real",
            oce_blocks::PortKind::Integer => "Integer",
            oce_blocks::PortKind::Boolean => "Boolean",
        };
        let reg_in: Vec<&str> = sig.inputs.iter().map(kind_of).collect();
        let reg_out: Vec<&str> = sig.outputs.iter().map(kind_of).collect();
        let tab_in: Vec<&str> = ports.input_kinds.iter().map(String::as_str).collect();
        let tab_out: Vec<&str> = ports.output_kinds.iter().map(String::as_str).collect();
        if reg_in != tab_in || reg_out != tab_out {
            disagreements.push(format!(
                "{class_path}\n      table    : in {tab_in:?} out {tab_out:?}\n      \
                 registry : in {reg_in:?} out {reg_out:?}"
            ));
        }
        // Names, which the registry could not supply until it carried them. This is the direction
        // that used to be uncheckable: with kinds alone a coordinated rename passed, because
        // nothing in the workspace knew what upstream calls a port.
        match oce_blocks::port_names::port_names(class_path) {
            Some(named) => {
                named_compared += 1;
                let tab_in_names: Vec<&str> = ports.inputs.iter().map(String::as_str).collect();
                let tab_out_names: Vec<&str> = ports.outputs.iter().map(String::as_str).collect();
                if named.inputs != tab_in_names.as_slice()
                    || named.outputs != tab_out_names.as_slice()
                {
                    disagreements.push(format!(
                        "{class_path} (names)\n      upstream : in {tab_in_names:?} out \
                         {tab_out_names:?}\n      registry : in {:?} out {:?}",
                        named.inputs, named.outputs
                    ));
                }
            }
            None => unnamed_in_registry.push(class_path.as_str()),
        }
    }
    assert!(
        disagreements.is_empty(),
        "the port-order table disagrees with the shipping block registry on {} class(es). \
         The table is wrong, or the registry is — either way the fixture comparison below would \
         be meaningless.\n\n{}",
        disagreements.len(),
        disagreements.join("\n\n")
    );
    assert!(
        missing_from_registry.is_empty(),
        "{} table class(es) have no entry in the shipping block registry, so they were compared \
         against nothing: {:?}. Either the table names a class we do not implement, or a registry \
         `class_path` changed and this cross-check silently stopped covering it.",
        missing_from_registry.len(),
        missing_from_registry
    );
    assert_eq!(
        compared, 104,
        "registry cross-check coverage changed. It must stay pinned: a check that quietly \
         compares fewer classes still passes, and this one guards the table every other \
         assertion in this file trusts."
    );
    assert_eq!(
        named_compared, 104,
        "port-name cross-check coverage changed. All 104 registry-comparable classes carry \
         declared names; a class dropping out of `oce_blocks::port_names` stops being checked on \
         names AND silently reverts to positional binding in the resolver."
    );
    assert!(
        unnamed_in_registry.is_empty(),
        "{} class(es) reach this cross-check but declare no port names in the registry, so their \
         names were compared against nothing: {:?}. The named set is meant to be exactly the set \
         this audit can reach.",
        unnamed_in_registry.len(),
        unnamed_in_registry
    );
    assert_eq!(
        exempt_array, 28,
        "array-exempt class count changed. These classes receive NO registry check — not even on \
         kinds — so growing the set silently shrinks coverage."
    );
}

/// The derived table, checked against the shipping registry before anything trusts it.
fn load_table() -> BTreeMap<String, ClassPorts> {
    let table = cdl_source::derive_table();
    // Never trust the derived table on its own word — see `cross_check_against_registry`.
    cross_check_against_registry(&table);
    table
}

/// `http://example.org#g36.fixture.instance.u1` -> `u1`.
fn leaf(iri: &str) -> &str {
    iri.rsplit_once('#')
        .map_or(iri, |(_, t)| t)
        .rsplit_once('.')
        .map_or(iri, |(_, t)| t)
}

/// JSON-LD collapses a single-element array to a bare object; normalize both to a slice-like Vec.
fn refs(node: &Value, key: &str) -> Option<Vec<String>> {
    let v = node.get(key)?;
    let one = |r: &Value| -> Option<String> {
        r.get("@id")
            .and_then(Value::as_str)
            .map(|s| leaf(s).to_owned())
    };
    match v {
        Value::Array(a) => Some(a.iter().filter_map(one).collect()),
        Value::Object(_) => one(v).map(|s| vec![s]),
        _ => None,
    }
}

fn class_of(node: &Value) -> Option<String> {
    let t = node.get("@type")?.as_str()?;
    let idx = t.rfind("CDL.")?;
    Some(t[idx..].to_owned())
}

#[test]
fn every_fixture_lists_ports_in_upstream_declaration_order() {
    let table = load_table();
    assert_eq!(table.len(), 132, "port-order table lost classes");

    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("g36");

    let mut checked = 0usize;
    let mut skipped_array = 0usize;
    let mut fixtures = 0usize;
    let mut failures: Vec<String> = Vec::new();

    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonld"))
        .collect();
    entries.sort();

    for path in &entries {
        fixtures += 1;
        let fixture = path.file_stem().unwrap().to_string_lossy().into_owned();
        let doc: Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("read fixture"))
                .expect("fixture is valid JSON");
        let Some(graph) = doc["@graph"].as_array() else {
            continue;
        };

        for node in graph {
            let Some(cp) = class_of(node) else { continue };
            let Some(want) = table.get(&cp) else { continue };

            for (key, expected, side, is_array) in [
                (
                    "S231:hasInput",
                    &want.inputs,
                    "inputs",
                    want.inputs_are_array,
                ),
                (
                    "S231:hasOutput",
                    &want.outputs,
                    "outputs",
                    want.outputs_are_array,
                ),
            ] {
                let Some(got) = refs(node, key) else { continue };
                // Skip on the DECLARED fact for THIS side, not on an arity mismatch and not for
                // the whole class — see the field rustdoc for both failure modes.
                if is_array {
                    skipped_array += 1;
                    continue;
                }
                // A length difference on a NON-array class is a real defect, not a skip: the
                // document declares an interface the class does not have.
                if got.len() != expected.len() {
                    let id = node["@id"].as_str().unwrap_or("?");
                    failures.push(format!(
                        "{fixture} :: {cp} [{side}] ARITY\n      instance : {id}\n      \
                         document : {got:?}\n      upstream : {expected:?}"
                    ));
                    continue;
                }
                checked += 1;
                if &got != expected {
                    let id = node["@id"].as_str().unwrap_or("?");
                    failures.push(format!(
                        "{fixture} :: {cp} [{side}]\n      instance : {id}\n      \
                         document : {got:?}\n      upstream : {expected:?}"
                    ));
                }
            }
        }
    }

    assert_eq!(fixtures, 46, "fixture corpus changed size");

    // Pin the volume. Without this, a change that stops discovering ports would leave the
    // comparison vacuously green — the failure mode this whole audit exists to prevent.
    assert_eq!(
        checked, 2129,
        "port-list volume changed: expected 2129 comparisons, made {checked}. \
         If the corpus legitimately changed, update this pin deliberately."
    );
    assert_eq!(
        skipped_array, 37,
        "array-port skip count changed: expected 37, got {skipped_array}. \
         A rising skip count hides wirings from this audit — investigate before re-pinning."
    );

    assert!(
        failures.is_empty(),
        "{} fixture port list(s) disagree with upstream CDL declaration order.\n\
         A same-kind transposition is invisible to the arity and kind guards and silently \
         computes a different answer.\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
