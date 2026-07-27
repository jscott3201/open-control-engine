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
//! This audit closes that gap for the checked-in corpus by comparing each instance's port list
//! against [`tools/reference-catalog/cdl-port-order.json`], which records the interface
//! declaration order of upstream Modelica source at the pinned reference commit.
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
//! The pins are 2135 checked / 37 skipped. A revision in between reported 2112/60 and claimed the
//! 23-side difference was vacuous array comparisons being correctly reclassified. That was wrong
//! in the dangerous direction: the generator OR-ed the registry's `width_driven` flag into BOTH
//! direction flags, so a class with an array input and a scalar output — `MultiSum` — had its
//! scalar output side skipped. Those 23 sides are real, checkable, and can fail. Array-ness is now
//! derived per DECLARATION, from the subscript on the connector itself.
//!
//! Also unchecked: parameter names and defaults, and composite wiring.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Ports of one class, in upstream declaration order.
struct ClassPorts {
    inputs: Vec<String>,
    outputs: Vec<String>,
    input_kinds: Vec<String>,
    output_kinds: Vec<String>,
    /// Upstream declares an array-valued connector (`u[nin]`) on that side. We flatten those into
    /// N scalars, so names cannot correspond 1:1.
    ///
    /// Recorded **per direction and as a declared fact**. Inferring it from an arity mismatch made
    /// every genuine arity disagreement look like an array port and hid 21 real wirings from an
    /// earlier revision. Marking the whole class exempt instead discarded checkable coverage:
    /// `MultiSum` has an array input but a scalar output `y`, which is verifiable.
    inputs_are_array: bool,
    outputs_are_array: bool,
}

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/oce-cxf.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root")
}

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
/// # What this does NOT check, and why the limits are load-bearing
///
/// **1. Names are never validated, in any direction.** The registry stores port **kinds** only —
/// that absence is the very gap this audit exists to cover. So the check catches a table whose
/// *arity or kinds* are wrong (it caught three extraction bugs: a nested-block leak, `end if`
/// miscounted as a class close, and `model` vs `block`) and nothing else about the names. Any
/// **coordinated rename passes**: change `Reals.PID`'s inputs to `zzz`/`aaa` in both the table and
/// all 8 fixture instances and this suite stays green — the names need not even be plausible, let
/// alone upstream's. Reordering two same-kind ports is just the narrowest case of that, not the
/// boundary. Name integrity rests on review of the table's diff plus the recorded provenance.
///
/// **2. The 28 array-port classes get no registry check at all.** Upstream declares one array
/// connector (`u[nin]`) where we carry N flattened scalars, so arity legitimately differs and
/// comparison is meaningless — but that means their table rows are unverified by anything here,
/// including on kinds.
///
/// Closing (1) means vendoring the ~768 KB of upstream BSD-licensed `.mo` sources and deriving the
/// table at test time, leaving no table to bless. That is an open decision, not an oversight.
fn cross_check_against_registry(table: &BTreeMap<String, ClassPorts>) {
    let mut disagreements = Vec::new();
    let mut compared = 0usize;
    let mut exempt_array = 0usize;
    let mut missing_from_registry = Vec::new();
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
        exempt_array, 28,
        "array-exempt class count changed. These classes receive NO registry check — not even on \
         kinds — so growing the set silently shrinks coverage."
    );
}

fn load_table() -> BTreeMap<String, ClassPorts> {
    let path = repo_root()
        .join("tools")
        .join("reference-catalog")
        .join("cdl-port-order.json");
    let raw =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&raw).expect("cdl-port-order.json is valid JSON");
    assert_eq!(
        doc["schema"], "oce-cdl-port-order-v2",
        "unexpected port-order table schema"
    );

    let field = |v: &Value, key: &str| -> Vec<String> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .map(|p| p[key].as_str().expect("port field").to_owned())
                    .collect()
            })
            .unwrap_or_default()
    };

    let table: BTreeMap<String, ClassPorts> = doc["classes"]
        .as_array()
        .expect("classes array")
        .iter()
        .map(|c| {
            (
                c["class_path"].as_str().expect("class_path").to_owned(),
                ClassPorts {
                    inputs: field(&c["inputs"], "name"),
                    outputs: field(&c["outputs"], "name"),
                    input_kinds: field(&c["inputs"], "kind"),
                    output_kinds: field(&c["outputs"], "kind"),
                    inputs_are_array: c["inputs_are_array"].as_bool().unwrap_or(false),
                    outputs_are_array: c["outputs_are_array"].as_bool().unwrap_or(false),
                },
            )
        })
        .collect();

    // Never trust the table on its own word — see the function's rustdoc.
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
        checked, 2135,
        "port-list volume changed: expected 2135 comparisons, made {checked}. \
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
