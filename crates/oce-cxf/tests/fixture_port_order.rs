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
//! Blocks whose upstream interface is an **array port** (`u[nin]`) are skipped: we flatten those
//! into N scalar connectors, so the names cannot correspond one-to-one. That leaves the
//! `Reals.MatrixGain` instances unverified. Skips are asserted against an exact expected count
//! rather than merely tolerated, so a change that silently widens the skip set fails the test.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Ports of one class, in upstream declaration order.
struct ClassPorts {
    inputs: Vec<String>,
    outputs: Vec<String>,
}

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/oce-cxf.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root")
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
        doc["schema"], "oce-cdl-port-order-v1",
        "unexpected port-order table schema"
    );

    let names = |v: &Value| -> Vec<String> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .map(|p| p["name"].as_str().expect("port name").to_owned())
                    .collect()
            })
            .unwrap_or_default()
    };

    doc["classes"]
        .as_array()
        .expect("classes array")
        .iter()
        .map(|c| {
            (
                c["class_path"].as_str().expect("class_path").to_owned(),
                ClassPorts {
                    inputs: names(&c["inputs"]),
                    outputs: names(&c["outputs"]),
                },
            )
        })
        .collect()
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

            for (key, expected, side) in [
                ("S231:hasInput", &want.inputs, "inputs"),
                ("S231:hasOutput", &want.outputs, "outputs"),
            ] {
                let Some(got) = refs(node, key) else { continue };
                // Array-typed upstream port flattened by us into N scalars: names cannot
                // correspond one-to-one. Counted, not silently dropped.
                if got.len() != expected.len() {
                    skipped_array += 1;
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
