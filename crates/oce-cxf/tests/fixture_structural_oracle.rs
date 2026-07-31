//! Conformance audit: every G36 fixture is structurally verified against the vendored
//! modelica-json oracle, or excluded with a recorded reason — never silently passed.
//!
//! # Why this exists
//!
//! The fixtures are the inputs to every conformance test in the workspace: Tier-2
//! goldens and Tier-A oracles all derive from them, so a structurally wrong fixture
//! makes the whole suite validate the wrong sequence, silently and permanently. This
//! audit closes the "a wrong fixture produces a self-consistent wrong golden" gap by
//! comparing each fixture against an independent translation of the same upstream
//! class: modelica-json's CXF output, vendored under
//! `third_party/modelica-buildings-cdl/cxf/` at the Buildings and modelica-json commits
//! pinned and verified by `third_party_manifest`, with the matching `.mo` sources beside
//! it for conditional resolution.
//!
//! The comparison (see `structural_oracle/`) flattens the oracle's composite hierarchy,
//! resolves every conditional — instance, ancestor-composite, and connector — against
//! the fixture's own parameter values on BOTH sides, canonicalizes array instances and
//! vector ports using the oracle's own declarations, and compares instances and
//! undirected edges. Orientation flips are counted separately: CXF §8.2 admits either
//! endpoint as the `isConnectedTo` subject (see `resolve_connection_orientation.rs`).
//!
//! # What this is, and what it is not
//!
//! Input hygiene, not code coverage: no engine, resolver, or shipping code path is
//! exercised. It guards the validity of the tests that test production code. The
//! verdict table is a bit-exact golden; excluded fixtures (our own compositions with no
//! upstream class; the AirEconomizerHighLimits parameter-specialized reductions) are
//! listed with reasons and are not passes.
//!
//! The co-resident `golden_provenance` audit verifies that every
//! `oce-conformance` G36 determinism provenance record is paired with a CSV and
//! that its `content_sha256` matches the checked-in CSV bytes.
//!
//! # Regenerating
//!
//! ```text
//! OCE_BLESS=1 cargo test -p oce-cxf --test fixture_structural_oracle verdict_table
//! # Regenerates tests/fixtures/golden/structural_oracle_verdicts.txt.
//! OCE_BLESS=1 cargo test -p oce-cxf --test fixture_structural_oracle checked_in_manifest_bytes_equal_fresh_render
//! # Regenerates tools/reference-catalog/modelica-buildings-cdl.hash-manifest.json.
//! ```
//!
//! Bless only after reading the diff: a verdict change is a finding about a fixture or
//! about this audit, never routine churn.

mod bless;
mod golden_provenance;
mod hooks_truthiness;
mod structural_oracle;
mod third_party_manifest;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;
use structural_oracle::compare::{Analysis, analyse};
use structural_oracle::conditions::MoIndex;
use structural_oracle::flatten::Ctx;
use structural_oracle::{ManifestEntry, fixture_graph, load_manifest};

const GOLDEN_REL: &str = "tests/fixtures/golden/structural_oracle_verdicts.txt";

/// The two differential high-limits fixtures are live pass-through specializations
/// (`retAirIdentity`, AddParameter p=0 wiring TRet→TCut), not constant folds — the
/// exclusion reason must say which reduction each fixture actually is.
const PASSTHROUGH_SPECIALIZED: &[&str] = &[
    "generic_air_economizer_high_limits_ashrae_differential",
    "generic_air_economizer_high_limits_title24_differential_offset_0",
];

fn fixture_name(path: &str) -> &str {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("fixture stem")
}

fn exclusion_reason(name: &str, entry: &ManifestEntry) -> Option<&'static str> {
    if entry.upstream_class.is_none() {
        return Some("own composition, no upstream class");
    }
    if PASSTHROUGH_SPECIALIZED.contains(&name) {
        return Some(
            "parameter-specialized pass-through reduction of AirEconomizerHighLimits \
             (retAirIdentity carries TRet to TCut); structurally unverifiable by design",
        );
    }
    if name.starts_with("generic_air_economizer_high_limits_") {
        return Some(
            "parameter-specialized constant fold of AirEconomizerHighLimits; values \
             verified against upstream constants; structurally unverifiable by design",
        );
    }
    None
}

fn render(rows: &SuiteRows, excluded: &ExcludedRows) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<46} {:>9} {:>11} {:>4} {:>6} {:>4} {:>5} {:>5} {:>6} {:>5} {:>5} {:>3}  verdict",
        "fixture",
        "inst",
        "edges",
        "flip",
        "absent",
        "miss",
        "dangl",
        "extra",
        "clsmis",
        "ronly",
        "oonly",
        "unk"
    );
    let mut verdicts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (name, a) in rows {
        *verdicts.entry(a.verdict()).or_default() += 1;
        let _ = writeln!(
            out,
            "{:<46} {:>4}/{:<4} {:>5}/{:<5} {:>4} {:>6} {:>4} {:>5} {:>5} {:>6} {:>5} {:>5} {:>3}  {}",
            name,
            a.ourinst,
            a.required,
            a.matched,
            a.refedges,
            a.flipped,
            a.absent_verified,
            a.missing.len(),
            a.dangling.len(),
            a.extra.len(),
            a.clsmis.len(),
            a.refonly.len(),
            a.ouronly.len(),
            a.unknowns(),
            a.verdict()
        );
    }
    let _ = write!(out, "\nVERDICTS:");
    for (v, n) in &verdicts {
        let _ = write!(out, " {v}={n}");
    }
    let _ = writeln!(out, "   EXCLUDED={}", excluded.len());
    let _ = writeln!(out, "\nEXCLUDED (never counted as passes):");
    for (name, reason) in excluded {
        let _ = writeln!(out, "  {name}: {reason}");
    }
    for (name, a) in rows {
        if a.verdict() == "EXACT" {
            continue;
        }
        let _ = writeln!(out, "\n--- {name}  [{}]", a.verdict());
        if !a.folds.is_empty() {
            let _ = writeln!(
                out,
                "   excluded-subtree: {:?} ({} ref inst, {} our nodes)",
                a.folds, a.fold_inst, a.fold_ours
            );
        }
        for (p, ann) in &a.missing {
            let _ = writeln!(out, "   MISSING (required): {p}  [{ann}]");
        }
        for p in &a.dangling {
            let _ = writeln!(out, "   DANGLING (false-arm instance present): {p}");
        }
        for p in &a.extra {
            let _ = writeln!(out, "   extra: {p}");
        }
        for (p, want, got) in &a.clsmis {
            let _ = writeln!(out, "   clsmis: {p} oracle={want} ours={got}");
        }
        for (x, y) in &a.refonly {
            let _ = writeln!(out, "   refonly: {x}  ~  {y}");
        }
        for (x, y) in &a.ouronly {
            let _ = writeln!(out, "   ouronly: {x}  ~  {y}");
        }
        for p in &a.unknown_inst {
            let _ = writeln!(out, "   unknown-cond instance: {p}");
        }
        for (x, y) in &a.unknown_edges {
            let _ = writeln!(out, "   unknown-cond edge: {x}  ~  {y}");
        }
        for p in &a.unknown_conn {
            let _ = writeln!(out, "   unknown-cond connector (material): {p}");
        }
        if !a.unknown_conn_note.is_empty() {
            let _ = writeln!(
                out,
                "   (note: {} unevaluable connector condition(s) immaterial — every touching edge dead via its other endpoint: {:?})",
                a.unknown_conn_note.len(),
                a.unknown_conn_note
            );
        }
    }
    out
}

/// Per-fixture analyses, in manifest order.
type SuiteRows = Vec<(String, Analysis)>;
/// Excluded fixtures with their recorded reasons.
type ExcludedRows = Vec<(String, &'static str)>;

fn run_suite() -> (SuiteRows, ExcludedRows) {
    let manifest = load_manifest();
    let mut ctx = Ctx::default();
    let mut mo = MoIndex::default();
    let mut rows = Vec::new();
    let mut excluded = Vec::new();
    for (path, entry) in &manifest {
        let name = fixture_name(path).to_owned();
        match exclusion_reason(&name, entry) {
            Some(reason) => excluded.push((name, reason)),
            None => {
                let class = entry
                    .upstream_class
                    .as_deref()
                    .expect("comparable ⇒ mapped");
                let graph = fixture_graph(path);
                rows.push((name, analyse(&mut ctx, &mut mo, class, &graph, &entry.root)));
            }
        }
    }
    (rows, excluded)
}

/// The audit itself: the full per-fixture verdict table, bit-exact against the golden.
#[test]
fn verdict_table_matches_golden() {
    let (rows, excluded) = run_suite();
    let rendered = render(&rows, &excluded);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_REL);
    if bless::enabled() {
        std::fs::write(&path, &rendered).expect("write golden");
        return;
    }
    let want = std::fs::read_to_string(&path)
        .expect("golden snapshot missing — regenerate with OCE_BLESS=1");
    assert_eq!(
        rendered, want,
        "structural-oracle verdicts drifted; a verdict change is a finding, not churn"
    );
}

// ---- mutation battery: the audit must catch each planted defect class, and the
// ---- unmutated control must stay clean. A differ that cannot fail is vacuous.

fn trim_fixture() -> (String, ManifestEntry, Vec<Value>) {
    let manifest = load_manifest();
    let path = manifest
        .keys()
        .find(|p| p.ends_with("trim_and_respond_have_hol_false.jsonld"))
        .expect("trim fixture in manifest")
        .clone();
    let entry = manifest[&path].clone();
    let graph = fixture_graph(&path);
    (path, entry, graph)
}

fn analyse_graph(entry: &ManifestEntry, graph: &[Value]) -> Analysis {
    let mut ctx = Ctx::default();
    let mut mo = MoIndex::default();
    analyse(
        &mut ctx,
        &mut mo,
        entry.upstream_class.as_deref().expect("mapped"),
        graph,
        &entry.root,
    )
}

fn node_index(graph: &[Value], suffix: &str) -> usize {
    graph
        .iter()
        .position(|n| {
            n.get("@id")
                .and_then(Value::as_str)
                .is_some_and(|id| id.ends_with(suffix))
        })
        .unwrap_or_else(|| panic!("no node with id suffix {suffix}"))
}

#[test]
fn unmutated_control_is_exact() {
    let (_, entry, graph) = trim_fixture();
    let a = analyse_graph(&entry, &graph);
    assert_eq!(a.verdict(), "EXACT", "control drifted: {a:?}");
}

#[test]
fn retargeted_edge_is_reported_on_both_sides() {
    let (_, entry, mut graph) = trim_fixture();
    let i = node_index(&graph, ".notHol.y");
    let target = graph[i]["S231:isConnectedTo"].clone();
    let root = format!("{}.", entry.root);
    let retarget = |t: &mut Value| {
        t["@id"] = Value::String(format!("{root}swiHol.u1"));
    };
    match &mut graph[i]["S231:isConnectedTo"] {
        Value::Array(a) => retarget(&mut a[0]),
        other => retarget(other),
    }
    assert_ne!(
        graph[i]["S231:isConnectedTo"], target,
        "mutation did not apply"
    );
    let a = analyse_graph(&entry, &graph);
    assert!(
        !a.refonly.is_empty() && !a.ouronly.is_empty(),
        "retarget invisible: {a:?}"
    );
}

#[test]
fn deleted_instance_is_reported_missing() {
    let (_, entry, graph) = trim_fixture();
    let root = format!("{}.", entry.root);
    let doomed = format!("{root}lat");
    let before = graph.len();
    let graph: Vec<Value> = graph
        .into_iter()
        .filter(|n| {
            let id = n.get("@id").and_then(Value::as_str).unwrap_or_default();
            id != doomed && !id.starts_with(&format!("{doomed}."))
        })
        .map(|mut n| {
            if let Some(Value::Array(targets)) = n.get_mut("S231:isConnectedTo") {
                targets.retain(|t| {
                    !t.get("@id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| id.starts_with(&format!("{doomed}.")))
                });
            }
            n
        })
        .collect();
    assert!(graph.len() < before, "mutation did not apply");
    let a = analyse_graph(&entry, &graph);
    assert!(
        a.missing.iter().any(|(p, _)| p == "lat"),
        "deleted lat not reported missing: {a:?}"
    );
}

#[test]
fn ghost_instance_is_reported_extra() {
    let (_, entry, mut graph) = trim_fixture();
    graph.push(serde_json::json!({
        "@id": format!("{}.zzghost", entry.root),
        "@type": "http://example.org#Buildings.Controls.OBC.CDL.Logical.Not",
    }));
    let a = analyse_graph(&entry, &graph);
    assert!(
        a.extra.iter().any(|p| p == "zzghost"),
        "ghost not reported extra: {a:?}"
    );
}

#[test]
fn flipped_parameter_exercises_both_resolution_directions() {
    // have_hol=true makes truHol (declared conditional, now active) required-but-unwired
    // (refonly), and fal (declared '!have_hol', now inactive) resolves away while its
    // edge remains in the document (ouronly)
    let (_, entry, mut graph) = trim_fixture();
    let i = node_index(&graph, ".have_hol");
    assert_eq!(
        graph[i]["S231:value"],
        Value::Bool(false),
        "mutation anchor drifted"
    );
    graph[i]["S231:value"] = Value::Bool(true);
    let a = analyse_graph(&entry, &graph);
    assert!(
        a.refonly.len() >= 2,
        "activated truHol wiring not demanded: {a:?}"
    );
    assert!(
        !a.ouronly.is_empty(),
        "deactivated fal's edge not orphaned: {a:?}"
    );
}

#[test]
fn swapped_class_is_reported_as_mismatch() {
    let (_, entry, mut graph) = trim_fixture();
    let i = node_index(&graph, ".lat");
    graph[i]["@type"] =
        Value::String("http://example.org#Buildings.Controls.OBC.CDL.Logical.Or".into());
    let a = analyse_graph(&entry, &graph);
    assert!(
        a.clsmis.iter().any(|(p, _, _)| p == "lat"),
        "class swap not reported: {a:?}"
    );
}

// ---- vector-port canonicalization hazard, pinned directly (review finding 1):
// ---- a numeric suffix collapses to its stem ONLY when the oracle inventory declares
// ---- the stem. The 46-fixture corpus cannot distinguish a blind-stripping regression,
// ---- so these unit cases are the coverage, not the golden.

fn canon_test_flat() -> structural_oracle::flatten::Flat {
    let mut flat = structural_oracle::flatten::Flat::default();
    // an And-like instance: u1/u2/y are real scalar ports, no vector stem 'u' exists
    flat.inst.insert(
        "logSwi".into(),
        "Buildings.Controls.OBC.CDL.Logical.Or".into(),
    );
    flat.inv.insert(
        "logSwi".into(),
        ["u1", "u2", "y"].into_iter().map(str::to_owned).collect(),
    );
    // a replicator-like instance: 'y' is a declared vector port
    flat.inst.insert(
        "booRep".into(),
        "Buildings.Controls.OBC.CDL.Routing.BooleanScalarReplicator".into(),
    );
    flat.inv.insert(
        "booRep".into(),
        ["u", "y"].into_iter().map(str::to_owned).collect(),
    );
    // root array port
    flat.topports.insert("yRelFan".into());
    flat
}

#[test]
fn declared_digit_ported_name_is_never_collapsed() {
    let flat = canon_test_flat();
    let id = |p: &str| p.to_owned();
    // logSwi.u2 is a genuine distinct port: it must survive canonicalization verbatim
    assert_eq!(
        structural_oracle::compare::canon_endpoint(&flat, &id, "logSwi.u2"),
        "logSwi.u2"
    );
}

#[test]
fn undeclared_digit_ported_name_must_not_alias_its_stem() {
    let flat = canon_test_flat();
    let id = |p: &str| p.to_owned();
    // u3 is not declared and neither is a vector stem 'u': blind stripping would
    // silently alias it onto a port that does not exist — it must stay visible as-is
    assert_eq!(
        structural_oracle::compare::canon_endpoint(&flat, &id, "logSwi.u3"),
        "logSwi.u3"
    );
}

#[test]
fn vector_port_element_collapses_to_its_declared_stem() {
    let flat = canon_test_flat();
    let id = |p: &str| p.to_owned();
    assert_eq!(
        structural_oracle::compare::canon_endpoint(&flat, &id, "booRep.y3"),
        "booRep.y"
    );
    // root-level array ports use the underscore spelling
    assert_eq!(
        structural_oracle::compare::canon_endpoint(&flat, &id, "yRelFan_3"),
        "yRelFan"
    );
}
